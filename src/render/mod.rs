//! wgpu renderer: one atlas, one shader, static world buffers + per-frame dynamic geometry.

use crate::assets::Vertex;
use anyhow::{Context, Result};
use glam::{Mat4, Vec3, Vec4};
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    fog_color: [f32; 4],
    fog: [f32; 4],
    sky_top: [f32; 4],
    light: [f32; 4],
    head_pos: [f32; 4],
    head_dir: [f32; 4],
    /// Camera forward axis; w = 1 when `fog` holds PS1 depth-cue parameters.
    cam_fwd: [f32; 4],
    /// The night headlight beam (see [`NightBeam`]): right axis (w xs), forward axis (w z0),
    /// origin (w 1 = on), (zs, ..).
    beam: [[f32; 4]; 4],
    /// Its 64x64 level map, 256x16 colour-index table and 256 light-table colours, packed
    /// as bytes / words.
    night: [[u32; 4]; NIGHT_WORDS],
}

const NIGHT_WORDS: usize = 256 + 256 + 64;

/// Night driving's headlight beam as the original applies it to the track's vertices
/// (Night_NightCalc): in the beam's space (the player car's axes from its position, 1/64 m)
/// a vertex at z0 <= z < z0 + 2^(zs+6), |x| < 2^(xs+5) has its light-table index s replaced by
/// table[s][map[(z - z0) >> zs][(x + 2^(xs+5)) >> xs]].
#[derive(Clone)]
pub struct NightBeam {
    pub origin: Vec3,
    pub right: Vec3,
    pub forward: Vec3,
    pub z0: f32,
    pub xs: f32,
    pub zs: f32,
    /// The map, table and light-table colours packed (`NIGHT_WORDS` vec4s).
    pub data: Arc<Vec<[u32; 4]>>,
}

impl NightBeam {
    /// Pack the beam map (4096 bytes), colour table (4096 bytes) and light table (256 words).
    pub fn pack(map: &[u8], table: &[u8], light: &[u32]) -> Vec<[u32; 4]> {
        let words = |b: &[u8]| -> Vec<u32> { b.chunks(4).map(|c| c.iter().enumerate().fold(0u32, |w, (i, &x)| w | (x as u32) << (8 * i))).collect() };
        let mut w = words(&map[..4096.min(map.len())]);
        w.resize(1024, 0);
        let mut t = words(&table[..4096.min(table.len())]);
        t.resize(1024, 0);
        let mut l = light.to_vec();
        l.resize(256, 0);
        w.extend(t);
        w.extend(l);
        w.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
    }
}

/// The PS1's depth cueing (GTE DPCS/DPCT): vertex colours fade towards `far` by
/// IR0 = clamp((DQB + DQA * H / SZ) >> 12, 0, 4096), SZ being the view depth in GTE units.
#[derive(Clone, Copy)]
pub struct DepthCue {
    /// Far colour (8-bit per channel, as the game sets it).
    pub far: Vec3,
    pub dqa: f32,
    pub dqb: f32,
    pub h: f32,
}

pub struct FrameParams {
    /// Index ranges of the world to draw (None: all of it).
    pub world_ranges: Option<Vec<std::ops::Range<u32>>>,
    /// Subtractive primitives (PS1 semi-transparency mode 2: background - foreground), drawn
    /// over the track: the cars' ground shadows.
    pub shadows: Vec<Vertex>,
    /// Subtractive primitives drawn after the cars (their reflection maps).
    pub subtract_late: Vec<Vertex>,
    pub ambient: f32,
    /// Headlight strength (0 = off), position and direction.
    pub headlight: (f32, Vec3, Vec3),
    /// Projection * rotation-only view, for the camera-centred sky.
    pub sky_view_proj: Mat4,
    /// How many of the sky's vertices are its dome: the sky layer (the sun) goes between
    /// the dome and the horizon band after it, as the PS1 draws them.
    pub sky_dome: u32,
    pub sky_top: Vec3,
    pub view_proj: Mat4,
    pub cam_pos: Vec3,
    pub fog_color: Vec3,
    pub fog_start: f32,
    pub fog_end: f32,
    pub cam_fwd: Vec3,
    /// When set, replaces the linear fog.
    pub depth_cue: Option<DepthCue>,
    /// Night driving with the headlights on.
    pub night_beam: Option<NightBeam>,
}

const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

struct DynBuf {
    buf: wgpu::Buffer,
    cap: u64,
}

impl DynBuf {
    fn new(device: &wgpu::Device, label: &str) -> Self {
        let cap = 1 << 20;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { buf, cap }
    }
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[Vertex]) {
        let bytes: &[u8] = bytemuck::cast_slice(data);
        if bytes.len() as u64 > self.cap {
            self.cap = (bytes.len() as u64).next_power_of_two();
            self.buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: self.cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.buf, 0, bytes);
        }
    }
}

/// Winding of the front side of track polygons (with the PS1 X axis mirrored).
const TRACK_FRONT_FACE: wgpu::FrontFace = wgpu::FrontFace::Ccw;

/// How one render pass of a frame draws (split screen: a pass per view, then the HUD).
#[derive(Clone, Copy, Default)]
pub struct Pass {
    /// The view's rectangle in the window (x, y, w, h); None: the whole window.
    pub viewport: Option<[f32; 4]>,
    /// Draw over what earlier passes drew (else clear first).
    pub keep: bool,
    /// Only the 2D (`hud`), no world.
    pub no_world: bool,
    /// No background (the sky) under this pass: it draws over the earlier ones.
    pub no_sky: bool,
}

/// One layer of a front-end frame with several showroom cars: a car (its geometry, split
/// after its reflection, and projection) and the 2D drawn after it.
pub struct Layer {
    pub params: FrameParams,
    pub dynamic: Vec<Vertex>,
    pub dyn_mid: u32,
    pub over: Vec<Vertex>,
    pub over_subtract: Vec<std::ops::Range<u32>>,
}

/// One view of a split-screen frame: its frame parameters, geometry and 2D layers.
pub struct ViewDraw {
    pub viewport: [f32; 4],
    pub params: FrameParams,
    pub dynamic: Vec<Vertex>,
    pub sky_layer: Vec<Vertex>,
    pub glows: Vec<Vertex>,
    pub glows_sub: Vec<Vertex>,
    /// The view's own 2D drawn last (the weather), with its subtracting ranges.
    pub over: Vec<Vertex>,
    pub over_subtract: Vec<std::ops::Range<u32>>,
}

pub struct Renderer {
    atlas_tex: wgpu::Texture,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    pub format: wgpu::TextureFormat,
    world_pipeline: wgpu::RenderPipeline,
    track_pipeline: wgpu::RenderPipeline,
    hud_pipeline: wgpu::RenderPipeline,
    /// 2D drawn subtracting, and which ranges of the next frame's 2D vertices use it.
    hud_sub_pipeline: wgpu::RenderPipeline,
    pub hud_subtract: Vec<std::ops::Range<u32>>,
    /// 2D drawn first, under the 3D geometry (the front end's background behind its car).
    pub hud_under: Vec<Vertex>,
    /// Ranges of `hud_under` drawn subtracting.
    pub hud_under_subtract: Vec<std::ops::Range<u32>>,
    under_vb: DynBuf,
    /// 2D drawn between the dynamic geometry's first `dyn_mid` vertices and the rest (the
    /// showroom's floor between the car's reflection and the car).
    pub hud_mid: Vec<Vertex>,
    /// Ranges of `hud_mid` drawn subtracting.
    pub hud_mid_subtract: Vec<std::ops::Range<u32>>,
    pub dyn_mid: u32,
    /// 2D drawn over the sky and under the world (a race's sun), with the 2D pass's atlas.
    pub sky_layer: Vec<Vertex>,
    sky_layer_vb: DynBuf,
    /// Light glows: 2D in window pixels with a depth (z, the HUD projection's), tested
    /// against the 3D but not written, with the 2D pass's atlas.
    pub glows: Vec<Vertex>,
    glow_vb: DynBuf,
    glow_pipeline: wgpu::RenderPipeline,
    /// Glows subtracted (PS1 mode 2: the smoke's dark puffs), tested like `glows`.
    pub glows_sub: Vec<Vertex>,
    glow_sub_vb: DynBuf,
    glow_sub_pipeline: wgpu::RenderPipeline,
    /// The 2D pass's own atlas (a race's HUD recorded from the original, textured from its
    /// VRAM), when set.
    overlay_atlas: Option<(wgpu::Texture, wgpu::BindGroup)>,
    pub use_overlay_atlas: bool,
    mid_vb: DynBuf,
    sky_pipeline: wgpu::RenderPipeline,
    world_globals: wgpu::Buffer,
    hud_globals: wgpu::Buffer,
    sky_globals: wgpu::Buffer,
    sky_bg: wgpu::BindGroup,
    sky_vb: DynBuf,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    world_bg: wgpu::BindGroup,
    hud_bg: wgpu::BindGroup,
    depth: Option<(wgpu::TextureView, u32, u32)>,
    static_vb: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    dyn_vb: DynBuf,
    shadow_vb: DynBuf,
    late_vb: DynBuf,
    sub_pipeline: wgpu::RenderPipeline,
    hud_vb: DynBuf,
    pub size: (u32, u32),
}

impl Renderer {
    pub async fn new(window: Option<Arc<winit::window::Window>>, size: (u32, u32), atlas: &crate::assets::Atlas) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = match &window {
            Some(w) => Some(instance.create_surface(w.clone())?),
            None => None,
        };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .context("no GPU adapter")?;
        log::info!("adapter: {:?}", adapter.get_info().name);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await?;
        let (surface, format) = match surface {
            Some(s) => {
                let mut config = s.get_default_config(&adapter, size.0.max(1), size.1.max(1)).context("surface config")?;
                let caps = s.get_capabilities(&adapter);
                // the PS1 does its colour maths on raw 8-bit values (texture x colour / 128,
                // blending, depth cueing): render in that space, without sRGB conversion
                config.format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
                config.present_mode = wgpu::PresentMode::AutoVsync;
                s.configure(&device, &config);
                let f = config.format;
                (Some((s, config)), f)
            }
            None => (None, wgpu::TextureFormat::Rgba8Unorm),
        };

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let mk_globals = || {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: std::mem::size_of::<Globals>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let world_globals = mk_globals();
        let hud_globals = mk_globals();
        let sky_globals = mk_globals();
        let (atlas_tex, [world_bg, hud_bg, sky_bg]) = make_bind_groups(&device, &queue, &bgl, &sampler, atlas, [&world_globals, &hud_globals, &sky_globals]);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let vattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vattrs,
        };
        let mk_pipeline = |vs: &str,
                           fs: &str,
                           buffers: &[Option<wgpu::VertexBufferLayout>],
                           depth: Option<(bool, wgpu::CompareFunction)>,
                           blend: Option<wgpu::BlendState>,
                           cull: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState { cull_mode: cull, front_face: TRACK_FRONT_FACE, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: Some(depth.map(|d| d.0).unwrap_or(false)),
                    depth_compare: Some(depth.map(|d| d.1).unwrap_or(wgpu::CompareFunction::Always)),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // reversed Z (1 = near, 0 = infinitely far): even precision over the whole track
        let depth = Some((true, wgpu::CompareFunction::Greater));
        // the track is single-sided like on the PS1 (double-sided materials are emitted twice)
        // semi-transparent texels blend (fs_hud's premultiplied output: PS1 modes 0 and 1)
        let blend = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let track_pipeline = mk_pipeline("vs_main", "fs_hud", &[Some(vbl.clone())], depth, blend, Some(wgpu::Face::Back));
        let world_pipeline = mk_pipeline("vs_main", "fs_hud", &[Some(vbl.clone())], depth, blend, None);
        // background - foreground, depth-tested but not written
        let subtract = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::ReverseSubtract };
        let sub_pipeline = mk_pipeline(
            "vs_main",
            "fs_sub",
            &[Some(vbl.clone())],
            Some((false, wgpu::CompareFunction::GreaterEqual)),
            Some(wgpu::BlendState { color: subtract, alpha: subtract }),
            None,
        );
        let hud_sub_pipeline = mk_pipeline("vs_main", "fs_sub", &[Some(vbl.clone())], None, Some(wgpu::BlendState { color: subtract, alpha: subtract }), None);
        let hud_pipeline = mk_pipeline("vs_main", "fs_hud", &[Some(vbl.clone())], None, Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), None);
        let sky_pipeline = mk_pipeline("vs_sky", "fs_sky", &[], None, None, None);
        let dyn_vb = DynBuf::new(&device, "dynamic");
        let shadow_vb = DynBuf::new(&device, "shadows");
        let late_vb = DynBuf::new(&device, "subtract late");
        let hud_vb = DynBuf::new(&device, "hud");
        let under_vb = DynBuf::new(&device, "hud under");
        let mid_vb = DynBuf::new(&device, "hud mid");
        let sky_layer_vb = DynBuf::new(&device, "sky layer");
        let glow_vb = DynBuf::new(&device, "glows");
        let glow_sub_vb = DynBuf::new(&device, "glows subtracted");
        let glow_sub_pipeline = mk_pipeline("vs_main", "fs_sub", &[Some(vbl.clone())], Some((false, wgpu::CompareFunction::GreaterEqual)), Some(wgpu::BlendState { color: subtract, alpha: subtract }), None);
        let glow_pipeline = mk_pipeline("vs_main", "fs_hud", &[Some(vbl.clone())], Some((false, wgpu::CompareFunction::GreaterEqual)), Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), None);
        let sky_vb = DynBuf::new(&device, "sky");
        Ok(Self {
            device,
            queue,
            surface,
            format,
            world_pipeline,
            track_pipeline,
            hud_pipeline,
            hud_sub_pipeline,
            hud_subtract: Vec::new(),
            hud_under: Vec::new(),
            hud_under_subtract: Vec::new(),
            under_vb,
            hud_mid: Vec::new(),
            hud_mid_subtract: Vec::new(),
            dyn_mid: 0,
            sky_layer: Vec::new(),
            sky_layer_vb,
            glows: Vec::new(),
            glow_vb,
            glow_pipeline,
            glows_sub: Vec::new(),
            glow_sub_vb,
            glow_sub_pipeline,
            overlay_atlas: None,
            use_overlay_atlas: false,
            mid_vb,
            sky_pipeline,
            world_globals,
            hud_globals,
            sky_globals,
            sky_bg,
            sky_vb,
            bgl,
            sampler,
            world_bg,
            hud_bg,
            depth: None,
            static_vb: None,
            dyn_vb,
            shadow_vb,
            late_vb,
            sub_pipeline,
            hud_vb,
            size,
            atlas_tex,
        })
    }

    /// Replace the texture atlas (e.g. when switching from the menu to a race).
    pub fn set_atlas(&mut self, atlas: &crate::assets::Atlas) {
        let (tex, [w, h, s]) = make_bind_groups(&self.device, &self.queue, &self.bgl, &self.sampler, atlas, [&self.world_globals, &self.hud_globals, &self.sky_globals]);
        self.atlas_tex = tex;
        self.world_bg = w;
        self.hud_bg = h;
        self.sky_bg = s;
    }

    /// Replace the atlas' pixels (same size: written into the existing texture; the front
    /// end's per-frame atlas).
    pub fn write_atlas(&mut self, atlas: &crate::assets::Atlas) {
        if self.atlas_tex.width() != atlas.size {
            self.set_atlas(atlas);
            return;
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.atlas_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &atlas.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(atlas.size * 4), rows_per_image: Some(atlas.size) },
            wgpu::Extent3d { width: atlas.size, height: atlas.size, depth_or_array_layers: 1 },
        );
    }

    /// Upload the 2D pass's own atlas (see [`Self::use_overlay_atlas`]).
    pub fn write_overlay_atlas(&mut self, atlas: &crate::assets::Atlas) {
        let fits = self.overlay_atlas.as_ref().is_some_and(|(t, _)| t.width() == atlas.size);
        if !fits {
            let (tex, [_, h, _]) = make_bind_groups(&self.device, &self.queue, &self.bgl, &self.sampler, atlas, [&self.world_globals, &self.hud_globals, &self.sky_globals]);
            self.overlay_atlas = Some((tex, h));
            return;
        }
        let (tex, _) = self.overlay_atlas.as_ref().unwrap();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &atlas.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(atlas.size * 4), rows_per_image: Some(atlas.size) },
            wgpu::Extent3d { width: atlas.size, height: atlas.size, depth_or_array_layers: 1 },
        );
    }

    pub fn clear_world(&mut self) {
        self.static_vb = None;
    }

    pub fn set_world(&mut self, verts: &[Vertex], indices: &[u32]) {
        let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world vb"),
            contents: bytemuck::cast_slice(verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world ib"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.static_vb = Some((vb, ib, indices.len() as u32));
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.size = (w, h);
        if let Some((s, c)) = &mut self.surface {
            c.width = w;
            c.height = h;
            s.configure(&self.device, c);
        }
    }

    fn depth_view(&mut self) -> wgpu::TextureView {
        let (w, h) = self.size;
        if let Some((v, dw, dh)) = &self.depth {
            if *dw == w && *dh == h {
                return v.clone();
            }
        }
        let t = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let v = t.create_view(&Default::default());
        self.depth = Some((v.clone(), w, h));
        v
    }

    fn encode(&mut self, view: &wgpu::TextureView, p: &FrameParams, sky: &[Vertex], dynamic: &[Vertex], hud: &[Vertex]) -> wgpu::CommandBuffer {
        self.encode_pass(view, p, sky, dynamic, hud, Pass::default())
    }

    fn encode_pass(&mut self, view: &wgpu::TextureView, p: &FrameParams, sky: &[Vertex], dynamic: &[Vertex], hud: &[Vertex], pass: Pass) -> wgpu::CommandBuffer {
        let g = Globals {
            view_proj: p.view_proj.to_cols_array_2d(),
            cam_pos: p.cam_pos.extend(1.0).into(),
            fog_color: match p.depth_cue {
                Some(d) => (d.far / 256.0).extend(1.0).into(),
                None => p.fog_color.extend(1.0).into(),
            },
            fog: match p.depth_cue {
                Some(d) => [d.dqa, d.dqb, d.h, 0.0],
                None => Vec4::new(p.fog_start, p.fog_end, 0.0, 0.0).into(),
            },
            sky_top: p.sky_top.extend(1.0).into(),
            light: [p.ambient, p.headlight.0, 0.0, 0.0],
            head_pos: p.headlight.1.extend(1.0).into(),
            head_dir: p.headlight.2.extend(0.0).into(),
            cam_fwd: p.cam_fwd.extend(if p.depth_cue.is_some() { 1.0 } else { 0.0 }).into(),
            beam: match &p.night_beam {
                Some(b) => [b.right.extend(b.xs).into(), b.forward.extend(b.z0).into(), b.origin.extend(1.0).into(), [b.zs, 0.0, 0.0, 0.0]],
                None => [[0.0; 4]; 4],
            },
            night: {
                let mut n = [[0u32; 4]; NIGHT_WORDS];
                if let Some(b) = &p.night_beam {
                    for (d, s) in n.iter_mut().zip(b.data.iter()) {
                        *d = *s;
                    }
                }
                n
            },
        };
        self.queue.write_buffer(&self.world_globals, 0, bytemuck::bytes_of(&g));
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let hud_g = Globals {
            view_proj: Mat4::orthographic_rh(0.0, w, h, 0.0, -1.0, 1.0).to_cols_array_2d(),
            cam_pos: [0.0; 4],
            fog_color: [0.0; 4],
            fog: [1e9, 2e9, 0.0, 0.0],
            sky_top: [0.0; 4],
            light: [1.0, 0.0, 0.0, 0.0],
            head_pos: [0.0; 4],
            head_dir: [0.0; 4],
            cam_fwd: [0.0; 4],
            beam: [[0.0; 4]; 4],
            night: [[0; 4]; NIGHT_WORDS],
        };
        self.queue.write_buffer(&self.hud_globals, 0, bytemuck::bytes_of(&hud_g));
        let sky_g = Globals {
            view_proj: p.sky_view_proj.to_cols_array_2d(),
            cam_pos: [0.0; 4],
            fog_color: p.sky_top.extend(1.0).into(),
            fog: [1e9, 2e9, 0.0, 0.0],
            sky_top: [0.0; 4],
            light: [if p.ambient < 0.5 { p.ambient * 0.5 } else { p.ambient }, 0.0, 0.0, 0.0],
            head_pos: [0.0; 4],
            head_dir: [0.0; 4],
            cam_fwd: [0.0; 4],
            beam: [[0.0; 4]; 4],
            night: [[0; 4]; NIGHT_WORDS],
        };
        self.queue.write_buffer(&self.sky_globals, 0, bytemuck::bytes_of(&sky_g));
        self.sky_vb.upload(&self.device, &self.queue, sky);
        self.dyn_vb.upload(&self.device, &self.queue, dynamic);
        self.shadow_vb.upload(&self.device, &self.queue, &p.shadows);
        self.late_vb.upload(&self.device, &self.queue, &p.subtract_late);
        self.hud_vb.upload(&self.device, &self.queue, hud);
        self.under_vb.upload(&self.device, &self.queue, &self.hud_under);
        self.mid_vb.upload(&self.device, &self.queue, &self.hud_mid);
        self.sky_layer_vb.upload(&self.device, &self.queue, &self.sky_layer);
        self.glow_vb.upload(&self.device, &self.queue, &self.glows);
        self.glow_sub_vb.upload(&self.device, &self.queue, &self.glows_sub);
        let depth = self.depth_view();
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: if pass.keep { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(wgpu::Color::BLACK) }, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // a view of a split screen: the 3D in its rectangle, the 2D (window coordinates)
            // clipped to it
            let full = [0.0, 0.0, w, h];
            let set_vp = |rp: &mut wgpu::RenderPass, r: [f32; 4]| rp.set_viewport(r[0], r[1], r[2], r[3], 0.0, 1.0);
            let (vp3d, vp2d) = match pass.viewport {
                Some(v) => {
                    let (x0, y0) = (v[0].max(0.0).floor(), v[1].max(0.0).floor());
                    let (x1, y1) = ((v[0] + v[2]).min(w).ceil(), (v[1] + v[3]).min(h).ceil());
                    rp.set_scissor_rect(x0 as u32, y0 as u32, (x1 - x0).max(1.0) as u32, (y1 - y0).max(1.0) as u32);
                    (v, full)
                }
                None => (full, full),
            };
            rp.set_bind_group(0, &self.world_bg, &[]);
            set_vp(&mut rp, vp3d);
            if pass.no_world {
                // (only the 2D over the views drawn before)
                self.draw_hud(&mut rp, hud);
                drop(rp);
                return enc.finish();
            }
            if !pass.no_sky {
                rp.set_pipeline(&self.sky_pipeline);
                rp.draw(0..3, 0..1);
            }
            if !self.hud_under.is_empty() {
                set_vp(&mut rp, vp2d);
                rp.set_pipeline(&self.hud_pipeline);
                let under_bg = match (&self.overlay_atlas, self.use_overlay_atlas) {
                    (Some((_, bg)), true) => bg,
                    _ => &self.hud_bg,
                };
                rp.set_bind_group(0, under_bg, &[]);
                rp.set_vertex_buffer(0, self.under_vb.buf.slice(..));
                let n = self.hud_under.len() as u32;
                let mut at = 0;
                for r in self.hud_under_subtract.iter().filter(|r| r.end <= n) {
                    if r.start > at {
                        rp.set_pipeline(&self.hud_pipeline);
                        rp.draw(at..r.start, 0..1);
                    }
                    rp.set_pipeline(&self.hud_sub_pipeline);
                    rp.draw(r.clone(), 0..1);
                    at = r.end;
                }
                if at < n {
                    rp.set_pipeline(&self.hud_pipeline);
                    rp.draw(at..n, 0..1);
                }
                rp.set_bind_group(0, &self.world_bg, &[]);
                set_vp(&mut rp, vp3d);
            }
            // the dome, the sun over it, then the horizon band over the sun
            let dome = p.sky_dome.min(sky.len() as u32);
            if dome > 0 {
                rp.set_pipeline(&self.hud_pipeline);
                rp.set_bind_group(0, &self.sky_bg, &[]);
                rp.set_vertex_buffer(0, self.sky_vb.buf.slice(..));
                rp.draw(0..dome, 0..1);
                rp.set_bind_group(0, &self.world_bg, &[]);
            }
            if !self.sky_layer.is_empty() {
                set_vp(&mut rp, vp2d);
                rp.set_pipeline(&self.hud_pipeline);
                // the sky layer is always textured from the 2D pass's own atlas
                let bg = match &self.overlay_atlas {
                    Some((_, bg)) => bg,
                    None => &self.hud_bg,
                };
                rp.set_bind_group(0, bg, &[]);
                rp.set_vertex_buffer(0, self.sky_layer_vb.buf.slice(..));
                rp.draw(0..self.sky_layer.len() as u32, 0..1);
                rp.set_bind_group(0, &self.world_bg, &[]);
                set_vp(&mut rp, vp3d);
            }
            if (sky.len() as u32) > dome {
                rp.set_pipeline(&self.hud_pipeline);
                rp.set_bind_group(0, &self.sky_bg, &[]);
                rp.set_vertex_buffer(0, self.sky_vb.buf.slice(..));
                rp.draw(dome..sky.len() as u32, 0..1);
                rp.set_bind_group(0, &self.world_bg, &[]);
            }
            rp.set_pipeline(&self.track_pipeline);
            if let Some((vb, ib, n)) = &self.static_vb {
                rp.set_vertex_buffer(0, vb.slice(..));
                rp.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                match &p.world_ranges {
                    Some(ranges) => {
                        for r in ranges.iter().filter(|r| r.end <= *n && r.start < r.end) {
                            rp.draw_indexed(r.clone(), 0, 0..1);
                        }
                    }
                    None => rp.draw_indexed(0..*n, 0, 0..1),
                }
            }
            if !p.shadows.is_empty() {
                rp.set_pipeline(&self.sub_pipeline);
                rp.set_vertex_buffer(0, self.shadow_vb.buf.slice(..));
                rp.draw(0..p.shadows.len() as u32, 0..1);
            }
            rp.set_pipeline(&self.world_pipeline);
            if !dynamic.is_empty() || !self.hud_mid.is_empty() {
                let (n, mid) = (dynamic.len() as u32, self.dyn_mid.min(dynamic.len() as u32));
                rp.set_vertex_buffer(0, self.dyn_vb.buf.slice(..));
                rp.draw(0..mid, 0..1);
                if !self.hud_mid.is_empty() {
                    set_vp(&mut rp, vp2d);
                    rp.set_pipeline(&self.hud_pipeline);
                    rp.set_bind_group(0, &self.hud_bg, &[]);
                    rp.set_vertex_buffer(0, self.mid_vb.buf.slice(..));
                    let n = self.hud_mid.len() as u32;
                    let mut at = 0;
                    for r in self.hud_mid_subtract.iter().filter(|r| r.end <= n) {
                        if r.start > at {
                            rp.set_pipeline(&self.hud_pipeline);
                            rp.draw(at..r.start, 0..1);
                        }
                        rp.set_pipeline(&self.hud_sub_pipeline);
                        rp.draw(r.clone(), 0..1);
                        at = r.end;
                    }
                    if at < n {
                        rp.set_pipeline(&self.hud_pipeline);
                        rp.draw(at..n, 0..1);
                    }
                    rp.set_pipeline(&self.world_pipeline);
                    rp.set_bind_group(0, &self.world_bg, &[]);
                    rp.set_vertex_buffer(0, self.dyn_vb.buf.slice(..));
                    set_vp(&mut rp, vp3d);
                }
                rp.draw(mid..n, 0..1);
            }
            if !p.subtract_late.is_empty() {
                rp.set_pipeline(&self.sub_pipeline);
                rp.set_vertex_buffer(0, self.late_vb.buf.slice(..));
                rp.draw(0..p.subtract_late.len() as u32, 0..1);
            }
            set_vp(&mut rp, vp2d);
            if !self.glows.is_empty() {
                if let Some((_, bg)) = &self.overlay_atlas {
                    rp.set_pipeline(&self.glow_pipeline);
                    rp.set_bind_group(0, bg, &[]);
                    rp.set_vertex_buffer(0, self.glow_vb.buf.slice(..));
                    rp.draw(0..self.glows.len() as u32, 0..1);
                }
            }
            if !self.glows_sub.is_empty() {
                if let Some((_, bg)) = &self.overlay_atlas {
                    rp.set_pipeline(&self.glow_sub_pipeline);
                    rp.set_bind_group(0, bg, &[]);
                    rp.set_vertex_buffer(0, self.glow_sub_vb.buf.slice(..));
                    rp.draw(0..self.glows_sub.len() as u32, 0..1);
                }
            }
            self.draw_hud(&mut rp, hud);
        }
        enc.finish()
    }

    /// The 2D over everything: runs of normal blending between the subtracting ranges.
    fn draw_hud(&self, rp: &mut wgpu::RenderPass, hud: &[Vertex]) {
        if hud.is_empty() {
            return;
        }
        let hud_bg = match (&self.overlay_atlas, self.use_overlay_atlas) {
            (Some((_, bg)), true) => bg,
            _ => &self.hud_bg,
        };
        rp.set_bind_group(0, hud_bg, &[]);
        rp.set_vertex_buffer(0, self.hud_vb.buf.slice(..));
        let mut at = 0;
        for r in self.hud_subtract.iter().filter(|r| r.end <= hud.len() as u32) {
            if r.start > at {
                rp.set_pipeline(&self.hud_pipeline);
                rp.draw(at..r.start, 0..1);
            }
            rp.set_pipeline(&self.hud_sub_pipeline);
            rp.draw(r.clone(), 0..1);
            at = r.end;
        }
        if at < hud.len() as u32 {
            rp.set_pipeline(&self.hud_pipeline);
            rp.draw(at..hud.len() as u32, 0..1);
        }
    }

    pub fn render(&mut self, p: &FrameParams, sky: &[Vertex], dynamic: &[Vertex], hud: &[Vertex]) {
        let Some((surface, config)) = &self.surface else { return };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&self.device, config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let cmd = self.encode(&view, p, sky, dynamic, hud);
        self.queue.submit([cmd]);
        self.queue.present(frame);
    }

    /// A split-screen frame: each view in its rectangle, then the HUD over the window
    /// (`hud` with `self.hud_subtract`). The 2D layers all use the overlay atlas.
    fn encode_views(&mut self, target: &wgpu::TextureView, views: &[ViewDraw], sky: &[Vertex], hud: &[Vertex]) -> Vec<wgpu::CommandBuffer> {
        let hud_subtract = std::mem::take(&mut self.hud_subtract);
        let mut cmds = Vec::new();
        for (k, v) in views.iter().enumerate() {
            self.sky_layer = v.sky_layer.clone();
            self.glows = v.glows.clone();
            self.glows_sub = v.glows_sub.clone();
            self.hud_subtract = v.over_subtract.clone();
            let cmd = self.encode_pass(target, &v.params, sky, &v.dynamic, &v.over, Pass { viewport: Some(v.viewport), keep: k > 0, ..Default::default() });
            // (each pass's buffers and uniforms are written before it is submitted)
            self.queue.submit([cmd]);
        }
        self.hud_subtract = hud_subtract;
        if let Some(v) = views.first() {
            cmds.push(self.encode_pass(target, &v.params, sky, &[], hud, Pass { viewport: None, keep: true, no_world: true, ..Default::default() }));
        }
        cmds
    }

    /// A frame drawn as layers (front end with several cars): the first with `hud_under`
    /// and `hud_mid` as set, the rest over it, each layer's car then its 2D.
    fn encode_layers(&mut self, target: &wgpu::TextureView, layers: &[Layer]) -> Vec<wgpu::CommandBuffer> {
        let saved = (std::mem::take(&mut self.hud_subtract), self.dyn_mid);
        let mut cmds = Vec::new();
        for (k, l) in layers.iter().enumerate() {
            if k == 1 {
                self.hud_under.clear();
                self.hud_under_subtract.clear();
                self.hud_mid.clear();
                self.hud_mid_subtract.clear();
            }
            self.hud_subtract = l.over_subtract.clone();
            self.dyn_mid = l.dyn_mid;
            let cmd = self.encode_pass(target, &l.params, &[], &l.dynamic, &l.over, Pass { keep: k > 0, no_sky: k > 0, ..Default::default() });
            if k + 1 < layers.len() {
                // (each pass's buffers are written before it is submitted)
                self.queue.submit([cmd]);
            } else {
                cmds.push(cmd);
            }
        }
        (self.hud_subtract, self.dyn_mid) = saved;
        cmds
    }

    pub fn render_layers(&mut self, layers: &[Layer]) {
        let Some((surface, config)) = &self.surface else { return };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&self.device, config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let cmds = self.encode_layers(&view, layers);
        self.queue.submit(cmds);
        self.queue.present(frame);
    }

    pub fn screenshot_layers(&mut self, layers: &[Layer]) -> image::RgbaImage {
        let (tex, view) = self.shot_target();
        let cmds = self.encode_layers(&view, layers);
        self.read_back(&tex, cmds)
    }

    pub fn render_views(&mut self, views: &[ViewDraw], sky: &[Vertex], hud: &[Vertex]) {
        let Some((surface, config)) = &self.surface else { return };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&self.device, config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let cmds = self.encode_views(&view, views, sky, hud);
        self.queue.submit(cmds);
        self.queue.present(frame);
    }

    pub fn screenshot_views(&mut self, views: &[ViewDraw], sky: &[Vertex], hud: &[Vertex]) -> image::RgbaImage {
        let (tex, view) = self.shot_target();
        let cmds = self.encode_views(&view, views, sky, hud);
        self.read_back(&tex, cmds)
    }

    fn shot_target(&self) -> (wgpu::Texture, wgpu::TextureView) {
        let (w, h) = self.size;
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shot"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        (tex, view)
    }

    /// Render one frame offscreen and return RGBA8 pixels.
    pub fn screenshot(&mut self, p: &FrameParams, sky: &[Vertex], dynamic: &[Vertex], hud: &[Vertex]) -> image::RgbaImage {
        let (tex, view) = self.shot_target();
        let cmd = self.encode(&view, p, sky, dynamic, hud);
        self.read_back(&tex, vec![cmd])
    }

    fn read_back(&mut self, tex: &wgpu::Texture, cmds: Vec<wgpu::CommandBuffer>) -> image::RgbaImage {
        let (w, h) = self.size;
        let row = (w * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(cmds.into_iter().chain([enc.finish()]));
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().expect("map screenshot buffer");
        let mut img = image::RgbaImage::new(w, h);
        let bgra = matches!(self.format, wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm);
        for y in 0..h {
            for x in 0..w {
                let i = (y * row + x * 4) as usize;
                let px = if bgra { [data[i + 2], data[i + 1], data[i], 255] } else { [data[i], data[i + 1], data[i + 2], 255] };
                img.put_pixel(x, y, image::Rgba(px));
            }
        }
        img
    }
}

fn make_bind_groups(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    atlas: &crate::assets::Atlas,
    globals: [&wgpu::Buffer; 3],
) -> (wgpu::Texture, [wgpu::BindGroup; 3]) {
    let tex = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d { width: atlas.size, height: atlas.size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &atlas.rgba,
    );
    let view = tex.create_view(&Default::default());
    let groups = globals.map(|buf| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    });
    (tex, groups)
}
