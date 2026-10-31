//! Engine-neutral mesh format and explicit mipmapped texture loading.
use anyhow::{Result, ensure};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use std::path::Path;

#[derive(Default, Clone)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub uvs1: Vec<[f32; 2]>,
    pub colours: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}
impl MeshData {
    pub fn read(path: &Path) -> Result<Self> {
        let raw = anyhow::Context::with_context(std::fs::read(path), || {
            format!("Reading {}", path.display())
        })?;
        Self::decode(&raw)
    }
    pub fn decode(raw: &[u8]) -> Result<Self> {
        ensure!(
            raw.len() >= 12 && &raw[..4] == b"BVM1",
            "Invalid mesh header"
        );
        let n = u32::from_le_bytes(raw[4..8].try_into()?) as usize;
        let m = u32::from_le_bytes(raw[8..12].try_into()?) as usize;
        ensure!(
            n <= 10_000_000 && m <= 60_000_000 && raw.len() == 12 + n * 48 + m * 4,
            "Invalid mesh size"
        );
        let mut result = Self {
            positions: Vec::with_capacity(n),
            normals: Vec::with_capacity(n),
            uvs: Vec::with_capacity(n),
            uvs1: Vec::new(),
            colours: Vec::with_capacity(n),
            indices: Vec::with_capacity(m),
        };
        for bytes in raw[12..12 + n * 48].chunks_exact(48) {
            let f: [f32; 12] = std::array::from_fn(|i| {
                f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
            });
            ensure!(f.iter().all(|x| x.is_finite()), "Non-finite vertex");
            result.positions.push([f[0], f[1], f[2]]);
            result.normals.push([f[3], f[4], f[5]]);
            result.uvs.push([f[6], f[7]]);
            result.colours.push([f[8], f[9], f[10], f[11]]);
        }
        result.indices = raw[12 + n * 48..]
            .chunks_exact(4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
            .collect();
        ensure!(
            result.indices.iter().all(|&i| (i as usize) < n),
            "Mesh index out of range"
        );
        Ok(result)
    }
    pub fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        if !self.uvs1.is_empty() {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, self.uvs1);
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colours);
        mesh.insert_indices(Indices::U32(self.indices));
        mesh
    }
    pub fn append(&mut self, other: &Self, transform: Mat4) {
        let base = self.positions.len() as u32;
        let normal_matrix = transform.inverse().transpose();
        self.positions.extend(
            other
                .positions
                .iter()
                .map(|&p| transform.transform_point3(Vec3::from(p)).to_array()),
        );
        self.normals.extend(other.normals.iter().map(|&n| {
            normal_matrix
                .transform_vector3(Vec3::from(n))
                .normalize_or_zero()
                .to_array()
        }));
        self.uvs.extend_from_slice(&other.uvs);
        self.colours.extend_from_slice(&other.colours);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }
    pub fn quad(&mut self, points: [Vec3; 4], uv: [[f32; 2]; 4], normal: Vec3, colour: [f32; 4]) {
        let base = self.positions.len() as u32;
        for (p, t) in points.into_iter().zip(uv) {
            self.positions.push(p.to_array());
            self.normals.push(normal.to_array());
            self.uvs.push(t);
            self.colours.push(colour);
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

pub fn texture(path: &Path, srgb: bool, cutout: bool) -> Result<Image> {
    let mut pixels =
        anyhow::Context::with_context(image::open(path), || format!("Loading {}", path.display()))?
            .into_rgba8();
    let (w, h) = pixels.dimensions();
    let target_coverage = if cutout {
        pixels.pixels().filter(|p| p[3] > 102).count() as f32 / (w * h) as f32
    } else {
        0.
    };
    let mut data = Vec::with_capacity((w * h * 6) as usize);
    let mut levels = 0;
    loop {
        data.extend_from_slice(pixels.as_raw());
        levels += 1;
        if pixels.width() == 1 && pixels.height() == 1 {
            break;
        }
        pixels = image::imageops::resize(
            &pixels,
            (pixels.width() / 2).max(1),
            (pixels.height() / 2).max(1),
            image::imageops::FilterType::Triangle,
        );
        if cutout {
            let mut low = 0.;
            let mut high = 4.;
            for _ in 0..10 {
                let scale = (low + high) * 0.5;
                let cover = pixels
                    .pixels()
                    .filter(|p| p[3] as f32 * scale > 102.)
                    .count() as f32
                    / (pixels.width() * pixels.height()) as f32;
                if cover < target_coverage {
                    low = scale;
                } else {
                    high = scale;
                }
            }
            for p in pixels.pixels_mut() {
                p[3] = (p[3] as f32 * (low + high) * 0.5).min(255.) as u8;
            }
        }
    }
    let mut img = Image::new_uninit(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        if srgb {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    img.data = Some(data);
    img.texture_descriptor.mip_level_count = levels;
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_truncated_and_out_of_range_meshes() {
        assert!(MeshData::decode(b"BVM1").is_err());
        let mut bytes = b"BVM1".to_vec();
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend([0u8; 48]);
        bytes.extend(1u32.to_le_bytes());
        assert!(MeshData::decode(&bytes).is_err());
        bytes[60..64].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(MeshData::decode(&bytes).unwrap().positions.len(), 1);
        bytes[12..16].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(MeshData::decode(&bytes).is_err());
    }
}
