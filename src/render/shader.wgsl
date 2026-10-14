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
    // night headlight beam: right axis (w xs), forward axis (w z0), origin (w 1 = on), (zs)
    beam: array<vec4<f32>, 4>,
    // its level map (64x64 bytes), colour-index table (256x16 bytes), light table (256 words)
    night: array<vec4<u32>, 576>,
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

fn night_word(i: u32) -> u32 {
    return g.night[i >> 2u][i & 3u];
}

fn night_byte(i: u32) -> u32 {
    return (night_word(i >> 2u) >> ((i & 3u) * 8u)) & 0xFFu;
}

// A track vertex carries its light-table index s as alpha + 2 (s + 1): at night inside the
// headlight beam its colour becomes light[table[s][level]] (Night_NightCalc).
fn track_colour(pos: vec3<f32>, c: vec4<f32>) -> vec4<f32> {
    if (c.a < 1.5) {
        return c;
    }
    let k = floor(c.a * 0.5);
    var o = vec4<f32>(c.rgb, c.a - 2.0 * k);
    let s = u32(k) - 1u;
    if (g.beam[2].w < 0.5) {
        return o;
    }
    let d = (pos - g.beam[2].xyz) * 64.0;
    let x = i32(floor(dot(d, g.beam[0].xyz)));
    let z = i32(floor(dot(d, g.beam[1].xyz)));
    let xs = u32(g.beam[0].w);
    let zs = u32(g.beam[3].x);
    let z0 = i32(g.beam[1].w);
    let half = 1 << (xs + 5u);
    if (z <= z0 || z >= z0 + (1 << (zs + 6u)) || x <= -half || x >= half) {
        return o;
    }
    let row = u32((z - z0) >> zs);
    let col = u32((x + half) >> xs);
    let level = night_byte(row * 64u + col);
    let s2 = night_byte(4096u + s * 16u + level);
    let l = night_word(2048u + s2);
    o = vec4<f32>(f32(l & 0xFFu) / 256.0, f32((l >> 8u) & 0xFFu) / 256.0, f32((l >> 16u) & 0xFFu) / 256.0, o.a);
    return o;
}

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.color = track_colour(v.pos, v.color);
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

// PS1 semi-transparency mode 2 (background - foreground), with the blend set to subtract.
@fragment
fn fs_sub(i: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, samp, i.uv);
    if (t.a < 0.25) {
        discard;
    }
    return vec4<f32>(min(t.rgb * i.color.rgb * 2.0, vec3<f32>(1.0)), 0.0);
}

fn shade(i: VOut) -> vec4<f32> {
    let t = textureSample(atlas, samp, i.uv);
    if (t.a < 0.25) {
        discard;
    }
    var vc = i.color.rgb;
    if (g.cam_fwd.w > 0.5 && i.color.a >= 0.25) {
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
