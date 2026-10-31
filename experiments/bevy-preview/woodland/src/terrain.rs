use crate::world::Field;
use bevy::prelude::*;
use bvl_engine::assets::MeshData;
pub const TILE: f32 = 125.;
pub fn tile(field: &Field, cx: i32, cz: i32, segments: usize) -> MeshData {
    let mut data = MeshData::default();
    let step = TILE / segments as f32;
    for j in 0..=segments {
        for i in 0..=segments {
            let x = cx as f32 * TILE - 2000. + i as f32 * step;
            let z = cz as f32 * TILE - 2000. + j as f32 * step;
            let normal = Vec3::new(
                field.height(x - 1., z) - field.height(x + 1., z),
                2.,
                field.height(x, z - 1.) - field.height(x, z + 1.),
            )
            .normalize();
            data.positions.push([x, field.height(x, z), z]);
            data.normals.push(normal.to_array());
            data.uvs.push([x * 0.25, z * 0.25]);
            data.colours.push([
                field
                    .sample(x, z, 1, false)
                    .max(field.sample(x, z, 0, true) * 0.7),
                field.sample(x, z, 2, false),
                field.sample(x, z, 3, false),
                field.sample(x, z, 1, true),
            ]);
        }
    }
    for j in 0..segments {
        for i in 0..segments {
            let a = (j * (segments + 1) + i) as u32;
            let b = a + 1;
            let c = a + (segments + 1) as u32;
            data.indices.extend([a, c, b, b, c, c + 1]);
        }
    }
    data
}
