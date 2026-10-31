//! Shallow curved foliage cards, prepared once when prototypes are loaded.
use bvl_engine::assets::{MeshData, Vertex};
use glam::{Vec2, Vec3, Vec4};

pub fn bowl_cards(source: MeshData) -> MeshData {
    bowl_cards_detail(source, 2)
}

pub fn bowl_cards_detail(source: MeshData, segments: u32) -> MeshData {
    let segments = segments.clamp(1, 2);
    let stride = segments + 1;
    // Only touch the independent four-vertex cards emitted by the exporter.
    // Unknown layouts retain their original geometry.
    let cards = source.vertices.len() / 4;
    if !source.vertices.len().is_multiple_of(4)
        || source.indices.len() != cards * 6
        || source
            .indices
            .as_chunks::<6>()
            .0
            .iter()
            .enumerate()
            .any(|(i, t)| {
                let a = i as u32 * 4;
                *t != [a, a + 2, a + 1, a + 2, a + 3, a + 1]
            })
    {
        return source;
    }
    let mut result = MeshData {
        vertices: Vec::with_capacity(cards * (stride * stride) as usize),
        indices: Vec::with_capacity(cards * (segments * segments * 6) as usize),
    };
    for card in source.vertices.as_chunks::<4>().0 {
        let p = card.map(|v| Vec3::from_array(v.position));
        let u = p[1] - p[0];
        let v = p[2] - p[0];
        let face = v.cross(u).normalize_or_zero();
        let old_normal = card
            .iter()
            .fold(Vec3::ZERO, |sum, v| sum + Vec3::from_array(v.normal));
        let outward = if face.dot(old_normal) < 0. {
            -face
        } else {
            face
        };
        // Centre rises by 12% of the shorter dimension. Original corners stay
        // fixed, preserving cluster extent, texture layout and attachment.
        let depth = u.length().min(v.length()) * 0.12;
        let first = result.vertices.len() as u32;
        for y in 0..=segments {
            for x in 0..=segments {
                let s = x as f32 / segments as f32;
                let t = y as f32 / segments as f32;
                let a = s * 2. - 1.;
                let b = t * 2. - 1.;
                let position = p[0].lerp(p[1], s).lerp(p[2].lerp(p[3], s), t)
                    + outward * depth * (1. - (a * a + b * b) * 0.5);
                let ds = (p[1] - p[0]).lerp(p[3] - p[2], t) - outward * (depth * 2. * a);
                let dt = (p[2] - p[0]).lerp(p[3] - p[1], s) - outward * (depth * 2. * b);
                let normal = dt.cross(ds).try_normalize().unwrap_or(Vec3::Y);
                let uv = card.map(|v| Vec2::from_array(v.uv));
                let color = card.map(|v| Vec4::from_array(v.color));
                result.vertices.push(Vertex {
                    position: position.to_array(),
                    normal: normal.to_array(),
                    uv: uv[0]
                        .lerp(uv[1], s)
                        .lerp(uv[2].lerp(uv[3], s), t)
                        .to_array(),
                    color: color[0]
                        .lerp(color[1], s)
                        .lerp(color[2].lerp(color[3], s), t)
                        .to_array(),
                    uv1: [0.; 2],
                });
            }
        }
        for y in 0..segments {
            for x in 0..segments {
                let a = first + y * stride + x;
                result
                    .indices
                    .extend([a, a + stride, a + 1, a + stride, a + stride + 1, a + 1]);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curved_card_keeps_corners_uvs_and_has_varying_unit_normals() {
        let source = MeshData {
            vertices: vec![
                Vertex {
                    position: [0., 0., 0.],
                    normal: [0., 0., -1.],
                    uv: [0., 0.],
                    ..Default::default()
                },
                Vertex {
                    position: [1., 0., 0.],
                    normal: [0., 0., -1.],
                    uv: [1., 0.],
                    ..Default::default()
                },
                Vertex {
                    position: [0., 1., 0.],
                    normal: [0., 0., -1.],
                    uv: [0., 1.],
                    ..Default::default()
                },
                Vertex {
                    position: [1., 1., 0.],
                    normal: [0., 0., -1.],
                    uv: [1., 1.],
                    ..Default::default()
                },
            ],
            indices: vec![0, 2, 1, 2, 3, 1],
        };
        let distant = bowl_cards_detail(source.clone(), 1);
        assert_eq!(distant.vertices.len(), source.vertices.len());
        assert_eq!(distant.indices, source.indices);
        for (a, b) in distant.vertices.iter().zip(&source.vertices) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.uv, b.uv);
        }
        let out = bowl_cards(source.clone());
        for (i, j) in [0, 2, 6, 8].into_iter().enumerate() {
            assert_eq!(out.vertices[j].position, source.vertices[i].position);
            assert_eq!(out.vertices[j].uv, source.vertices[i].uv);
        }
        assert!((out.vertices[4].position[2] + 0.12).abs() < 1e-6);
        assert_ne!(out.vertices[0].normal, out.vertices[8].normal);
        assert!(
            out.vertices
                .iter()
                .all(|v| (Vec3::from_array(v.normal).length() - 1.).abs() < 1e-5)
        );
        assert!(
            out.indices
                .iter()
                .all(|&i| (i as usize) < out.vertices.len())
        );
    }
}
