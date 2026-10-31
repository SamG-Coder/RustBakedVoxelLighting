//! Terrain-relative full-map voxel occupancy. Geometry is prepared once; transport is baked on Vulkan.
use crate::world::{Field, Impostor, Manifest};
use bvl_engine::assets::TextureData;
pub const N: usize = 256;
pub const LAYERS: usize = 16;
pub const HEIGHT: f32 = 64.;
/// Fine height lookup used only for deforming the lower root collar in shaders.
/// Sampling the rendered surface also keeps all raster/shadow LODs consistent.
pub fn root_heightfield(field: &Field) -> TextureData {
    const SIZE: usize = 2049;
    let mut pixels = Vec::with_capacity(SIZE * SIZE * 4);
    for z in 0..SIZE {
        for x in 0..SIZE {
            let wx = x as f32 / (SIZE - 1) as f32 * field.extent - field.extent * 0.5;
            let wz = z as f32 / (SIZE - 1) as f32 * field.extent - field.extent * 0.5;
            let h = field.surface_height(wx, wz);
            let packed = (((h + 128.) / 1024.).clamp(0., 1.) * 65535.).round() as u16;
            pixels.extend_from_slice(&[(packed >> 8) as u8, packed as u8, 0, 255]);
        }
    }
    TextureData {
        width: SIZE as u32,
        height: SIZE as u32,
        pixels,
        srgb: false,
        mips: Vec::new(),
    }
}
pub fn build(
    field: &Field,
    manifest: &Manifest,
    captures: &[Impostor],
) -> (TextureData, TextureData) {
    let mut pixels = vec![0u8; N * N * LAYERS * 4];
    let mut height = Vec::with_capacity(N * N * 4);
    for z in 0..N {
        for x in 0..N {
            let wx = (x as f32 + 0.5) / N as f32 * 4000. - 2000.;
            let wz = (z as f32 + 0.5) / N as f32 * 4000. - 2000.;
            let h = field.height(wx, wz);
            let packed = (((h + 128.) / 1024.).clamp(0., 1.) * 65535.).round() as u16;
            height.extend_from_slice(&[
                (packed >> 8) as u8,
                packed as u8,
                (field.sample(wx, wz, 2, false) * 255.).clamp(0., 255.) as u8,
                255,
            ]);
            // The bottom layer contains the represented soil/grass, not an opaque billboard.
            let o = (z * LAYERS * N + x) * 4;
            pixels[o..o + 4].copy_from_slice(&[42, 50, 18, 38]);
        }
    }
    let step = 4000. / N as f32;
    for tree in manifest.trees.iter().chain(&manifest.shrubs) {
        let form = &captures[tree.variant];
        let height = form.height * tree.scale;
        let r = form.width * tree.scale * 0.48;
        let center_y = tree.y + form.bottom * tree.scale + height * 0.68;
        let ry = height * 0.32;
        let minx = ((tree.x - r + 2000.) / step).floor().max(0.) as usize;
        let maxx = ((tree.x + r + 2000.) / step).ceil().min(N as f32 - 1.) as usize;
        let minz = ((tree.z - r + 2000.) / step).floor().max(0.) as usize;
        let maxz = ((tree.z + r + 2000.) / step).ceil().min(N as f32 - 1.) as usize;
        for z in minz..=maxz {
            for x in minx..=maxx {
                let wx = (x as f32 + 0.5) * step - 2000.;
                let wz = (z as f32 + 0.5) * step - 2000.;
                let ground = field.height(wx, wz);
                let dx = (wx - tree.x).abs().max(0.0);
                let dz = (wz - tree.z).abs().max(0.0);
                // Area coverage preserves small crowns in coarse distant voxels.
                let rx = (r + step * 0.5 - dx).clamp(0., step) / step;
                let rz = (r + step * 0.5 - dz).clamp(0., step) / step;
                for y in 0..LAYERS {
                    let wy = ground + (y as f32 + 0.5) * HEIGHT / LAYERS as f32;
                    let vertical = (1. - ((wy - center_y) / ry.max(1.)).powi(2)).max(0.);
                    let a = (rx * rz * vertical * 0.7).clamp(0., 0.85);
                    if a < 0.002 {
                        continue;
                    }
                    let i = ((z * LAYERS + y) * N + x) * 4;
                    let old = pixels[i + 3] as f32 / 255.;
                    let total = 1. - (1. - old) * (1. - a);
                    for c in 0..3 {
                        let leaf = [0.055, 0.105, 0.018][c];
                        pixels[i + c] =
                            (((pixels[i + c] as f32 / 255.) * old * (1. - a) + leaf * a) / total
                                * 255.) as u8;
                    }
                    pixels[i + 3] = (total * 255.) as u8;
                }
            }
        }
    }

    (
        TextureData {
            width: N as u32,
            height: (N * LAYERS) as u32,
            pixels,
            srgb: false,
            mips: Vec::new(),
        },
        TextureData {
            width: N as u32,
            height: N as u32,
            pixels: height,
            srgb: false,
            mips: Vec::new(),
        },
    )
}

/// Static 8-direction terrain horizon and bent-sky field. Fine ground shading augments
/// the coarse 3D transport; no camera-dependent ray marches in the render loop.
pub fn ground_visibility(field: &Field) -> TextureData {
    const SIZE: usize = 512;
    let mut pixels = Vec::with_capacity(SIZE * SIZE * 4);
    for z in 0..SIZE {
        for x in 0..SIZE {
            let wx = (x as f32 + 0.5) / SIZE as f32 * 4000. - 2000.;
            let wz = (z as f32 + 0.5) / SIZE as f32 * 4000. - 2000.;
            let ground = field.height(wx, wz) + 0.4;
            let mut bent = glam::Vec3::Y;
            let mut visibility = 0.;
            for direction in 0..8 {
                let angle = direction as f32 * std::f32::consts::TAU / 8.;
                let (dz, dx) = angle.sin_cos();
                let mut slope = 0_f32;
                for distance in [4., 8., 16., 28., 48., 80., 128., 192.] {
                    slope = slope.max(
                        (field.height(wx + dx * distance, wz + dz * distance) - ground) / distance,
                    );
                }
                let openness = 1. / (1. + slope * slope);
                visibility += openness / 8.;
                bent += glam::Vec3::new(dx, 0.5, dz) * openness;
            }
            let normal = bent.normalize() * 0.5 + glam::Vec3::splat(0.5);
            pixels.extend(normal.to_array().map(|c| (c * 255.).round() as u8));
            pixels.push((visibility * 255.).round() as u8);
        }
    }
    TextureData {
        width: SIZE as u32,
        height: SIZE as u32,
        pixels,
        srgb: false,
        mips: Vec::new(),
    }
}
