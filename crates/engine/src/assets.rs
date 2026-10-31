//! Renderer-neutral, validated asset data and packed scene buffers.
use anyhow::{Result, ensure};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use std::path::Path;
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub uv1: [f32; 2],
}
#[derive(Default, Clone)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}
impl MeshData {
    pub fn read(path: &Path) -> Result<Self> {
        Self::decode(&std::fs::read(path)?)
    }
    pub fn decode(raw: &[u8]) -> Result<Self> {
        ensure!(
            raw.len() >= 12 && &raw[..4] == b"BVM1",
            "Invalid mesh header"
        );
        let n = u32::from_le_bytes(raw[4..8].try_into()?) as usize;
        let m = u32::from_le_bytes(raw[8..12].try_into()?) as usize;
        ensure!(
            n < 10_000_000 && m < 60_000_000 && raw.len() == 12 + n * 48 + m * 4,
            "Invalid mesh length"
        );
        let mut mesh = Self {
            vertices: Vec::with_capacity(n),
            indices: Vec::with_capacity(m),
        };
        for bytes in raw[12..12 + n * 48].as_chunks::<48>().0 {
            let f: [f32; 12] = std::array::from_fn(|i| {
                f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
            });
            ensure!(f.iter().all(|v| v.is_finite()), "Non-finite mesh vertex");
            mesh.vertices.push(Vertex {
                position: [f[0], f[1], f[2]],
                normal: [f[3], f[4], f[5]],
                uv: [f[6], f[7]],
                color: [f[8], f[9], f[10], f[11]],
                uv1: [0.; 2],
            });
        }
        mesh.indices = raw[12 + n * 48..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|s| u32::from_le_bytes(*s))
            .collect();
        ensure!(
            mesh.indices.iter().all(|&i| (i as usize) < n),
            "Invalid mesh index"
        );
        Ok(mesh)
    }
    pub fn quad(
        &mut self,
        p: [Vec3; 4],
        uv: [[f32; 2]; 4],
        n: Vec3,
        color: [f32; 4],
        extra: [f32; 2],
    ) {
        let b = self.vertices.len() as u32;
        self.vertices
            .extend(p.into_iter().zip(uv).map(|(p, uv)| Vertex {
                position: p.to_array(),
                normal: n.to_array(),
                uv,
                color,
                uv1: extra,
            }));
        self.indices.extend([b, b + 1, b + 2, b, b + 2, b + 3]);
    }
}
#[derive(Clone, Copy, Default)]
pub struct MeshRange {
    pub first_index: u32,
    pub index_count: u32,
    pub vertex_offset: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Instance {
    pub position_scale: [f32; 4],
    pub rotation_tint: [f32; 4],
    pub fade: [f32; 4],
}
impl Default for Instance {
    fn default() -> Self {
        Self {
            position_scale: [0., 0., 0., 1.],
            rotation_tint: [0., 1., 1., 1.],
            fade: [1., 0., 0., 0.],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Material {
    pub textures: [u32; 4],
    pub settings: [f32; 4],
    pub tint: [f32; 4],
    pub extra: [u32; 4],
}
impl Default for Material {
    fn default() -> Self {
        Self {
            textures: [0; 4],
            settings: [0., 0.4, 0.9, 0.],
            tint: [1.; 4],
            extra: [0; 4],
        }
    }
}
#[derive(Clone, Copy)]
pub struct Draw {
    pub mesh: MeshRange,
    pub material: u32,
    pub first_instance: u32,
    pub instances: u32,
}
pub struct TextureData {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub srgb: bool,
    pub mips: Vec<Vec<u8>>,
}
impl TextureData {
    pub fn read(path: &Path, srgb: bool) -> Result<Self> {
        let encoded = std::fs::read(path)?;
        let p = image::load_from_memory(&encoded)?.into_rgba8();
        let mut mips = Vec::new();
        let sidecar = path.with_extension("mips");
        if sidecar.exists() {
            let data = std::fs::read(&sidecar)?;
            ensure!(
                data.len() >= 24 && &data[..4] == b"BVM2",
                "Invalid mip bake: {}",
                sidecar.display()
            );
            let read = |i| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
            let hash = encoded.iter().fold(14695981039346656037_u64, |h, b| {
                (h ^ *b as u64).wrapping_mul(1099511628211)
            });
            ensure!(
                read(4) == p.width()
                    && read(8) == p.height()
                    && u64::from_le_bytes(data[16..24].try_into().unwrap()) == hash,
                "Stale foliage mip bake: {}. Run tools/bake-foliage-mips.py",
                sidecar.display()
            );
            ensure!(
                read(12) == 31 - p.width().max(p.height()).leading_zeros(),
                "Invalid mip count"
            );
            let (mut w, mut h, mut offset) = (p.width(), p.height(), 24_usize);
            for _ in 0..read(12) {
                w = (w / 2).max(1);
                h = (h / 2).max(1);
                let end = offset + (w * h * 4) as usize;
                ensure!(end <= data.len(), "Truncated mip bake");
                mips.push(data[offset..end].to_vec());
                offset = end;
            }
            ensure!(offset == data.len(), "Trailing mip bake data");
        }
        Ok(Self {
            width: p.width(),
            height: p.height(),
            pixels: p.into_raw(),
            mips,
            srgb,
        })
    }
}
#[derive(Clone, Copy)]
pub struct RayMesh {
    pub mesh: MeshRange,
    pub material: u32,
}
#[derive(Clone, Copy)]
pub struct RayInstance {
    pub mesh: u32,
    pub transform: [f32; 12],
}
#[derive(Clone, Copy)]
pub struct TransportBake {
    pub occupancy: u32,
    pub heights: u32,
    pub size: u32,
    pub layers: u32,
}
#[derive(Default)]
pub struct SceneData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub transport: Option<TransportBake>,
    pub ray_meshes: Vec<RayMesh>,
    pub ray_instances: Vec<RayInstance>,
    pub textures: Vec<TextureData>,
    pub materials: Vec<Material>,
}
impl SceneData {
    pub fn mesh(&mut self, mesh: MeshData) -> MeshRange {
        let range = MeshRange {
            first_index: self.indices.len() as u32,
            index_count: mesh.indices.len() as u32,
            vertex_offset: self.vertices.len() as i32,
        };
        self.vertices.extend(mesh.vertices);
        self.indices.extend(mesh.indices);
        range
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_truncated_mesh_and_out_of_bounds_index() {
        assert!(MeshData::decode(b"BVM1").is_err());
        let mut raw = b"BVM1".to_vec();
        raw.extend(1u32.to_le_bytes());
        raw.extend(1u32.to_le_bytes());
        raw.extend([0; 48]);
        raw.extend(1u32.to_le_bytes());
        assert!(MeshData::decode(&raw).is_err());
        raw[60..64].copy_from_slice(&0u32.to_le_bytes());
        assert_eq!(MeshData::decode(&raw).unwrap().vertices.len(), 1);
        raw[12..16].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(MeshData::decode(&raw).is_err());
    }
}
