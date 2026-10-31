//! Woodland data ownership and geographic sampling; no renderer dependencies.
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::path::Path;
#[derive(serde::Serialize, Deserialize, Clone)]
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
#[derive(serde::Serialize, Deserialize)]
pub struct Field {
    pub size: usize,
    pub extent: f32,
    pub values: Vec<f32>,
    pub ecology: Vec<f32>,
}
impl Field {
    pub const SURFACE_STEP: f32 = 125. / 32.;

    pub fn refined_edges(&self, x: f32, z: f32) -> [bool; 4] {
        let s = Self::SURFACE_STEP;
        [(0., 0.5), (0.5, 1.), (1., 0.5), (0.5, 0.)]
            .map(|(u, v)| self.sample(x + u * s, z + v * s, 2, true) < 26.)
    }

    /// Height of the actual rendered triangles, including river edge refinement.
    /// The source field is bilinear; using it directly lets plants float above
    /// the coarser piecewise-planar render mesh on uneven slopes.
    pub fn surface_height(&self, x: f32, z: f32) -> f32 {
        let s = Self::SURFACE_STEP;
        let half = self.extent * 0.5;
        let cells = (self.extent / s).ceil();
        let gx = ((x + half) / s).clamp(0., cells - 0.0001);
        let gz = ((z + half) / s).clamp(0., cells - 0.0001);
        let x0 = gx.floor() * s - half;
        let z0 = gz.floor() * s - half;
        let u = gx.fract();
        let v = gz.fract();
        let refine = self.refined_edges(x0, z0);
        let height = |a: (f32, f32)| self.height(x0 + a.0 * s, z0 + a.1 * s);
        if !refine.iter().any(|&r| r) {
            return if u + v <= 1. {
                height((0., 0.)) * (1. - u - v) + height((1., 0.)) * u + height((0., 1.)) * v
            } else {
                height((1., 1.)) * (u + v - 1.)
                    + height((1., 0.)) * (1. - v)
                    + height((0., 1.)) * (1. - u)
            };
        }
        let corners = [(0., 0.), (0., 1.), (1., 1.), (1., 0.)];
        let mut ring = [(0_f32, 0_f32); 8];
        let mut count = 0;
        for i in 0..4 {
            ring[count] = corners[i];
            count += 1;
            if refine[i] {
                let next = corners[(i + 1) % 4];
                ring[count] = ((corners[i].0 + next.0) * 0.5, (corners[i].1 + next.1) * 0.5);
                count += 1;
            }
        }
        for i in 0..count {
            let b = ring[i];
            let c = ring[(i + 1) % count];
            let denominator = (b.1 - c.1) * (0.5 - c.0) + (c.0 - b.0) * (0.5 - c.1);
            let a_weight = ((b.1 - c.1) * (u - c.0) + (c.0 - b.0) * (v - c.1)) / denominator;
            let b_weight = ((c.1 - 0.5) * (u - c.0) + (0.5 - c.0) * (v - c.1)) / denominator;
            let c_weight = 1. - a_weight - b_weight;
            if a_weight >= -0.00001 && b_weight >= -0.00001 && c_weight >= -0.00001 {
                return height((0.5, 0.5)) * a_weight + height(b) * b_weight + height(c) * c_weight;
            }
        }
        self.height(x, z)
    }
    pub fn load(path: &Path, manifest: &Manifest) -> Result<Self> {
        fn floats(path: &Path, expected: usize) -> Result<Vec<f32>> {
            let bytes = std::fs::read(path)?;
            ensure!(bytes.len() == expected * 4, "Invalid terrain field length");
            let values: Vec<_> = bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| f32::from_le_bytes(*v))
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
