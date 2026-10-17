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
    /// Camera forward axis; w = 1 when `fog` holds PS1 depth-cue parameters.
    cam_fwd: [f32; 4],
    /// x = 1: bilinear texture filtering (see [`Renderer::bilinear`]); y the last mip level
    /// sampled, z the mip level bias (see [`Renderer::mipmaps`]).
    texfilter: [f32; 4],
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

/// Texels to replace in the scene's atlas before drawing (x, y, w, h in texels; RGBA rows).
#[derive(Clone)]
pub struct AtlasWrite {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

pub struct FrameParams {
    /// Atlas texels this frame changes (the track's scrolling textures).
    pub atlas_writes: Vec<AtlasWrite>,
    /// Index ranges of the world to draw (None: all of it).
    pub world_ranges: Option<Vec<std::ops::Range<u32>>>,
    /// More of the world placed this frame (the track objects the original draws), drawn
    /// like the static world (one-sided).
    pub objects: Vec<Vertex>,
    /// Subtractive primitives (PS1 semi-transparency mode 2: background - foreground), drawn
    /// over the track: the cars' ground shadows.
    pub shadows: Vec<Vertex>,
    /// Subtractive primitives drawn after the cars (their reflection maps).
    pub subtract_late: Vec<Vertex>,
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
const ATLAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Bloom's levels: in linear light, which adds up past 1.
const BLOOM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The most levels bloom has (the first half the window's size, each half the one before).
const BLOOM_LEVELS: usize = 6;

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
    /// The scene's pipelines, for `pipes.samples` samples per pixel.
    pipes: ScenePipelines,
    /// For building them again (see [`Renderer::msaa`]).
    shader: wgpu::ShaderModule,
    layout: wgpu::PipelineLayout,
    /// MSAA: samples per pixel asked for (1: none); the scene is drawn with the most the GPU
    /// has up to that (`msaa_counts`), to `msaa_color` and resolved to the target.
    pub msaa: u32,
    msaa_counts: Vec<u32>,
    msaa_color: Option<(wgpu::TextureView, u32, u32, u32)>,
    /// Which ranges of the next frame's 2D vertices are drawn subtracting.
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
    /// Glows subtracted (PS1 mode 2: the smoke's dark puffs), tested like `glows`.
    pub glows_sub: Vec<Vertex>,
    glow_sub_vb: DynBuf,
    /// The 2D pass's own atlas (a race's HUD recorded from the original, textured from its
    /// VRAM), when set.
    overlay_atlas: Option<(wgpu::Texture, wgpu::BindGroup)>,
    pub use_overlay_atlas: bool,
    mid_vb: DynBuf,
    world_globals: wgpu::Buffer,
    hud_globals: wgpu::Buffer,
    sky_globals: wgpu::Buffer,
    sky_bg: wgpu::BindGroup,
    sky_vb: DynBuf,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    world_bg: wgpu::BindGroup,
    hud_bg: wgpu::BindGroup,
    /// Depth buffers by size and samples (the scene's, and the window's for `stats`).
    depth: Vec<(wgpu::TextureView, u32, u32, u32)>,
    static_vb: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    dyn_vb: DynBuf,
    shadow_vb: DynBuf,
    objects_vb: DynBuf,
    late_vb: DynBuf,
    hud_vb: DynBuf,
    /// Drawn over every frame shown in the window (not screenshots): the frame-rate counter.
    pub stats: Vec<Vertex>,
    stats_vb: DynBuf,
    stats_pipeline: wgpu::RenderPipeline,
    /// Bilinear filtering of the 3D scene's textures (the world, cars and sky; the 2D
    /// overlay stays sharp).
    pub bilinear: bool,
    /// The 3D scene's textures sampled from the atlas' mip levels (when it has them: see
    /// [`crate::assets::Atlas::mipmapped`]) by their size on screen, `mip_bias` levels further
    /// (> 0 blurrier, < 0 sharper).
    pub mipmaps: bool,
    pub mip_bias: f32,
    mip_pipeline: wgpu::RenderPipeline,
    mip_bgl: wgpu::BindGroupLayout,
    /// Per mip level after the first: the level above's bind group and the level's view.
    atlas_mips: Vec<(wgpu::BindGroup, wgpu::TextureView)>,
    /// The CRT filter over every frame shown in the window (not screenshots): the frame
    /// drawn to `post_target`, then through crt.wgsl to the window.
    pub crt: bool,
    crt_pipeline: wgpu::RenderPipeline,
    crt_settings: Crt,
    /// Supersampling: frames drawn at this many times the window's width and height (to
    /// `post_target`), then each window pixel the average of its samples (`down_pipeline`):
    /// no shimmering thin geometry and edges in the distance.
    pub supersample: u32,
    down_pipeline: wgpu::RenderPipeline,
    /// Bloom over every frame shown in the window (not screenshots): the frame's bright parts
    /// (`bloom_bright_pipeline`) halved in size level by level (`bloom_down_pipeline`), added
    /// back up (`bloom_up_pipeline`) and the result added to the frame on its way to the
    /// window.
    pub bloom: bool,
    bloom_settings: Bloom,
    bloom_bright_pipeline: wgpu::RenderPipeline,
    bloom_down_pipeline: wgpu::RenderPipeline,
    bloom_up_pipeline: wgpu::RenderPipeline,
    /// A black texel, bound where a pass has no bloom.
    black: wgpu::TextureView,
    crt_bgl: wgpu::BindGroupLayout,
    crt_sampler: wgpu::Sampler,
    /// The frame drawn for the CRT filter, supersampling or bloom, with the bloom's levels.
    post_target: Option<PostTarget>,
    /// The CRT filter's and bloom's settings ([`Renderer::set_crt`], [`Renderer::set_bloom`]).
    crt_params: wgpu::Buffer,
    /// The window's present mode without vsync ([`Renderer::set_vsync`]).
    no_vsync: wgpu::PresentMode,
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
        // (MSAA counts other than 4 are the adapter's own)
        let features = adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: features,
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await?;
        let mut no_vsync = wgpu::PresentMode::AutoVsync;
        let (surface, format) = match surface {
            Some(s) => {
                let mut config = s.get_default_config(&adapter, size.0.max(1), size.1.max(1)).context("surface config")?;
                let caps = s.get_capabilities(&adapter);
                // the PS1 does its colour maths on raw 8-bit values (texture x colour / 128,
                // blending, depth cueing): render in that space, without sRGB conversion
                config.format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
                config.present_mode = wgpu::PresentMode::AutoVsync;
                // without vsync (the app paces the frames): no tearing where the driver allows
                // (mailbox)
                no_vsync = [wgpu::PresentMode::Mailbox, wgpu::PresentMode::Immediate].into_iter().find(|m| caps.present_modes.contains(m)).unwrap_or(wgpu::PresentMode::AutoVsync);
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
        let pipes = ScenePipelines::new(&device, &layout, &shader, format, 1);
        let msaa_counts: Vec<u32> = [2, 4, 8, 16]
            .into_iter()
            .filter(|&n| {
                let ok = |f: wgpu::TextureFormat| {
                    if features.is_empty() {
                        n == 4
                    } else {
                        adapter.get_texture_format_features(f).flags.sample_count_supported(n)
                    }
                };
                ok(format) && ok(DEPTH)
            })
            .collect();
        log::info!("MSAA sample counts: {msaa_counts:?}");
        let dyn_vb = DynBuf::new(&device, "dynamic");
        let shadow_vb = DynBuf::new(&device, "shadows");
        let objects_vb = DynBuf::new(&device, "objects");
        let late_vb = DynBuf::new(&device, "subtract late");
        let hud_vb = DynBuf::new(&device, "hud");
        let under_vb = DynBuf::new(&device, "hud under");
        let mid_vb = DynBuf::new(&device, "hud mid");
        let sky_layer_vb = DynBuf::new(&device, "sky layer");
        let glow_vb = DynBuf::new(&device, "glows");
        let glow_sub_vb = DynBuf::new(&device, "glows subtracted");
        let sky_vb = DynBuf::new(&device, "sky");
        let stats_vb = DynBuf::new(&device, "stats");
        let stats_pipeline = mk_pipeline(&device, &layout, &shader, format, 1, "vs_main", "fs_solid", true, None, Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), None);
        let crt_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("crt"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let crt_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("crt"),
            size: std::mem::size_of::<[[f32; 4]; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let black = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("black"),
                size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: BLOOM_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let crt_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let crt_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crt"),
            source: wgpu::ShaderSource::Wgsl(include_str!("crt.wgsl").into()),
        });
        let crt_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crt"),
            bind_group_layouts: &[Some(&crt_bgl)],
            immediate_size: 0,
        });
        let mk_post = |fs: &str, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&crt_layout),
                vertex: wgpu::VertexState {
                    module: &crt_shader,
                    entry_point: Some("vs_crt"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &crt_shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let crt_pipeline = mk_post("fs_crt", format, None);
        let down_pipeline = mk_post("fs_down", format, None);
        let bloom_bright_pipeline = mk_post("fs_bloom_bright", BLOOM_FORMAT, None);
        let bloom_down_pipeline = mk_post("fs_bloom_down", BLOOM_FORMAT, None);
        let add = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        let bloom_up_pipeline = mk_post("fs_bloom_up", BLOOM_FORMAT, Some(wgpu::BlendState { color: add, alpha: add }));
        let mip_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mip"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let mip_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mip"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mip.wgsl").into()),
        });
        let mip_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mip"),
            bind_group_layouts: &[Some(&mip_bgl)],
            immediate_size: 0,
        });
        let mip_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mip"),
            layout: Some(&mip_layout),
            vertex: wgpu::VertexState {
                module: &mip_shader,
                entry_point: Some("vs_mip"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &mip_shader,
                entry_point: Some("fs_mip"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: ATLAS_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let atlas_mips = mip_chain(&device, &mip_bgl, &atlas_tex);
        let r = Self {
            device,
            queue,
            surface,
            format,
            pipes,
            shader,
            layout,
            msaa: 1,
            msaa_counts,
            msaa_color: None,
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
            glows_sub: Vec::new(),
            glow_sub_vb,
            overlay_atlas: None,
            use_overlay_atlas: false,
            mid_vb,
            world_globals,
            hud_globals,
            sky_globals,
            sky_bg,
            sky_vb,
            bgl,
            sampler,
            world_bg,
            hud_bg,
            depth: Vec::new(),
            static_vb: None,
            dyn_vb,
            shadow_vb,
            objects_vb,
            late_vb,
            hud_vb,
            stats: Vec::new(),
            stats_vb,
            stats_pipeline,
            bilinear: false,
            crt: false,
            crt_pipeline,
            crt_bgl,
            crt_sampler,
            post_target: None,
            crt_settings: Crt::default(),
            supersample: 1,
            down_pipeline,
            bloom: false,
            bloom_settings: Bloom::default(),
            bloom_bright_pipeline,
            bloom_down_pipeline,
            bloom_up_pipeline,
            black,
            crt_params,
            mipmaps: false,
            mip_bias: 0.0,
            mip_pipeline,
            mip_bgl,
            atlas_mips,
            no_vsync,
            size,
            atlas_tex,
        };
        r.gen_mips();
        Ok(r)
    }

    /// The CRT filter's settings.
    pub fn set_crt(&mut self, c: &Crt) {
        self.crt_settings = *c;
    }

    /// Bloom's settings.
    pub fn set_bloom(&mut self, b: &Bloom) {
        self.bloom_settings = *b;
    }

    /// The MSAA samples per pixel in effect: `msaa`, or the most the GPU has below it.
    fn samples(&self) -> u32 {
        self.msaa_counts.iter().copied().filter(|&n| n <= self.msaa).max().unwrap_or(1)
    }

    /// The multisampled frame the scene is drawn to with MSAA (kept for the next frames).
    fn msaa_view(&mut self, (w, h): (u32, u32), samples: u32) -> wgpu::TextureView {
        if let Some((v, tw, th, ts)) = &self.msaa_color {
            if (*tw, *th, *ts) == (w, h, samples) {
                return v.clone();
            }
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("msaa"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let v = tex.create_view(&Default::default());
        self.msaa_color = Some((v.clone(), w, h, samples));
        v
    }

    /// The supersampling factor in effect: `supersample`, as far as the GPU's largest
    /// texture allows at the window's size.
    fn scale(&self) -> u32 {
        let max = self.device.limits().max_texture_dimension_2d;
        self.supersample.clamp(1, (max / self.size.0.max(self.size.1).max(1)).max(1))
    }

    /// `Globals::texfilter` for the 3D scene.
    fn texfilter(&self) -> [f32; 4] {
        let last = if self.mipmaps { self.atlas_mips.len() as f32 } else { 0.0 };
        [self.bilinear as u32 as f32, last, self.mip_bias, 0.0]
    }

    /// Fill the atlas' mip levels from its first (after it changed).
    fn gen_mips(&self) {
        self.gen_mips_in(&[]);
    }

    /// Fill the atlas' mip levels from its first where these rectangles of it (x, y, w, h)
    /// changed (none: everywhere).
    fn gen_mips_in(&self, rects: &[[u32; 4]]) {
        if self.atlas_mips.is_empty() {
            return;
        }
        let mut enc = self.device.create_command_encoder(&Default::default());
        for (l, (src, dst)) in (1..).zip(&self.atlas_mips) {
            let size = (self.atlas_tex.width() >> l).max(1);
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mip"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if rects.is_empty() { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) } else { wgpu::LoadOp::Load },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_pipeline(&self.mip_pipeline);
            rp.set_bind_group(0, src, &[]);
            if rects.is_empty() {
                rp.draw(0..3, 0..1);
            }
            for &[x, y, w, h] in rects {
                // the level's texels the rectangle covers part of
                let (x0, y0) = ((x >> l).min(size), (y >> l).min(size));
                let (x1, y1) = ((x + w).div_ceil(1 << l).min(size), (y + h).div_ceil(1 << l).min(size));
                if x1 > x0 && y1 > y0 {
                    rp.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                    rp.draw(0..3, 0..1);
                }
            }
        }
        self.queue.submit([enc.finish()]);
    }

    /// Replace the texture atlas (e.g. when switching from the menu to a race).
    pub fn set_atlas(&mut self, atlas: &crate::assets::Atlas) {
        let (tex, [w, h, s]) = make_bind_groups(&self.device, &self.queue, &self.bgl, &self.sampler, atlas, [&self.world_globals, &self.hud_globals, &self.sky_globals]);
        self.atlas_mips = mip_chain(&self.device, &self.mip_bgl, &tex);
        self.atlas_tex = tex;
        self.world_bg = w;
        self.hud_bg = h;
        self.sky_bg = s;
        self.gen_mips();
    }

    /// Replace the atlas' pixels (same size: written into the existing texture; the front
    /// end's per-frame atlas).
    pub fn write_atlas(&mut self, atlas: &crate::assets::Atlas) {
        if self.atlas_tex.width() != atlas.size || self.atlas_tex.mip_level_count() != atlas.mip_levels {
            self.set_atlas(atlas);
            return;
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.atlas_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &atlas.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(atlas.size * 4), rows_per_image: Some(atlas.size) },
            wgpu::Extent3d { width: atlas.size, height: atlas.size, depth_or_array_layers: 1 },
        );
        self.gen_mips();
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

    /// Present at the display's refresh (the default), or as soon as a frame is ready.
    pub fn set_vsync(&mut self, on: bool) {
        if let Some((s, c)) = &mut self.surface {
            c.present_mode = if on { wgpu::PresentMode::AutoVsync } else { self.no_vsync };
            log::info!("present mode: {:?}", c.present_mode);
            s.configure(&self.device, c);
        }
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

    /// A depth buffer of this size and samples per pixel (kept for the next frames).
    fn depth_view(&mut self, (w, h): (u32, u32), samples: u32) -> wgpu::TextureView {
        if let Some((v, ..)) = self.depth.iter().find(|d| (d.1, d.2, d.3) == (w, h, samples)) {
            return v.clone();
        }
        let t = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let v = t.create_view(&Default::default());
        // (the scene's and the window's; the others are from before a resize)
        if self.depth.len() >= 2 {
            self.depth.remove(0);
        }
        self.depth.push((v.clone(), w, h, samples));
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
            cam_fwd: p.cam_fwd.extend(if p.depth_cue.is_some() { 1.0 } else { 0.0 }).into(),
            texfilter: self.texfilter(),
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
        for a in &p.atlas_writes {
            if a.x + a.w <= self.atlas_tex.width() && a.y + a.h <= self.atlas_tex.height() {
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo { texture: &self.atlas_tex, mip_level: 0, origin: wgpu::Origin3d { x: a.x, y: a.y, z: 0 }, aspect: wgpu::TextureAspect::All },
                    &a.rgba,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(a.w * 4), rows_per_image: Some(a.h) },
                    wgpu::Extent3d { width: a.w, height: a.h, depth_or_array_layers: 1 },
                );
            }
        }
        if !p.atlas_writes.is_empty() {
            self.gen_mips_in(&p.atlas_writes.iter().map(|a| [a.x, a.y, a.w, a.h]).collect::<Vec<_>>());
        }
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let hud_g = Globals {
            view_proj: Mat4::orthographic_rh(0.0, w, h, 0.0, -1.0, 1.0).to_cols_array_2d(),
            cam_pos: [0.0; 4],
            fog_color: [0.0; 4],
            fog: [1e9, 2e9, 0.0, 0.0],
            sky_top: [0.0; 4],
            cam_fwd: [0.0; 4],
            texfilter: [0.0; 4],
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
            cam_fwd: [0.0; 4],
            texfilter: self.texfilter(),
            beam: [[0.0; 4]; 4],
            night: [[0; 4]; NIGHT_WORDS],
        };
        self.queue.write_buffer(&self.sky_globals, 0, bytemuck::bytes_of(&sky_g));
        self.sky_vb.upload(&self.device, &self.queue, sky);
        self.dyn_vb.upload(&self.device, &self.queue, dynamic);
        self.shadow_vb.upload(&self.device, &self.queue, &p.shadows);
        self.objects_vb.upload(&self.device, &self.queue, &p.objects);
        self.late_vb.upload(&self.device, &self.queue, &p.subtract_late);
        self.hud_vb.upload(&self.device, &self.queue, hud);
        self.under_vb.upload(&self.device, &self.queue, &self.hud_under);
        self.mid_vb.upload(&self.device, &self.queue, &self.hud_mid);
        self.sky_layer_vb.upload(&self.device, &self.queue, &self.sky_layer);
        self.glow_vb.upload(&self.device, &self.queue, &self.glows);
        self.glow_sub_vb.upload(&self.device, &self.queue, &self.glows_sub);
        // (drawn at the supersampling scale: see `present`; with MSAA, to `msaa_color` and
        // resolved to `view` after each pass, the next one going on from the samples)
        let k = self.scale();
        let target = (self.size.0 * k, self.size.1 * k);
        let samples = self.samples();
        if self.pipes.samples != samples {
            self.pipes = ScenePipelines::new(&self.device, &self.layout, &self.shader, self.format, samples);
        }
        let depth = self.depth_view(target, samples);
        let msaa = (samples > 1).then(|| self.msaa_view(target, samples));
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: msaa.as_ref().unwrap_or(view),
                    depth_slice: None,
                    resolve_target: msaa.as_ref().map(|_| view),
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
            // (in the target's pixels: the window's times the supersampling)
            let k = k as f32;
            let full = [0.0, 0.0, w * k, h * k];
            let set_vp = |rp: &mut wgpu::RenderPass, r: [f32; 4]| rp.set_viewport(r[0], r[1], r[2], r[3], 0.0, 1.0);
            let (vp3d, vp2d) = match pass.viewport {
                Some(v) => {
                    let v = v.map(|c| c * k);
                    let (x0, y0) = (v[0].max(0.0).floor(), v[1].max(0.0).floor());
                    let (x1, y1) = ((v[0] + v[2]).min(w * k).ceil(), (v[1] + v[3]).min(h * k).ceil());
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
                rp.set_pipeline(&self.pipes.sky);
                rp.draw(0..3, 0..1);
            }
            if !self.hud_under.is_empty() {
                set_vp(&mut rp, vp2d);
                rp.set_pipeline(&self.pipes.hud);
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
                        rp.set_pipeline(&self.pipes.hud);
                        rp.draw(at..r.start, 0..1);
                    }
                    rp.set_pipeline(&self.pipes.hud_sub);
                    rp.draw(r.clone(), 0..1);
                    at = r.end;
                }
                if at < n {
                    rp.set_pipeline(&self.pipes.hud);
                    rp.draw(at..n, 0..1);
                }
                rp.set_bind_group(0, &self.world_bg, &[]);
                set_vp(&mut rp, vp3d);
            }
            // the dome, the sun over it, then the horizon band over the sun
            let dome = p.sky_dome.min(sky.len() as u32);
            if dome > 0 {
                rp.set_pipeline(&self.pipes.hud);
                rp.set_bind_group(0, &self.sky_bg, &[]);
                rp.set_vertex_buffer(0, self.sky_vb.buf.slice(..));
                rp.draw(0..dome, 0..1);
                rp.set_bind_group(0, &self.world_bg, &[]);
            }
            if !self.sky_layer.is_empty() {
                set_vp(&mut rp, vp2d);
                rp.set_pipeline(&self.pipes.hud);
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
                rp.set_pipeline(&self.pipes.hud);
                rp.set_bind_group(0, &self.sky_bg, &[]);
                rp.set_vertex_buffer(0, self.sky_vb.buf.slice(..));
                rp.draw(dome..sky.len() as u32, 0..1);
                rp.set_bind_group(0, &self.world_bg, &[]);
            }
            rp.set_pipeline(&self.pipes.track);
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
            if !p.objects.is_empty() {
                rp.set_vertex_buffer(0, self.objects_vb.buf.slice(..));
                rp.draw(0..p.objects.len() as u32, 0..1);
            }
            if !p.shadows.is_empty() {
                rp.set_pipeline(&self.pipes.sub);
                rp.set_vertex_buffer(0, self.shadow_vb.buf.slice(..));
                rp.draw(0..p.shadows.len() as u32, 0..1);
            }
            rp.set_pipeline(&self.pipes.world);
            if !dynamic.is_empty() || !self.hud_mid.is_empty() {
                let (n, mid) = (dynamic.len() as u32, self.dyn_mid.min(dynamic.len() as u32));
                rp.set_vertex_buffer(0, self.dyn_vb.buf.slice(..));
                rp.draw(0..mid, 0..1);
                if !self.hud_mid.is_empty() {
                    set_vp(&mut rp, vp2d);
                    rp.set_pipeline(&self.pipes.hud);
                    rp.set_bind_group(0, &self.hud_bg, &[]);
                    rp.set_vertex_buffer(0, self.mid_vb.buf.slice(..));
                    let n = self.hud_mid.len() as u32;
                    let mut at = 0;
                    for r in self.hud_mid_subtract.iter().filter(|r| r.end <= n) {
                        if r.start > at {
                            rp.set_pipeline(&self.pipes.hud);
                            rp.draw(at..r.start, 0..1);
                        }
                        rp.set_pipeline(&self.pipes.hud_sub);
                        rp.draw(r.clone(), 0..1);
                        at = r.end;
                    }
                    if at < n {
                        rp.set_pipeline(&self.pipes.hud);
                        rp.draw(at..n, 0..1);
                    }
                    rp.set_pipeline(&self.pipes.world);
                    rp.set_bind_group(0, &self.world_bg, &[]);
                    rp.set_vertex_buffer(0, self.dyn_vb.buf.slice(..));
                    set_vp(&mut rp, vp3d);
                }
                rp.draw(mid..n, 0..1);
            }
            if !p.subtract_late.is_empty() {
                rp.set_pipeline(&self.pipes.sub);
                rp.set_vertex_buffer(0, self.late_vb.buf.slice(..));
                rp.draw(0..p.subtract_late.len() as u32, 0..1);
            }
            set_vp(&mut rp, vp2d);
            if !self.glows.is_empty() {
                if let Some((_, bg)) = &self.overlay_atlas {
                    rp.set_pipeline(&self.pipes.glow);
                    rp.set_bind_group(0, bg, &[]);
                    rp.set_vertex_buffer(0, self.glow_vb.buf.slice(..));
                    rp.draw(0..self.glows.len() as u32, 0..1);
                }
            }
            if !self.glows_sub.is_empty() {
                if let Some((_, bg)) = &self.overlay_atlas {
                    rp.set_pipeline(&self.pipes.glow_sub);
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
                rp.set_pipeline(&self.pipes.hud);
                rp.draw(at..r.start, 0..1);
            }
            rp.set_pipeline(&self.pipes.hud_sub);
            rp.draw(r.clone(), 0..1);
            at = r.end;
        }
        if at < hud.len() as u32 {
            rp.set_pipeline(&self.pipes.hud);
            rp.draw(at..hud.len() as u32, 0..1);
        }
    }

    /// `text` (digits, space and the letters in [`glyph`]) as `stats`: white pixel-font
    /// characters on a dark box at the window's top left, sized to the window.
    pub fn set_stats_text(&mut self, text: &str) {
        self.stats.clear();
        if text.is_empty() {
            return;
        }
        let px = (self.size.1 as f32 / 240.0).round().max(2.0);
        let (x0, y0) = (px * 2.0, px * 2.0);
        let mut quad = |x: f32, y: f32, w: f32, h: f32, c: [f32; 4]| {
            let v = |x: f32, y: f32| Vertex { pos: [x, y, 0.0], uv: [0.0; 2], color: c };
            self.stats.extend([v(x, y), v(x + w, y), v(x, y + h), v(x + w, y), v(x + w, y + h), v(x, y + h)]);
        };
        let n = text.chars().count() as f32;
        quad(x0 - px, y0 - px, n * 4.0 * px + px, 7.0 * px, [0.0, 0.0, 0.0, 0.6]);
        for (i, ch) in text.chars().enumerate() {
            for (r, bits) in glyph(ch).iter().enumerate() {
                for col in 0..3 {
                    if bits & (4 >> col) != 0 {
                        quad(x0 + (i as f32 * 4.0 + col as f32) * px, y0 + r as f32 * px, px, px, [1.0; 4]);
                    }
                }
            }
        }
    }

    /// The `stats` pass over a finished frame.
    fn encode_stats(&mut self, view: &wgpu::TextureView) -> Option<wgpu::CommandBuffer> {
        if self.stats.is_empty() {
            return None;
        }
        self.stats_vb.upload(&self.device, &self.queue, &self.stats);
        let depth = self.depth_view(self.size, 1);
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("stats"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // (hud_bg: its uniforms hold the window's pixel projection)
            rp.set_pipeline(&self.stats_pipeline);
            rp.set_bind_group(0, &self.hud_bg, &[]);
            rp.set_vertex_buffer(0, self.stats_vb.buf.slice(..));
            rp.draw(0..self.stats.len() as u32, 0..1);
        }
        Some(enc.finish())
    }

    pub fn render(&mut self, p: &FrameParams, sky: &[Vertex], dynamic: &[Vertex], hud: &[Vertex]) {
        self.present(|r, view| vec![r.encode(view, p, sky, dynamic, hud)]);
    }

    /// Draw a frame with `draw` and show it in the window: drawn to `post_target` when
    /// supersampled, through the CRT filter or with bloom, then `stats` over it.
    fn present(&mut self, draw: impl FnOnce(&mut Self, &wgpu::TextureView) -> Vec<wgpu::CommandBuffer>) {
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
        let mut cmds = self.draw_scaled(&view, true, draw);
        cmds.extend(self.encode_stats(&view));
        self.queue.submit(cmds);
        self.queue.present(frame);
    }

    /// `draw` a frame to `view` (window-sized): straight there, or at the supersampling
    /// scale to `post_target` and then scaled down or through the CRT filter, with bloom
    /// (the window's `effects`, CRT and bloom, when they are on).
    fn draw_scaled(&mut self, view: &wgpu::TextureView, effects: bool, draw: impl FnOnce(&mut Self, &wgpu::TextureView) -> Vec<wgpu::CommandBuffer>) -> Vec<wgpu::CommandBuffer> {
        let (crt, bloom) = (effects && self.crt, effects && self.bloom);
        if !crt && !bloom && self.scale() == 1 {
            return draw(self, view);
        }
        let t = self.post_target();
        let mut cmds = draw(self, &t.scene);
        cmds.push(self.encode_post(view, &t, crt, bloom));
        cmds
    }

    /// The frame drawn for the CRT filter, supersampling or bloom (the window's size times the
    /// supersampling), with the bloom's levels.
    fn post_target(&mut self) -> PostTarget {
        let k = self.scale();
        if let Some(t) = &self.post_target {
            if (t.size, t.scale) == (self.size, k) {
                return t.clone();
            }
        }
        let texture = |label, (w, h): (u32, u32), format| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let bind = |src: &wgpu::TextureView, bloom: &wgpu::TextureView| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("crt"),
                layout: &self.crt_bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(src) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.crt_sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: self.crt_params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(bloom) },
                ],
            })
        };
        let scene = texture("scene", (self.size.0 * k, self.size.1 * k), self.format);
        // the levels from half the window's size down to about 16 pixels (or BLOOM_LEVELS)
        let mut bloom = Vec::new();
        let mut size = ((self.size.0 / 2).max(1), (self.size.1 / 2).max(1));
        while bloom.len() < BLOOM_LEVELS {
            let v = texture("bloom", size, BLOOM_FORMAT);
            bloom.push((bind(&v, &self.black), v));
            if size.0.min(size.1) <= 16 {
                break;
            }
            size = ((size.0 / 2).max(1), (size.1 / 2).max(1));
        }
        let t = PostTarget { size: self.size, scale: k, bg: bind(&scene, &bloom[0].1), bg_bright: bind(&scene, &self.black), scene, bloom };
        self.post_target = Some(t.clone());
        t
    }

    /// The frame in `t` to `view`: its bloom (`bloom`) made, then the frame through the CRT
    /// filter or scaled down, with the bloom added.
    fn encode_post(&self, view: &wgpu::TextureView, t: &PostTarget, crt: bool, bloom: bool) -> wgpu::CommandBuffer {
        let mut u = [[0.0; 4]; 4];
        u[..3].copy_from_slice(&self.crt_settings.uniform(self.size, self.scale()));
        if bloom {
            // (the levels are added up: their average times the intensity)
            u[3] = [self.bloom_settings.intensity / t.bloom.len() as f32, self.bloom_settings.threshold, 0.0, 0.0];
        }
        self.queue.write_buffer(&self.crt_params, 0, bytemuck::cast_slice(&u));
        let mut enc = self.device.create_command_encoder(&Default::default());
        if bloom {
            for (k, (_, v)) in t.bloom.iter().enumerate() {
                let (pipeline, bg) = if k == 0 { (&self.bloom_bright_pipeline, &t.bg_bright) } else { (&self.bloom_down_pipeline, &t.bloom[k - 1].0) };
                post_pass(&mut enc, v, true, pipeline, bg);
            }
            for k in (1..t.bloom.len()).rev() {
                post_pass(&mut enc, &t.bloom[k - 1].1, false, &self.bloom_up_pipeline, &t.bloom[k].0);
            }
        }
        post_pass(&mut enc, view, true, if crt { &self.crt_pipeline } else { &self.down_pipeline }, &t.bg);
        enc.finish()
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
        self.present(|r, view| r.encode_layers(view, layers));
    }

    pub fn screenshot_layers(&mut self, layers: &[Layer]) -> image::RgbaImage {
        let (tex, view) = self.shot_target();
        let cmds = self.draw_scaled(&view, false, |r, v| r.encode_layers(v, layers));
        self.read_back(&tex, cmds)
    }

    pub fn render_views(&mut self, views: &[ViewDraw], sky: &[Vertex], hud: &[Vertex]) {
        self.present(|r, view| r.encode_views(view, views, sky, hud));
    }

    pub fn screenshot_views(&mut self, views: &[ViewDraw], sky: &[Vertex], hud: &[Vertex]) -> image::RgbaImage {
        let (tex, view) = self.shot_target();
        let cmds = self.draw_scaled(&view, false, |r, v| r.encode_views(v, views, sky, hud));
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
        let cmds = self.draw_scaled(&view, false, |r, v| vec![r.encode(v, p, sky, dynamic, hud)]);
        self.read_back(&tex, cmds)
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
    let levels = atlas.mip_levels.clamp(1, atlas.size.max(1).ilog2() + 1);
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("atlas"),
        size: wgpu::Extent3d { width: atlas.size, height: atlas.size, depth_or_array_layers: 1 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ATLAS_FORMAT,
        // (the mip levels are drawn: see `Renderer::gen_mips`)
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | if levels > 1 { wgpu::TextureUsages::RENDER_ATTACHMENT } else { wgpu::TextureUsages::empty() },
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        &atlas.rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(atlas.size * 4), rows_per_image: Some(atlas.size) },
        wgpu::Extent3d { width: atlas.size, height: atlas.size, depth_or_array_layers: 1 },
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

/// The pipelines drawing the scene (to targets of `samples` samples per pixel).
struct ScenePipelines {
    samples: u32,
    world: wgpu::RenderPipeline,
    track: wgpu::RenderPipeline,
    hud: wgpu::RenderPipeline,
    /// 2D drawn subtracting.
    hud_sub: wgpu::RenderPipeline,
    sub: wgpu::RenderPipeline,
    sky: wgpu::RenderPipeline,
    glow: wgpu::RenderPipeline,
    glow_sub: wgpu::RenderPipeline,
}

impl ScenePipelines {
    fn new(device: &wgpu::Device, layout: &wgpu::PipelineLayout, shader: &wgpu::ShaderModule, format: wgpu::TextureFormat, samples: u32) -> Self {
        let mk = |vs, fs, vertices, depth, blend, cull| mk_pipeline(device, layout, shader, format, samples, vs, fs, vertices, depth, blend, cull);
        // reversed Z (1 = near, 0 = infinitely far): even precision over the whole track
        let depth = Some((true, wgpu::CompareFunction::Greater));
        // semi-transparent texels blend (fs_hud's premultiplied output: PS1 modes 0 and 1)
        let blend = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        // background - foreground
        let subtract = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::ReverseSubtract };
        let subtract = Some(wgpu::BlendState { color: subtract, alpha: subtract });
        // tested against the 3D but not written
        let tested = Some((false, wgpu::CompareFunction::GreaterEqual));
        Self {
            samples,
            // the track is single-sided like on the PS1 (double-sided materials are emitted twice)
            track: mk("vs_main", "fs_hud", true, depth, blend, Some(wgpu::Face::Back)),
            world: mk("vs_main", "fs_hud", true, depth, blend, None),
            sub: mk("vs_main", "fs_sub", true, tested, subtract, None),
            hud_sub: mk("vs_main", "fs_sub", true, None, subtract, None),
            hud: mk("vs_main", "fs_hud", true, None, blend, None),
            sky: mk("vs_sky", "fs_sky", false, None, None, None),
            glow_sub: mk("vs_main", "fs_sub", true, tested, subtract, None),
            glow: mk("vs_main", "fs_hud", true, tested, blend, None),
        }
    }
}

/// A pipeline of shader.wgsl: `vertices` from a [`Vertex`] buffer, `depth` (write, compare;
/// None: always passes, not written).
#[allow(clippy::too_many_arguments)]
fn mk_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    samples: u32,
    vs: &str,
    fs: &str,
    vertices: bool,
    depth: Option<(bool, wgpu::CompareFunction)>,
    blend: Option<wgpu::BlendState>,
    cull: Option<wgpu::Face>,
) -> wgpu::RenderPipeline {
    let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];
    let vbl = [Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None,
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: if vertices { &vbl } else { &[] },
        },
        primitive: wgpu::PrimitiveState { cull_mode: cull, front_face: TRACK_FRONT_FACE, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(depth.map(|d| d.0).unwrap_or(false)),
            depth_compare: Some(depth.map(|d| d.1).unwrap_or(wgpu::CompareFunction::Always)),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Per mip level of `tex` after the first: a bind group of the level above (for mip.wgsl)
/// and the level's view to draw it to.
fn mip_chain(device: &wgpu::Device, bgl: &wgpu::BindGroupLayout, tex: &wgpu::Texture) -> Vec<(wgpu::BindGroup, wgpu::TextureView)> {
    let level = |l: u32| tex.create_view(&wgpu::TextureViewDescriptor { base_mip_level: l, mip_level_count: Some(1), ..Default::default() });
    (1..tex.mip_level_count())
        .map(|l| {
            let src = level(l - 1);
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("mip"),
                layout: bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src) }],
            });
            (bg, level(l))
        })
        .collect()
}

/// The CRT filter's settings (see crt.wgsl).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crt {
    /// Barrel distortion per axis (the edges' midpoints stay put, the corners bow out).
    pub curvature: [f32; 2],
    /// The aperture grille's dark stripes (1: no grille).
    pub mask_dark: f32,
    /// The beam's width in lines on black and on white (the scanlines' gaps).
    pub beam_min: f32,
    pub beam_max: f32,
    pub gamma: f32,
    pub brightness: f32,
    /// The vignette's strength (0: none).
    pub vignette: f32,
    /// The scanlines' height in window pixels (0: by the window's height).
    pub line_px: f32,
}

impl Default for Crt {
    fn default() -> Self {
        Self { curvature: [0.015, 0.02], mask_dark: 0.72, beam_min: 0.22, beam_max: 0.42, gamma: 2.2, brightness: 1.2, vignette: 0.12, line_px: 0.0 }
    }
}

impl Crt {
    /// crt.wgsl's `Params`, for a window of `size` and the frame drawn `scale` times that.
    fn uniform(&self, size: (u32, u32), scale: u32) -> [[f32; 4]; 3] {
        [
            [self.curvature[0], self.curvature[1], self.mask_dark, self.beam_min],
            [self.beam_max, self.gamma, self.brightness, self.vignette],
            [self.line_px, size.0 as f32, size.1 as f32, scale as f32],
        ]
    }
}

/// Bloom's settings (see crt.wgsl).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bloom {
    /// How much of the glow is added to the frame.
    pub intensity: f32,
    /// The brightness (0 to 1, in linear light) above which a pixel glows.
    pub threshold: f32,
}

impl Default for Bloom {
    fn default() -> Self {
        Self { intensity: 0.5, threshold: 0.6 }
    }
}

/// The frame drawn for the window's post-processing (see [`Renderer::post_target`]).
#[derive(Clone)]
struct PostTarget {
    /// The window's size and the supersampling it was made for.
    size: (u32, u32),
    scale: u32,
    scene: wgpu::TextureView,
    /// `scene` bound with the bloom's first level, for the last pass.
    bg: wgpu::BindGroup,
    /// `scene` bound without it, for the bloom's first level.
    bg_bright: wgpu::BindGroup,
    /// The bloom's levels, each half the size of the one before: each one bound (to be
    /// sampled by the next) and its view.
    bloom: Vec<(wgpu::BindGroup, wgpu::TextureView)>,
}

/// One full-window triangle of crt.wgsl's `pipeline` to `view` (cleared to black first or
/// drawn over), sampling what `bg` binds.
fn post_pass(enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, clear: bool, pipeline: &wgpu::RenderPipeline, bg: &wgpu::BindGroup) {
    let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("post"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: if clear { wgpu::LoadOp::Clear(wgpu::Color::BLACK) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    rp.set_pipeline(pipeline);
    rp.set_bind_group(0, bg, &[]);
    rp.draw(0..3, 0..1);
}

/// A 3x5 pixel font character: five rows, bit 2 the left column (blank when not in it).
fn glyph(c: char) -> [u8; 5] {
    match c.to_ascii_uppercase() {
        '0' => [7, 5, 5, 5, 7],
        '1' => [2, 6, 2, 2, 7],
        '2' => [7, 1, 7, 4, 7],
        '3' => [7, 1, 3, 1, 7],
        '4' => [5, 5, 7, 1, 1],
        '5' => [7, 4, 7, 1, 7],
        '6' => [7, 4, 7, 5, 7],
        '7' => [7, 1, 1, 2, 2],
        '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 7],
        'F' => [7, 4, 6, 4, 4],
        'P' => [6, 5, 6, 4, 4],
        'S' => [3, 4, 2, 1, 6],
        _ => [0; 5],
    }
}
