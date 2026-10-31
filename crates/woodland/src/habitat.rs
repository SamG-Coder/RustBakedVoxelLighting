//! Deterministic geographic colour fields, shared with the browser source model.
pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn hash(x: i32, z: i32, seed: u32) -> f32 {
    let mut v = (x as u32).wrapping_mul(374761393) ^ (z as u32).wrapping_mul(668265263) ^ seed;
    v = (v ^ (v >> 13)).wrapping_mul(1274126177);
    (v ^ (v >> 16)) as f32 / 4294967296.
}
pub fn noise(x: f32, z: f32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let u = smooth(0., 1., x - x.floor());
    let v = smooth(0., 1., z - z.floor());
    let a = hash(ix, iz, seed);
    let b = hash(ix + 1, iz, seed);
    let c = hash(ix, iz + 1, seed);
    let d = hash(ix + 1, iz + 1, seed);
    (a + (b - a) * u) * (1. - v) + (c + (d - c) * u) * v
}

pub struct Habitat {
    pub weights: [f32; crate::plant_catalog::TYPES],
    pub woodland: f32,
    pub density: f32,
    pub height: f32,
    pub width: f32,
    pub flow: f32,
    pub colour: [f32; 4],
}
// Riverbank noise uses addition, while habitat patch noise uses XOR.
fn bank_noise(x: f32, z: f32) -> f32 {
    let hash = |x: i32, z: i32| {
        let mut n = (x as u32)
            .wrapping_mul(374761393)
            .wrapping_add((z as u32).wrapping_mul(668265263));
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        (n ^ (n >> 16)) as f32 / 4294967295.
    };
    let a = x.floor() as i32;
    let b = z.floor() as i32;
    let u = smooth(0., 1., x - x.floor());
    let v = smooth(0., 1., z - z.floor());
    (hash(a, b) * (1. - u) + hash(a + 1, b) * u) * (1. - v)
        + (hash(a, b + 1) * (1. - u) + hash(a + 1, b + 1) * u) * v
}
fn source_sample(x: f32, z: f32, moisture: f32, canopy: f32, forest: f32, river: f32) -> Habitat {
    let stand = smooth(0.28, 0.74, forest);
    let gap = smooth(0.72, 0.89, noise(x * 0.032 + 71., z * 0.032 - 19., 823));
    let core = stand * (1. - gap);
    let edge = 4. * core * (1. - core);
    let warp = (noise(x * 0.009, z * 0.009, 503) - 0.5) * 28.;
    let broad = noise((x + warp) * 0.027, (z - warp * 0.6) * 0.011, 73);
    let fine = noise(x * 0.19, z * 0.19, 127);
    let colony = noise(x * 0.061 + warp * 0.02, z * 0.061, 439);
    let moisture = moisture.clamp(0., 1.);
    let shade = canopy.min(0.98);
    let woodland = smooth(0.18, 0.78, shade) * (0.65 + stand * 0.35);
    let dry = smooth(0.34, 0.8, 1. - moisture) * (0.45 + broad * 0.55);
    let flower = smooth(0.46, 0.76, colony) * (1. - woodland) * (1. - dry * 0.65);
    let dry_mix = smooth(0.18, 0.58, dry) * (1. - flower) * (1. - woodland);
    let mut mixes = [
        (1. - woodland) * (1. - flower) - dry_mix,
        dry_mix,
        flower * (1. - woodland),
        woodland,
    ];
    let sum: f32 = mixes.iter().sum();
    for m in &mut mixes {
        *m /= sum;
    }
    let mut weights = std::array::from_fn(|t| {
        crate::plant_catalog::MIXES
            .iter()
            .zip(mixes)
            .map(|(mix, w)| mix[t] * w)
            .sum::<f32>()
    });
    let fern = weights[7];
    weights[7] *= smooth(0.22, 0.7, moisture) * (0.3 + colony * 0.7);
    weights[3] += fern - weights[7];
    let bracken = weights[8];
    weights[8] *= (0.25 + edge * 0.75) * smooth(0.15, 0.5, moisture);
    weights[9] += bracken - weights[8];
    let edge_bracken = weights[3] * edge * (0.2 + colony * 0.5) * smooth(0.2, 0.6, moisture);
    weights[3] -= edge_bracken;
    weights[8] += edge_bracken;
    let short = smooth(0.6, 0.83, noise(x * 0.043, z * 0.043, 719));
    let tint = 0.82 + fine * 0.14;
    let region = noise(x * 0.005 + 17., z * 0.007 - 8., 1009);
    let gold = smooth(0.32, 0.76, region) * (1. - moisture * 0.65) * (1. - woodland * 0.65);
    let lush = moisture * (1. - gold) * (0.4 + region * 0.6);
    let shore = river - 8.;
    let bank = if shore >= 13. {
        0.
    } else if shore <= 0. {
        1.
    } else {
        let width = 2.4 + bank_noise(x * 0.075 + 313., z * 0.075 - 171.) * 7.6;
        let broken = bank_noise(x * 0.31 - 63., z * 0.31 + 819.);
        (1. - smooth(0.25, width, shore)) * (1. - smooth(0., 1., shore) * (1. - broken) * 0.32)
    };
    let established = smooth(0.06, 0.88, 1. - bank);
    Habitat {
        weights,
        woodland,
        density: (0.52 + fine * 0.35 + moisture * 0.16).min(1.)
            * (1. - woodland * 0.91)
            * (1. - bank * 0.97),
        height: (0.78 + broad * 0.63)
            * (1. - short * 0.34)
            * (1. - woodland * 0.46)
            * (0.22 + established * 0.78),
        width: 0.42 + established * 0.58,
        flow: 0.8 + (noise(x * 0.012, z * 0.012, 89) - 0.5) * 1.4 + (broad - 0.5) * 0.6,
        colour: [
            tint * (0.7 + dry * 0.48 + short * 0.11 + gold * 0.38 - lush * 0.1),
            tint * (0.96 - dry * 0.1 + gold * 0.06 + lush * 0.07) * (0.76 + broad * 0.28),
            tint * (0.6 - dry * 0.22 + moisture * 0.16 - gold * 0.18),
            1.,
        ],
    }
}
/// Native appearance modulation is continuous in world space and applied to BOTH
/// geometry and the baked far-ground palette. Base browser ecology stays reproducible.
pub fn sample(x: f32, z: f32, moisture: f32, canopy: f32, forest: f32, river: f32) -> Habitat {
    let mut h = source_sample(x, z, moisture, canopy, forest, river);
    let patch = noise(
        x * 0.047 + noise(x * 0.011, z * 0.013, 881) * 5.,
        z * 0.031,
        887,
    );
    let vigour = smooth(0.22, 0.78, patch);
    let green = moisture.clamp(0., 1.) * (0.35 + vigour * 0.65);
    h.colour[0] *= 0.90 - green * 0.08;
    h.colour[1] *= 0.91 + green * 0.04;
    h.colour[2] *= 1.02 + green * 0.10;
    h
}
pub fn choose(weights: &[f32; crate::plant_catalog::TYPES], mut u: f32) -> usize {
    for (i, w) in weights.iter().enumerate() {
        u -= w;
        if u <= 0. {
            return i;
        }
    }
    weights.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_browser_habitat_reference() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/grass-habitat.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let a: Vec<f32> = case["input"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            let h = source_sample(a[0], a[1], a[2], a[3], a[4], a[5]);
            let e = &case["expected"];
            for (name, actual) in [
                ("density", h.density),
                ("height", h.height),
                ("width", h.width),
                ("woodland", h.woodland),
                ("flow", h.flow),
            ] {
                assert!(
                    (actual - e[name].as_f64().unwrap() as f32).abs() < 0.002,
                    "{name}: {actual} != {} at {a:?}",
                    e[name]
                );
            }
            for (key, values) in [("weights", &h.weights[..]), ("colour", &h.colour[..3])] {
                for (actual, expected) in values.iter().zip(e[key].as_array().unwrap()) {
                    assert!(
                        (actual - expected.as_f64().unwrap() as f32).abs() < 0.002,
                        "{key} differs at {a:?}"
                    );
                }
            }
        }
    }
}
