//! `export-car`: everything the disc has for a car in one folder, in formats other tools
//! open: its models as glTF (with the paints as material variants), the texture pages and
//! every texture shape as PNG, the engine sounds as WAV, and the original files.

use crate::assets::{self, CAR_TEXTURES, GameData, ps1_to_world};
use crate::formats::{self, geo::Geo, psh::Psh, viv::Viv};
use anyhow::{Context as _, Result, bail};
use glam::Vec3;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::Path;

/// The model files of a car: `ZZZ<code>H.VIV` (the player's car, also the showroom's),
/// `ZZZ<code>.VIV` and `ZZZ<code>S.VIV` (the opponents'), by the name they get here.
const MODELS: [(&str, &str); 3] = [("H", "_high"), ("", ""), ("S", "_low")];

/// Export car `code` ("all": every car with a model) to `out` (`out/<code>` for "all").
pub fn export_car(data: &GameData, code: &str, out: &Path) -> Result<()> {
    if code.eq_ignore_ascii_case("all") {
        for code in all_cars(data)? {
            export_one(data, &code, &out.join(&code))?;
        }
        return Ok(());
    }
    export_one(data, &code.to_ascii_uppercase(), out)
}

/// Every car code with a model on the disc (race, police and traffic cars).
fn all_cars(data: &GameData) -> Result<Vec<String>> {
    let mut codes = BTreeSet::new();
    for e in std::fs::read_dir(&data.root)? {
        let name = e?.file_name().to_string_lossy().to_ascii_uppercase();
        let Some(stem) = name.strip_prefix("ZZZ").and_then(|n| n.strip_suffix(".VIV")) else { continue };
        // (ZZZZ...: the music and other archives; codes have four letters but the van's three)
        if stem.len() < 3 || stem.starts_with('Z') {
            continue;
        }
        let code = &stem[..stem.len().min(4)];
        let has_geo = data.read(&name).ok().and_then(|d| Viv::parse(&d).ok()).is_some_and(|v| v.find_suffix(".geo").is_some());
        if has_geo {
            codes.insert(code.to_string());
        }
    }
    // the police cars: their sounds, with the liveries' models
    for e in std::fs::read_dir(&data.root)? {
        let name = e?.file_name().to_string_lossy().to_ascii_uppercase();
        // (ZZZZ?ENG.VIV: speech)
        let Some(code) = name.strip_prefix('Z').and_then(|n| n.strip_suffix("ENG.VIV")).filter(|c| c.len() == 4 && !c.starts_with('Z')) else { continue };
        if !codes.contains(code) {
            codes.retain(|c| c[1..] != code[1..]);
            codes.insert(code.to_string());
        }
    }
    Ok(codes.into_iter().collect())
}

fn export_one(data: &GameData, code: &str, out: &Path) -> Result<()> {
    // the police cars have their liveries' models ("CBMW": ABMW, BBMW, FBMW, GBMW)
    let mut bodies = vec![code.to_string()];
    if !MODELS.iter().any(|(s, _)| data.root.join(format!("ZZZ{code}{s}.VIV")).is_file()) {
        bodies = ('A'..='Z')
            .map(|c| format!("{c}{}", &code[1..]))
            .filter(|b| MODELS.iter().any(|(s, _)| data.root.join(format!("ZZZ{b}{s}.VIV")).is_file()))
            .collect();
    }
    let sounds: Vec<String> = ["ENG", "ENS"].iter().map(|s| format!("Z{code}{s}.VIV")).filter(|f| data.root.join(f).is_file()).collect();
    if bodies.is_empty() && sounds.is_empty() {
        bail!("no car {code} on the disc (no ZZZ{code}*.VIV or Z{code}ENG.VIV)");
    }
    std::fs::create_dir_all(out.join("textures/shapes"))?;
    std::fs::create_dir_all(out.join("original"))?;
    let mut readme = format!("{code}: exported by nfshs export-car from the disc's files (copied as they are to original/).\n\n");
    let paint_list = data.read("ZFECARS.CAR").unwrap_or_default();
    let mut shapes_done = BTreeSet::new();
    let mut painted = false;
    for body in &bodies {
        let paints = match crate::ps1::host::car_paints(&paint_list, body) {
            p if p.is_empty() => vec![0],
            p => p,
        };
        painted |= paints.len() > 1;
        for (suffix, label) in MODELS {
            let file = format!("ZZZ{body}{suffix}.VIV");
            let Ok(raw) = data.read(&file) else { continue };
            std::fs::write(out.join("original").join(&file), &raw)?;
            let viv = Viv::parse(&raw)?;
            let name = format!("{body}{label}");
            let info = export_model(data, &viv, body, &name, &paints, out).with_context(|| format!("exporting {file}"))?;
            writeln!(readme, "{name}.gltf: {file}, {info}")?;
            // every texture shape as it is in its file (unpainted)
            for (n, d) in viv.entries.iter().filter(|(n, _)| n.to_ascii_lowercase().ends_with(".psh")) {
                if shapes_done.insert(n.to_ascii_lowercase()) {
                    export_shapes(&Psh::parse(d)?, &out.join("textures/shapes").join(n.to_ascii_lowercase().trim_end_matches(".psh")))?;
                }
            }
        }
        // the body sheets also on their own (the race reads them from the VIVs)
        for f in ["D", "L"].map(|s| format!("ZZZ{body}{s}.PSH")) {
            if let Ok(d) = data.read(&f) {
                std::fs::write(out.join("original").join(&f), d)?;
            }
        }
    }
    if !bodies.is_empty() {
        readme += "\nThe models are in metres, Y up, +Z forward, origin on the ground under the car's centre.\n\
            Root node \"car\" holds the parts a race draws; \"other parts\" the rest (lower-detail and\n\
            damaged versions, lit lights, light bars), each named by its GEO part number and textures.\n\
            Wheels are separate nodes placed at their hub.\n";
    }
    if painted {
        readme += "The paints are KHR_materials_variants (textures/<model>_paintNN.png, NN the game's\n\
            paint number: 0-7 the light body sheet, 8-15 the dark one), the first the default.\n";
    }
    // the engine sounds: the player's (ENG) and the opponents' (ENS), with the rpm curves
    if !sounds.is_empty() {
        std::fs::create_dir_all(out.join("sounds"))?;
        readme += "\n";
    }
    for f in &sounds {
        let raw = data.read(f)?;
        std::fs::write(out.join("original").join(f), &raw)?;
        let who = if f.ends_with("ENG.VIV") { "engine" } else { "opponent_engine" };
        let (mut n, mut looped) = (0, 0);
        for (entry, d) in Viv::parse(&raw)?.entries {
            if d.starts_with(b"BNKl") {
                for (i, s) in formats::audio::Bank::parse(&d)?.sounds.iter().enumerate() {
                    if let Some(s) = s.as_ref().filter(|s| !s.pcm.is_empty()) {
                        formats::audio::write_wav(&out.join(format!("sounds/{who}_{i:02}.wav")), s)?;
                        n += 1;
                        looped += s.loop_start.is_some() as usize;
                    }
                }
            } else {
                // the rpm -> volume / pitch curves (CRDl) and default.cfg, as they are
                std::fs::write(out.join("sounds").join(format!("{who}_{}", file_name(&entry))), &d)?;
            }
        }
        let loops = if looped > 0 { format!(", {looped} with their loop in the WAV's smpl chunk") } else { String::new() };
        writeln!(readme, "sounds/{who}_NN.wav: {n} sounds of {f} (NN its number in the bank{loops})")?;
    }
    // the engine data, compressed on the disc
    for f in ["QDA", "QCS"].map(|s| format!("Z{code}.{s}")) {
        if let Ok(d) = data.read(&f) {
            std::fs::write(out.join("original").join(&f), &d)?;
            std::fs::create_dir_all(out.join("data"))?;
            std::fs::write(out.join("data").join(format!("{}.bin", f.to_ascii_lowercase())), formats::compress::decompress(&d)?)?;
        }
    }
    if out.join("data").is_dir() {
        writeln!(readme, "\ndata/: Z{code}.QDA (engine: redline, gear speeds, acceleration curve) and .QCS, decompressed")?;
    }
    std::fs::write(out.join("README.txt"), readme)?;
    println!("{code} -> {}", out.display());
    Ok(())
}

/// Each shape of a PSH as `dir/<name>.png`.
fn export_shapes(psh: &Psh, dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut seen: HashMap<String, usize> = HashMap::new();
    for s in psh.shapes.iter().filter(|s| s.width > 0 && s.height > 0) {
        let mut name = file_name(s.name.trim());
        // (names differ in case only, or repeat: Windows folders would merge them)
        let k = seen.entry(name.to_ascii_lowercase()).or_default();
        *k += 1;
        if *k > 1 {
            name = format!("{name}_{k}");
        }
        image::RgbaImage::from_raw(s.width as u32, s.height as u32, s.rgba.clone()).context("shape size")?.save(dir.join(format!("{name}.png")))?;
    }
    Ok(())
}

/// The parts the original draws of a traffic car (R3DCar_ObjectVisible in a race; the race
/// cars' are [`assets::CAR_PARTS_VISIBLE`]).
const TRAFFIC_PARTS_VISIBLE: [usize; 17] = [0, 2, 3, 13, 17, 20, 23, 26, 29, 31, 35, 47, 48, 51, 52, 55, 56];

/// One primitive being built: its vertices (by GEO vertex and UV) and triangles.
#[derive(Default)]
struct Prim {
    keys: HashMap<(u8, [u8; 2]), u16>,
    pos: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    index: Vec<u16>,
}

/// The binary buffer and the JSON arrays that describe it.
#[derive(Default)]
struct Gltf {
    bin: Vec<u8>,
    views: Vec<String>,
    accessors: Vec<String>,
}

impl Gltf {
    /// Append `data` as a buffer view and an accessor of `count` `ty` elements of `component`.
    fn accessor(&mut self, data: &[u8], count: usize, component: u32, ty: &str, target: u32, bounds: Option<([f32; 3], [f32; 3])>) -> usize {
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }
        let view = self.views.len();
        self.views.push(format!(r#"{{"buffer":0,"byteOffset":{},"byteLength":{},"target":{target}}}"#, self.bin.len(), data.len()));
        self.bin.extend_from_slice(data);
        let b = bounds.map(|(lo, hi)| format!(r#","min":[{},{},{}],"max":[{},{},{}]"#, lo[0], lo[1], lo[2], hi[0], hi[1], hi[2])).unwrap_or_default();
        self.accessors.push(format!(r#"{{"bufferView":{view},"componentType":{component},"count":{count},"type":"{ty}"{b}}}"#));
        self.accessors.len() - 1
    }
}

/// Write `name.gltf` / `name.bin` and the textures for the model of a car VIV; returns a
/// short description for the readme.
fn export_model(data: &GameData, viv: &Viv, code: &str, name: &str, paints: &[u8], out: &Path) -> Result<String> {
    let (_, geo) = viv.find_suffix(".geo").context("no .geo")?;
    let geo = Geo::parse(geo)?;
    // the texture pages for each paint (page 1, the interior, does not change with it)
    let mut images = Vec::new();
    let mut page_of = HashMap::new();
    let mut interior = None;
    for &paint in paints {
        let (pages, of) = assets::car_pages(data, viv, code, None, paint)?;
        let file = format!("textures/{name}_paint{paint:02}.png");
        image::RgbaImage::from_raw(256, 256, pages[0].rgba.clone()).context("page")?.save(out.join(&file))?;
        images.push(file);
        if interior.is_none() && of.values().any(|&p| p == 1) {
            let file = format!("textures/{name}_interior.png");
            image::RgbaImage::from_raw(256, 256, pages[1].rgba.clone()).context("page")?.save(out.join(&file))?;
            interior = Some(file);
        }
        page_of = of;
    }
    let has_normals = geo.parts.iter().any(|p| !p.normals.is_empty());
    let mut g = Gltf::default();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let (mut drawn, mut other) = (Vec::new(), Vec::new());
    let mut variant_prims = 0;
    for (pi, part) in geo.parts.iter().enumerate() {
        if part.polys.is_empty() {
            continue;
        }
        let off = Vec3::new(part.position[0] as f32, part.position[1] as f32, part.position[2] as f32) / 65536.0;
        // wheels (47..56) turn around their hub, as load_car keeps them
        let hub = if (47..=56).contains(&pi) { ps1_to_world(off) } else { Vec3::ZERO };
        // by (texture page, semi-transparent: DrawC_Prim's flags bit 0, the windows)
        let mut prims: BTreeMap<(usize, bool), Prim> = BTreeMap::new();
        let mut textures = BTreeSet::new();
        for poly in &part.polys {
            let tex = CAR_TEXTURES.get(poly.texture as usize).copied().unwrap_or("");
            textures.insert(tex.trim());
            let prim = prims.entry((page_of.get(tex).copied().unwrap_or(0), poly.flags & 1 != 0)).or_default();
            let mut tri = [0u16; 3];
            for k in 0..3 {
                let vi = poly.v[k];
                let key = (vi, poly.uv[k]);
                tri[k] = match prim.keys.get(&key) {
                    Some(&i) => i,
                    None => {
                        let v = part.verts.get(vi as usize).copied().unwrap_or([0; 3]);
                        let p = ps1_to_world(off + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) / 256.0) - hub;
                        let n = part.normals.get(vi as usize).map(|n| ps1_to_world(Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32)).normalize_or(Vec3::Y)).unwrap_or(Vec3::Y);
                        prim.pos.push(p.into());
                        prim.normal.push(n.into());
                        prim.uv.push([(poly.uv[k][0] as f32 + 0.5) / 256.0, (poly.uv[k][1] as f32 + 0.5) / 256.0]);
                        let i = prim.pos.len() as u16 - 1;
                        prim.keys.insert(key, i);
                        i
                    }
                };
            }
            // the mirrored X turns the PS1's winding round: glTF's front faces are
            // counter-clockwise
            prim.index.extend([tri[0], tri[2], tri[1]]);
        }
        let mut prim_json = Vec::new();
        for ((page, blend), p) in &prims {
            let lo = p.pos.iter().fold([f32::MAX; 3], |m, v| [m[0].min(v[0]), m[1].min(v[1]), m[2].min(v[2])]);
            let hi = p.pos.iter().fold([f32::MIN; 3], |m, v| [m[0].max(v[0]), m[1].max(v[1]), m[2].max(v[2])]);
            let pos = g.accessor(bytemuck::cast_slice(&p.pos), p.pos.len(), 5126, "VEC3", 34962, Some((lo, hi)));
            let normal = has_normals.then(|| g.accessor(bytemuck::cast_slice(&p.normal), p.normal.len(), 5126, "VEC3", 34962, None));
            let uv = g.accessor(bytemuck::cast_slice(&p.uv), p.uv.len(), 5126, "VEC2", 34962, None);
            let index = g.accessor(bytemuck::cast_slice(&p.index), p.index.len(), 5123, "SCALAR", 34963, None);
            let attrs = match normal {
                Some(n) => format!(r#""POSITION":{pos},"NORMAL":{n},"TEXCOORD_0":{uv}"#),
                None => format!(r#""POSITION":{pos},"TEXCOORD_0":{uv}"#),
            };
            // materials: per paint the body page opaque and blended, then the interior's two
            let material = |paint: usize| if *page == 1 { paints.len() * 2 + *blend as usize } else { paint * 2 + *blend as usize };
            let variants = if *page == 0 && paints.len() > 1 {
                variant_prims += 1;
                let maps: Vec<String> = (0..paints.len()).map(|k| format!(r#"{{"material":{},"variants":[{k}]}}"#, material(k))).collect();
                format!(r#","extensions":{{"KHR_materials_variants":{{"mappings":[{}]}}}}"#, maps.join(","))
            } else {
                String::new()
            };
            prim_json.push(format!(r#"{{"attributes":{{{attrs}}},"indices":{index},"material":{}{variants}}}"#, material(0)));
        }
        let label = textures.iter().copied().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
        let node_name = format!("{pi:02} {label}");
        meshes.push(format!(r#"{{"name":"{}","primitives":[{}]}}"#, esc(&node_name), prim_json.join(",")));
        let t = if hub == Vec3::ZERO { String::new() } else { format!(r#","translation":[{},{},{}]"#, hub.x, hub.y, hub.z) };
        nodes.push(format!(r#"{{"name":"{}","mesh":{}{t}}}"#, esc(&node_name), meshes.len() - 1));
        // the parts R3DCar_ObjectVisible has on in a race
        let on = match geo.packed {
            true => TRAFFIC_PARTS_VISIBLE.contains(&pi),
            false => assets::CAR_PARTS_VISIBLE.get(pi).is_some_and(|&v| v != 0 && v < 17),
        };
        if on { &mut drawn } else { &mut other }.push(nodes.len() - 1);
    }
    let list = |v: &[usize]| v.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
    let mut roots = Vec::new();
    for (label, kids) in [("car", &drawn), ("other parts", &other)] {
        if !kids.is_empty() {
            nodes.push(format!(r#"{{"name":"{label}","children":[{}]}}"#, list(kids)));
            roots.push(nodes.len() - 1);
        }
    }
    // the materials: unlit-looking PBR (no metal, fully rough), the textures sharp as on the PS1;
    // alpha 0 is the transparent colour, the windows blend
    let mut images_json: Vec<String> = images.iter().map(|f| format!(r#"{{"uri":"{f}"}}"#)).collect();
    let mut materials = Vec::new();
    let mut material = |tex: usize, blend: bool, label: &str| {
        let alpha = if blend { r#""alphaMode":"BLEND""# } else { r#""alphaMode":"MASK","alphaCutoff":0.25"# };
        let suffix = if blend { " glass" } else { "" };
        materials.push(format!(
            r#"{{"name":"{label}{suffix}","pbrMetallicRoughness":{{"baseColorTexture":{{"index":{tex}}},"metallicFactor":0,"roughnessFactor":1}},{alpha}}}"#
        ));
    };
    for (k, &paint) in paints.iter().enumerate() {
        material(k, false, &format!("{name} paint {paint}"));
        material(k, true, &format!("{name} paint {paint}"));
    }
    let interior_tex = match &interior {
        Some(f) => {
            images_json.push(format!(r#"{{"uri":"{f}"}}"#));
            images_json.len() - 1
        }
        None => 0,
    };
    material(interior_tex, false, &format!("{name} interior"));
    material(interior_tex, true, &format!("{name} interior"));
    let textures_json: Vec<String> = (0..images_json.len()).map(|i| format!(r#"{{"sampler":0,"source":{i}}}"#)).collect();
    let (ext_used, ext_root) = if variant_prims > 0 {
        let v: Vec<String> = paints.iter().map(|p| format!(r#"{{"name":"paint {p}"}}"#)).collect();
        (r#","extensionsUsed":["KHR_materials_variants"]"#.to_string(), format!(r#","extensions":{{"KHR_materials_variants":{{"variants":[{}]}}}}"#, v.join(",")))
    } else {
        Default::default()
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"nfshs export-car"}}{ext_used}{ext_root},"scene":0,"scenes":[{{"name":"{name}","nodes":[{}]}}],"nodes":[{}],"meshes":[{}],"materials":[{}],"textures":[{}],"images":[{}],"samplers":[{{"magFilter":9728,"minFilter":9728}}],"buffers":[{{"uri":"{name}.bin","byteLength":{}}}],"bufferViews":[{}],"accessors":[{}]}}"#,
        list(&roots),
        nodes.join(","),
        meshes.join(","),
        materials.join(","),
        textures_json.join(","),
        images_json.join(","),
        g.bin.len(),
        g.views.join(","),
        g.accessors.join(","),
    );
    std::fs::write(out.join(format!("{name}.bin")), &g.bin)?;
    std::fs::write(out.join(format!("{name}.gltf")), json)?;
    let polys: usize = geo.parts.iter().map(|p| p.polys.len()).sum();
    Ok(format!("{} parts, {polys} triangles, paints {paints:?}", meshes.len()))
}

/// `s` as a file name on any system.
fn file_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    if n.is_empty() { "_".into() } else { n }
}

/// A string for a JSON literal.
fn esc(s: &str) -> String {
    s.chars().flat_map(|c| if c == '"' || c == '\\' { vec!['\\', c] } else if c.is_control() { vec![] } else { vec![c] }).collect()
}

/// `export-bgs`: the full-screen pictures (front-end backgrounds, loading screens) as PNG at
/// their own resolution. Such a picture is a grid of tiles sharing a name, each placed at
/// `(-center_x, -center_y)`; a name whose tiles cover at least 300×200 counts as one. It
/// looks in every `.PSH`, every compressed `.QPS` and every shape file inside a `.VIV`.
pub fn export_bgs(data: &GameData, out: &Path) -> Result<()> {
    let mut files: Vec<_> = std::fs::read_dir(&data.root)?
        .filter_map(|e| e.ok().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    files.sort();
    let mut count = 0;
    for file in files {
        let upper = file.to_ascii_uppercase();
        let d = data.read(&file)?;
        let pshs: Vec<(String, Vec<u8>)> = if upper.ends_with(".PSH") {
            vec![(String::new(), d)]
        } else if upper.ends_with(".QPS") {
            vec![(String::new(), formats::compress::decompress(&d)?)]
        } else if upper.ends_with(".VIV") {
            let Ok(viv) = Viv::parse(&d) else { continue };
            viv.entries.into_iter().filter(|(_, e)| e.starts_with(b"SHPP")).collect()
        } else {
            continue;
        };
        let stem = Path::new(&file).file_stem().unwrap().to_string_lossy().into_owned();
        for (entry, psh) in pshs {
            let Ok(psh) = Psh::parse(&psh) else { continue };
            let mut groups: BTreeMap<&str, Vec<&formats::psh::Shape>> = BTreeMap::new();
            for s in &psh.shapes {
                groups.entry(s.name.as_str()).or_default().push(s);
            }
            for (name, tiles) in groups {
                let x0 = tiles.iter().map(|s| -(s.center.0 as i32)).min().unwrap();
                let y0 = tiles.iter().map(|s| -(s.center.1 as i32)).min().unwrap();
                let x1 = tiles.iter().map(|s| s.width as i32 - s.center.0 as i32).max().unwrap();
                let y1 = tiles.iter().map(|s| s.height as i32 - s.center.1 as i32).max().unwrap();
                let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
                if w < 300 || h < 200 {
                    continue;
                }
                let mut img = image::RgbaImage::new(w, h);
                for s in &tiles {
                    let (ox, oy) = ((-(s.center.0 as i32) - x0) as u32, (-(s.center.1 as i32) - y0) as u32);
                    for (i, px) in s.rgba.chunks_exact(4).enumerate() {
                        // (opaque: the semi-transparency bit means nothing on a background)
                        let a = if px[3] == 0 { 0 } else { 255 };
                        img.put_pixel(ox + (i % s.width as usize) as u32, oy + (i / s.width as usize) as u32, image::Rgba([px[0], px[1], px[2], a]));
                    }
                }
                let entry = Path::new(&entry).file_stem().map(|e| format!("_{}", e.to_string_lossy())).unwrap_or_default();
                let name: String = name.trim().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
                let dir = out.join(&stem);
                std::fs::create_dir_all(&dir)?;
                let path = dir.join(format!("{stem}{entry}_{name}.png"));
                img.save(&path)?;
                println!("{w:4}x{h:<4} {} ({} tiles)", path.display(), tiles.len());
                count += 1;
            }
        }
    }
    println!("{count} pictures in {}", out.display());
    Ok(())
}
