// The finished frame (drawn supersampled, or for this) to the window: as a curved TV picture
// with scanlines, a beam that widens on bright lines and an aperture-grille mask (the CRT
// filter, F6), or scaled down; with bloom added (F7), made by the passes at the end.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
// bloom's first level, the others added up into it (black in bloom's own passes)
@group(0) @binding(3) var bloom_tex: texture_2d<f32>;

// the settings (nfshs.ini's [crt] and [bloom])
struct Params {
    // barrel distortion per axis (the edges' midpoints stay put, the corners bow out), the
    // grille's dark stripes, the beam's width (in lines) on black
    a: vec4<f32>,
    // the beam's width on white, gamma, brightness, vignette
    b: vec4<f32>,
    // a scanline's height in window pixels (0: by the window's height), the window's size,
    // the frame's scale (supersampling)
    c: vec4<f32>,
    // bloom's intensity over its number of levels (0: no bloom), its threshold
    d: vec4<f32>,
};
@group(0) @binding(2) var<uniform> prm: Params;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// one triangle over the window
@vertex
fn vs_crt(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VsOut;
    o.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    o.uv = uv;
    return o;
}

fn lin(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(prm.b.y));
}

// where a point of the window shows the frame: barrel distortion (the edges' midpoints stay
// put, the corners bow out)
fn curve(uv: vec2<f32>) -> vec2<f32> {
    var c = uv * 2.0 - 1.0;
    c = c * (1.0 + c.yx * c.yx * prm.a.xy);
    return c * 0.5 + 0.5;
}

// a line's colour at `x` (across the window): the frame averaged over the line's height (one
// row of it would skip the rows between the lines, shimmering as the picture moves), with
// three taps across softening it to a TV's horizontal resolution
fn line_color(x: f32, line: f32, px: f32) -> vec3<f32> {
    let size = prm.c.yz;
    let dx = vec2<f32>(px * 0.5 / size.x, 0.0);
    var sum = vec3<f32>(0.0);
    for (var i = 0; i < 4; i++) {
        let p = curve(vec2<f32>(x, ((line - 0.5 + (f32(i) + 0.5) * 0.25) * px + 0.5) / size.y));
        let a = textureSampleLevel(src, samp, p - dx, 0.0).rgb;
        let b = textureSampleLevel(src, samp, p, 0.0).rgb;
        let c = textureSampleLevel(src, samp, p + dx, 0.0).rgb;
        sum += lin(a) * 0.25 + lin(b) * 0.5 + lin(c) * 0.25;
    }
    return sum * 0.25;
}

// a gaussian beam `d` lines from its centre, wider the brighter it is
fn beam(d: f32, c: vec3<f32>) -> vec3<f32> {
    let w = mix(vec3<f32>(prm.a.w), vec3<f32>(prm.b.x), sqrt(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0))));
    return exp(-0.5 * (d * d) / (w * w));
}

@fragment
fn fs_crt(in: VsOut) -> @location(0) vec4<f32> {
    let size = prm.c.yz;
    let uv = curve(in.uv);
    // the tube's rounded edge
    let edge = smoothstep(vec2<f32>(0.0), vec2<f32>(4.0) / size, uv) * smoothstep(vec2<f32>(0.0), vec2<f32>(4.0) / size, 1.0 - uv);
    if (edge.x * edge.y <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // the PS1's 480 lines, each a whole number of window pixels tall (no beating)
    var px = max(2.0, round(size.y / 480.0));
    if (prm.c.x > 0.0) {
        px = prm.c.x;
    }
    // the lines run along the window's pixel rows, the picture curving behind them: curved,
    // lines a few pixels tall beat against the rows in bands across the screen
    // (each line's centre on a row: lines two rows tall are a lit row and a gap)
    let lines = size.y / px;
    let y = (in.pos.y - 0.5) / px + 0.5;
    let line = floor(y);
    let f = y - line - 0.5;
    let near = select(line + 1.0, line - 1.0, f < 0.0);
    let c0 = line_color(in.uv.x, line, px);
    let c1 = line_color(in.uv.x, clamp(near, 0.0, ceil(lines) - 1.0), px);
    var col = c0 * beam(f, c0) + c1 * beam(1.0 - abs(f), c1);
    col += bloom_at(uv);

    // aperture grille: R, G, B stripes a window pixel each
    let m = u32(in.pos.x) % 3u;
    let dark = prm.a.z;
    var mask = vec3<f32>(dark);
    mask[m] = 1.0;
    col = col * mask;

    // back up to the brightness the scanlines and mask took away, then a soft vignette
    col = col * (1.0 / ((1.0 + 2.0 * dark) / 3.0)) * prm.b.z;
    let v = uv * (1.0 - uv);
    col = col * pow(v.x * v.y * 16.0, prm.b.w);
    col = pow(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / prm.b.y));
    return vec4<f32>(col * edge.x * edge.y, 1.0);
}

// Supersampling: each window pixel the average of its samples, in linear light (so the
// edges between bright and dark do not come out too dark).
@fragment
fn fs_down(in: VsOut) -> @location(0) vec4<f32> {
    let k = i32(prm.c.w);
    let p = vec2<i32>(in.pos.xy) * k;
    var sum = vec3<f32>(0.0);
    for (var y = 0; y < k; y++) {
        for (var x = 0; x < k; x++) {
            sum += pow(textureLoad(src, p + vec2<i32>(x, y), 0).rgb, vec3<f32>(2.2));
        }
    }
    let col = min(sum / f32(k * k) + bloom_at(in.uv), vec3<f32>(1.0));
    return vec4<f32>(pow(col, vec3<f32>(1.0 / 2.2)), 1.0);
}

// Bloom: the frame's light above the threshold, halved in size level by level (each level
// averaging a wider area of the one before), then each level from the smallest up added to
// the one above it, smoothed; the first level, all of them added up, is added to the frame.

// the glow at `uv`, in linear light (none without bloom)
fn bloom_at(uv: vec2<f32>) -> vec3<f32> {
    if (prm.d.x <= 0.0) {
        return vec3<f32>(0.0);
    }
    return textureSampleLevel(bloom_tex, samp, uv, 0.0).rgb * prm.d.x;
}

fn tap(uv: vec2<f32>, gamma: f32) -> vec3<f32> {
    return pow(max(textureSampleLevel(src, samp, uv, 0.0).rgb, vec3<f32>(0.0)), vec3<f32>(gamma));
}

// `src` averaged over a target pixel at `uv` and around it: 13 bilinear taps, weighted so
// that a lone bright pixel does not flicker as it moves (from Jimenez, "Next generation post
// processing in Call of Duty: Advanced Warfare")
fn down13(in: VsOut, gamma: f32) -> vec3<f32> {
    // a source pixel, for a target half the size (the first level from a larger frame: half
    // a target pixel)
    let d = fwidth(in.uv) * 0.5;
    let uv = in.uv;
    let a = tap(uv + d * vec2<f32>(-2.0, -2.0), gamma);
    let b = tap(uv + d * vec2<f32>(0.0, -2.0), gamma);
    let c = tap(uv + d * vec2<f32>(2.0, -2.0), gamma);
    let e = tap(uv + d * vec2<f32>(-2.0, 0.0), gamma);
    let f = tap(uv, gamma);
    let g = tap(uv + d * vec2<f32>(2.0, 0.0), gamma);
    let h = tap(uv + d * vec2<f32>(-2.0, 2.0), gamma);
    let i = tap(uv + d * vec2<f32>(0.0, 2.0), gamma);
    let j = tap(uv + d * vec2<f32>(2.0, 2.0), gamma);
    let k = tap(uv + d * vec2<f32>(-1.0, -1.0), gamma);
    let l = tap(uv + d * vec2<f32>(1.0, -1.0), gamma);
    let m = tap(uv + d * vec2<f32>(-1.0, 1.0), gamma);
    let n = tap(uv + d * vec2<f32>(1.0, 1.0), gamma);
    return (k + l + m + n) * 0.125 + f * 0.125 + (b + e + g + i) * 0.0625 + (a + c + h + j) * 0.03125;
}

// the first level: the frame (in linear light) where it is brighter than the threshold,
// fading in over a soft knee below it
@fragment
fn fs_bloom_bright(in: VsOut) -> @location(0) vec4<f32> {
    let c = down13(in, 2.2);
    let l = max(c.r, max(c.g, c.b));
    let t = prm.d.y;
    let knee = max(t * 0.5, 1e-4);
    var s = clamp(l - t + knee, 0.0, 2.0 * knee);
    s = s * s / (4.0 * knee);
    return vec4<f32>(c * (max(s, l - t) / max(l, 1e-4)), 1.0);
}

// the next level down
@fragment
fn fs_bloom_down(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(down13(in, 1.0), 1.0);
}

// a level added to the one above it: a 3x3 tent of its pixels, smoothing their blocks out
@fragment
fn fs_bloom_up(in: VsOut) -> @location(0) vec4<f32> {
    let d = 1.0 / vec2<f32>(textureDimensions(src));
    var sum = vec3<f32>(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let w = f32((2 - abs(x)) * (2 - abs(y)));
            sum += tap(in.uv + d * vec2<f32>(f32(x), f32(y)), 1.0) * w;
        }
    }
    return vec4<f32>(sum / 16.0, 1.0);
}
