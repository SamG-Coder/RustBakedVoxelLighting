use crate::{scene::WorldState, vegetation};
use bevy::{camera::visibility::VisibilityRange, light::NotShadowCaster, prelude::*};
use bvl_engine::{
    assets::MeshData,
    controller::PlayerCamera,
    jobs::{CancelToken, JobPool},
};
use std::collections::HashMap;
struct Resident {
    entities: Vec<Entity>,
    meshes: Vec<Handle<Mesh>>,
}
#[derive(Resource)]
pub struct GrassStream {
    pool: JobPool<Vec<MeshData>>,
    pending: HashMap<(i32, i32), (u64, CancelToken)>,
    resident: HashMap<(i32, i32), Resident>,
    serial: u64,
    timer: f32,
}
impl Default for GrassStream {
    fn default() -> Self {
        Self {
            pool: JobPool::new(2, 8),
            pending: HashMap::new(),
            resident: HashMap::new(),
            serial: 0,
            timer: 0.,
        }
    }
}
pub fn update(
    mut commands: Commands,
    time: Res<Time>,
    camera: Single<&Transform, With<PlayerCamera>>,
    state: Res<WorldState>,
    mut stream: ResMut<GrassStream>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let cx = (camera.translation.x / 24.).floor() as i32;
    let cz = (camera.translation.z / 24.).floor() as i32;
    stream.pending.retain(|&(x, z), (_, token)| {
        let keep = (x - cx).abs() <= 4 && (z - cz).abs() <= 4;
        if !keep {
            token.cancel();
        }
        keep
    });
    stream.resident.retain(|&(x, z), tile| {
        let keep = (x - cx).abs() <= 5 && (z - cz).abs() <= 5;
        if !keep {
            for &e in &tile.entities {
                commands.entity(e).despawn();
            }
            for mesh in &tile.meshes {
                meshes.remove(mesh.id());
            }
        }
        keep
    });
    // Publish one completed tile/frame. Transfer work is bounded by tile size.
    if let Some(done) = stream.pool.poll() {
        let key = stream
            .pending
            .iter()
            .find(|(_, v)| v.0 == done.id)
            .map(|(k, _)| *k);
        if let Some(key) = key {
            stream.pending.remove(&key);
            if let Some(groups) = done.value {
                let mut tile = Resident {
                    entities: Vec::new(),
                    meshes: Vec::new(),
                };
                let center = Vec3::new(
                    key.0 as f32 * 24. + 12.,
                    state
                        .field
                        .height(key.0 as f32 * 24. + 12., key.1 as f32 * 24. + 12.),
                    key.1 as f32 * 24. + 12.,
                );
                for (kind, mut data) in groups.into_iter().enumerate() {
                    if data.indices.is_empty() {
                        continue;
                    }
                    for p in &mut data.positions {
                        *p = (Vec3::from(*p) - center).to_array();
                    }
                    let mesh = meshes.add(data.into_mesh());
                    let entity = commands
                        .spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(state.grass_materials[kind].clone()),
                            NotShadowCaster,
                            Transform::from_translation(center),
                            VisibilityRange {
                                start_margin: 0.0..0.0,
                                end_margin: 75.0..105.0,
                                use_aabb: false,
                            },
                        ))
                        .id();
                    tile.entities.push(entity);
                    tile.meshes.push(mesh);
                }
                stream.resident.insert(key, tile);
            }
        }
    }
    stream.timer += time.delta_secs();
    if stream.timer < 0.03 || stream.pending.len() >= 8 {
        return;
    }
    stream.timer = 0.;
    let mut candidate = None;
    let mut priority = f32::MAX;
    let forward = *camera.forward();
    for z in cz - 4..=cz + 4 {
        for x in cx - 4..=cx + 4 {
            let key = (x, z);
            if stream.pending.contains_key(&key) || stream.resident.contains_key(&key) {
                continue;
            }
            let delta = Vec3::new(
                x as f32 * 24. + 12. - camera.translation.x,
                0.,
                z as f32 * 24. + 12. - camera.translation.z,
            );
            let score = delta.length_squared() * (if delta.dot(forward) > 0. { 1. } else { 1.5 });
            if score < priority {
                priority = score;
                candidate = Some(key);
            }
        }
    }
    if let Some((x, z)) = candidate {
        stream.serial += 1;
        let id = stream.serial;
        let token = CancelToken::default();
        let field = state.field.clone();
        if stream.pool.submit(id, token.clone(), move |cancel| {
            vegetation::grass(&field, x, z, cancel)
        }) {
            stream.pending.insert((x, z), (id, token));
        }
    }
}
