//! How much of the sun the world leaves uncovered, for the original's lens flare.
//!
//! Flare_LensFlare draws a 5x5 white quad at the sun, behind the world, and reads the 25
//! pixels back from the finished frame (StoreImage): the more of them are still white, the
//! brighter the flare. The PS1 frame is never drawn here, so the same 25 points (one per PS1
//! pixel of that square) are tested against this renderer's depth buffer instead, each by an
//! occlusion query on one pixel drawn at the sky's depth: it passes only where nothing of
//! the world was drawn. The results come back a frame or two later.

use super::{DEPTH, DynBuf, TRACK_FRONT_FACE};
use crate::assets::Vertex;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// The points tested (5 x 5).
pub const POINTS: u32 = 25;
/// Readback buffers in flight.
const RING: usize = 3;
/// A readback buffer's state.
const IDLE: u8 = 0;
const PENDING: u8 = 1;
const MAPPED: u8 = 2;

struct Readback {
    buf: wgpu::Buffer,
    state: Arc<AtomicU8>,
}

pub(super) struct SunProbe {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    ring: Vec<Readback>,
    /// The ring's buffer this frame's queries are copied to (taken when the pass is encoded).
    next: Option<usize>,
    pub(super) vb: DynBuf,
}

impl SunProbe {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let size = POINTS as u64 * wgpu::QUERY_SIZE as u64;
        let ring = (0..RING)
            .map(|_| Readback {
                buf: device.create_buffer(&wgpu::BufferDescriptor { label: Some("sun probe read"), size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }),
                state: Arc::new(AtomicU8::new(IDLE)),
            })
            .collect();
        Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("sun probe"), ty: wgpu::QueryType::Occlusion, count: POINTS }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor { label: Some("sun probe resolve"), size, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false }),
            ring,
            next: None,
            vb: DynBuf::new(device, "sun probe"),
        }
    }

    /// The query set for a pass that tests the sun, when a readback buffer is free for it.
    pub(super) fn begin(&mut self) -> Option<&wgpu::QuerySet> {
        self.next = self.ring.iter().position(|r| r.state.load(Ordering::Acquire) == IDLE);
        self.next.map(|_| &self.queries)
    }

    /// The 25 one-pixel quads (window coordinates) around `centre`, `cell` apart, each
    /// covering the target pixel (`scale` per window pixel) its point falls in, at the sky's
    /// depth (z 1: depth 0 in the 2D projection).
    pub(super) fn quads(centre: [f32; 2], cell: [f32; 2], scale: f32) -> Vec<Vertex> {
        let mut v = Vec::with_capacity(POINTS as usize * 6);
        for j in -2..=2 {
            for i in -2..=2 {
                let x0 = ((centre[0] + i as f32 * cell[0]) * scale).floor() / scale;
                let y0 = ((centre[1] + j as f32 * cell[1]) * scale).floor() / scale;
                let (x1, y1) = (x0 + 1.0 / scale, y0 + 1.0 / scale);
                let p = |x: f32, y: f32| Vertex { pos: [x, y, 1.0], uv: [0.0; 2], color: [1.0; 4] };
                v.extend([p(x0, y0), p(x1, y0), p(x0, y1), p(x1, y0), p(x1, y1), p(x0, y1)]);
            }
        }
        v
    }

    /// After the pass: the query results copied to the ring's buffer, mapped once submitted.
    pub(super) fn finish(&mut self, enc: &mut wgpu::CommandEncoder) {
        let Some(k) = self.next.take() else { return };
        let r = &self.ring[k];
        enc.resolve_query_set(&self.queries, 0..POINTS, &self.resolve, 0);
        enc.copy_buffer_to_buffer(&self.resolve, 0, &r.buf, 0, POINTS as u64 * wgpu::QUERY_SIZE as u64);
        r.state.store(PENDING, Ordering::Release);
        let state = r.state.clone();
        enc.map_buffer_on_submit(&r.buf, wgpu::MapMode::Read, .., move |res| state.store(if res.is_ok() { MAPPED } else { IDLE }, Ordering::Release));
    }

    /// The newest results that came back: bit `5 * row + column` set when that point of the
    /// sun is uncovered.
    pub(super) fn take(&mut self, device: &wgpu::Device) -> Option<u32> {
        let _ = device.poll(wgpu::PollType::Poll);
        let mut newest = None;
        for r in &self.ring {
            if r.state.load(Ordering::Acquire) != MAPPED {
                continue;
            }
            if let Ok(data) = r.buf.slice(..).get_mapped_range() {
                let counts = data.as_chunks::<8>().0.iter().map(|&q| u64::from_le_bytes(q));
                newest = Some(counts.enumerate().fold(0u32, |bits, (i, n)| if n > 0 { bits | 1 << i } else { bits }));
            }
            r.buf.unmap();
            r.state.store(IDLE, Ordering::Release);
        }
        newest
    }
}

/// The probe's pipeline: `vs_main` in the 2D projection, tested against the world's depth
/// (passing only at the sky's, 0), nothing written.
pub(super) fn pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, shader: &wgpu::ShaderModule, format: wgpu::TextureFormat, samples: u32) -> wgpu::RenderPipeline {
    let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4];
    let vbl = [Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sun probe"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &vbl },
        primitive: wgpu::PrimitiveState { front_face: TRACK_FRONT_FACE, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_solid"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::empty() })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
