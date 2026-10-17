// The atlas' mip levels, each from the one above: a texel is its 2x2 texels averaged over
// the ones keyed in (alpha >= 0.25: colour-keyed texels take no part, so cut-outs keep their
// colour to the edge), and keyed out itself where fewer than two are.

@group(0) @binding(0) var src: texture_2d<f32>;

// one triangle over the level
@vertex
fn vs_mip(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_mip(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(pos.xy) * 2;
    let a = textureLoad(src, p, 0);
    let b = textureLoad(src, p + vec2<i32>(1, 0), 0);
    let c = textureLoad(src, p + vec2<i32>(0, 1), 0);
    let d = textureLoad(src, p + vec2<i32>(1, 1), 0);
    let w = step(vec4<f32>(0.25), vec4<f32>(a.a, b.a, c.a, d.a));
    let n = w.x + w.y + w.z + w.w;
    if (n < 2.0) {
        return vec4<f32>(0.0);
    }
    // (alpha too: the STP texels' 0.5 against the opaque ones' 1)
    return (a * w.x + b * w.y + c * w.z + d * w.w) / n;
}
