//! Woodland data ownership and geographic sampling; no renderer dependencies.
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::path::Path;
#[derive(Deserialize, Clone)]
pub struct Plant {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub scale: f32,
    pub rotation: f32,
    pub variant: usize,
}
#[derive(Deserialize)]
pub struct Level {
    pub wood: String,
    pub leaf: String,
}
#[derive(Deserialize)]
pub struct Variant {
    pub bark: String,
    pub leaf: String,
    pub levels: Vec<Level>,
}
#[derive(Deserialize)]
pub struct Impostor {
    pub width: f32,
    pub bottom: f32,
    pub height: f32,
}
#[derive(Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub extent: f32,
    pub grid: usize,
    pub variants: Vec<Variant>,
    pub trees: Vec<Plant>,
    pub shrubs: Vec<Plant>,
    pub rivers: Vec<String>,
    #[serde(rename = "sourceCommit")]
    pub source_commit: String,
}
pub struct Field {
    pub size: usize,
    pub extent: f32,
    pub values: Vec<f32>,
    pub ecology: Vec<f32>,
}
impl Field {
    pub fn load(path: &Path, manifest: &Manifest) -> Result<Self> {
        fn floats(path: &Path, expected: usize) -> Result<Vec<f32>> {
            let bytes = std::fs::read(path)?;
            ensure!(bytes.len() == expected * 4, "Invalid terrain field length");
            let values: Vec<_> = bytes
                .chunks_exact(4)
                .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
                .collect();
            ensure!(
                values.iter().all(|x| x.is_finite()),
                "Invalid terrain value"
            );
            Ok(values)
        }
        let count = manifest.grid * manifest.grid * 4;
        Ok(Self {
            size: manifest.grid,
            extent: manifest.extent,
            values: floats(&path.join("terrain.f32"), count)?,
            ecology: floats(&path.join("ecology.f32"), count)?,
        })
    }
    pub fn sample(&self, x: f32, z: f32, channel: usize, ecology: bool) -> f32 {
        let x =
            ((x / self.extent + 0.5) * (self.size - 1) as f32).clamp(0., (self.size - 1) as f32);
        let z =
            ((z / self.extent + 0.5) * (self.size - 1) as f32).clamp(0., (self.size - 1) as f32);
        let ix = (x as usize).min(self.size - 2);
        let iz = (z as usize).min(self.size - 2);
        let u = x - ix as f32;
        let v = z - iz as f32;
        let data = if ecology { &self.ecology } else { &self.values };
        let i = (iz * self.size + ix) * 4 + channel;
        let a = data[i] * (1. - u) + data[i + 4] * u;
        let b = data[i + self.size * 4] * (1. - u) + data[i + self.size * 4 + 4] * u;
        a * (1. - v) + b * v
    }
    pub fn height(&self, x: f32, z: f32) -> f32 {
        self.sample(x, z, 0, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn field_interpolates_and_clamps_at_world_boundary() {
        let mut values = vec![0.; 16];
        for (i, v) in [0., 10., 20., 30.].into_iter().enumerate() {
            values[i * 4] = v;
        }
        let field = Field {
            size: 2,
            extent: 2.,
            values,
            ecology: vec![0.; 16],
        };
        assert_eq!(field.height(0., 0.), 15.);
        assert_eq!(field.height(5., 5.), 30.);
        assert_eq!(field.height(-5., -5.), 0.);
        assert_eq!(field.height(0., -1.), 5.);
    }
}
