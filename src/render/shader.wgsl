// PS1-style shading: texture * vertex colour * 2 (0x80 = neutral), colour-key transparency, distance fog.

struct Globals {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    fog_color: vec4<f32>,
    // x = fog start, y = fog end
    fog: vec4<f32>,
    sky_top: vec4<f32>,
    // x = ambient light multiplier, y = headlight strength
    light: vec4<f32>,
    head_pos: vec4<f32>,
    head_dir: vec4<f32>,
    // camera forward axis; w = 1: `fog` holds the PS1 depth cue (DQA, DQB, H), fog_color the far colour
    cam_fwd: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) dist: f32,
    @location(3) world: vec3<f32>,
};

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.dist = distance(v.pos, g.cam_pos.xyz);
    o.world = v.pos;
    return o;
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    return shade(i);
}

// The 2D overlay, blended with premultiplied alpha so one pipeline does the PS1's modes:
// opaque (vertex alpha 1), half background + half foreground (0.5) and background +
// foreground (0); the semi-transparent modes apply to STP texels only.
@fragment
fn fs_hud(i: VOut) -> @location(0) vec4<f32> {
    let o = shade(i);
    if (o.a < 0.75) {
        if (i.color.a < 0.25) {
            return vec4<f32>(o.rgb, 0.0);
        }
        return vec4<f32>(o.rgb * 0.5, 0.5);
    }
    return vec4<f32>(o.rgb, 1.0);
}

fn shade(i: VOut) -> vec4<f32> {
    let t = textureSample(atlas, samp, i.uv);
    if (t.a < 0.25) {
        discard;
    }
    var vc = i.color.rgb;
    if (g.cam_fwd.w > 0.5) {
        // GTE depth cue: IR0 = clamp((DQB + DQA * H / SZ) / 4096, 0, 4096); SZ in GTE units (1/64 m)
        let sz = max(dot(i.world - g.cam_pos.xyz, g.cam_fwd.xyz), 0.01) * 64.0;
        let n = min(g.fog.z * 65536.0 / sz, 131071.0);
        let ir0 = clamp((g.fog.y + g.fog.x * n) / 4096.0, 0.0, 4096.0) / 4096.0;
        vc = mix(vc, g.fog_color.rgb, ir0);
    }
    var c = min(t.rgb * vc * 2.0, vec3<f32>(1.0));
    // ambient (night darkening) + player headlight cone
    var lit = g.light.x;
    if (g.light.y > 0.0) {
        let to = i.world - g.head_pos.xyz;
        let d = length(to);
        let cone = smoothstep(0.86, 0.97, dot(to / max(d, 0.001), g.head_dir.xyz));
        let fall = clamp(1.0 - d / 70.0, 0.0, 1.0);
        lit = lit + g.light.y * cone * fall * fall;
    }
    c = c * min(lit, 1.25);
    if (g.cam_fwd.w < 0.5) {
        let f = clamp((i.dist - g.fog.x) / max(g.fog.y - g.fog.x, 1.0), 0.0, 1.0);
        c = mix(c, g.fog_color.rgb, f);
    }
    // PS1 semi-transparency (mode 0, half background + half foreground): primitives flagged
    // semi-transparent (vertex alpha < 0.75) blend only their STP texels (alpha 128)
    var a = 1.0;
    if (i.color.a < 0.75 && t.a < 0.75) {
        a = 0.5;
    }
    return vec4<f32>(c, a);
}

// Fullscreen sky gradient.
struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) y: f32,
};

@vertex
fn vs_sky(@builtin(vertex_index) idx: u32) -> SkyOut {
    let p = vec2<f32>(f32((idx << 1u) & 2u), f32(idx & 2u)) * 2.0 - 1.0;
    var o: SkyOut;
    o.clip = vec4<f32>(p, 0.9999, 1.0);
    o.y = p.y;
    return o;
}

@fragment
fn fs_sky(i: SkyOut) -> @location(0) vec4<f32> {
    let top = g.sky_top.rgb;
    let k = clamp(i.y * 0.5 + 0.5, 0.0, 1.0);
    return vec4<f32>(mix(g.fog_color.rgb, top, k * k), 1.0);
}
