//! Versioned, renderer-neutral browser package; generated from the native scene importer.
use bvl_engine::assets::Material;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
pub struct TextureInfo {
    pub width: u32,
    pub height: u32,
    pub srgb: bool,
    pub sizes: Vec<usize>,
}
#[derive(Serialize, Deserialize)]
pub struct Package {
    pub version: u32,
    pub vertices: usize,
    pub indices: usize,
    pub materials: Vec<Material>,
    pub textures: Vec<TextureInfo>,
}
#[cfg(not(target_arch = "wasm32"))]
pub fn export(root: &std::path::Path, out: &std::path::Path) -> anyhow::Result<()> {
    let (map, scene) = crate::map::load(root)?;
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("map.bin"), bincode::serialize(&map)?)?;
    std::fs::write(
        out.join("vertices.bin"),
        bytemuck::cast_slice(&scene.vertices),
    )?;
    std::fs::write(
        out.join("indices.bin"),
        bytemuck::cast_slice(&scene.indices),
    )?;
    let mut textures = Vec::new();
    for (id, mut t) in scene.textures.into_iter().enumerate() {
        // Preserve the coverage-corrected native foliage mips; complete other chains offline.
        if t.mips.is_empty() {
            let (mut w, mut h) = (t.width, t.height);
            let mut prev = t.pixels.clone();
            while w > 1 || h > 1 {
                let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
                let mut next = vec![0u8; (nw * nh * 4) as usize];
                for y in 0..nh {
                    for x in 0..nw {
                        for c in 0..4 {
                            let mut sum = 0.;
                            for dy in 0..2 {
                                for dx in 0..2 {
                                    let v = prev[(((y * 2 + dy).min(h - 1) * w
                                        + (x * 2 + dx).min(w - 1))
                                        * 4
                                        + c)
                                        as usize]
                                        as f32
                                        / 255.;
                                    sum += if t.srgb && c < 3 { v.powf(2.2) } else { v };
                                }
                            }
                            let v = sum * 0.25;
                            next[((y * nw + x) * 4 + c) as usize] =
                                ((if t.srgb && c < 3 { v.powf(1. / 2.2) } else { v }) * 255.)
                                    .round() as u8;
                        }
                    }
                }
                t.mips.push(next.clone());
                prev = next;
                w = nw;
                h = nh;
            }
        }
        let mut sizes = vec![t.pixels.len()];
        let mut raw = t.pixels;
        for mip in t.mips {
            sizes.push(mip.len());
            raw.extend(mip);
        }
        std::fs::write(out.join(format!("texture-{id}.bin")), raw)?;
        textures.push(TextureInfo {
            width: t.width,
            height: t.height,
            srgb: t.srgb,
            sizes,
        });
    }
    let p = Package {
        version: 1,
        vertices: scene.vertices.len(),
        indices: scene.indices.len(),
        materials: scene.materials,
        textures,
    };
    std::fs::write(out.join("package.json"), serde_json::to_vec(&p)?)?;
    println!(
        "Browser package: {} vertices, {} indices, {} textures",
        p.vertices,
        p.indices,
        p.textures.len()
    );
    Ok(())
}
