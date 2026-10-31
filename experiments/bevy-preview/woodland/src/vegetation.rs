//! Woodland placement policy. Streaming transport and mesh decoding belong to the engine.
use crate::{
    materials::FarMaterial,
    scene::WorldState,
    world::{Field, Impostor, Plant},
};
use bevy::{camera::visibility::VisibilityRange, prelude::*};
use bvl_engine::{assets::MeshData, controller::PlayerCamera};
use std::collections::HashMap;
pub struct Prototype {
    pub wood: Vec<Handle<Mesh>>,
    pub leaves: Vec<Handle<Mesh>>,
    pub bark: Handle<StandardMaterial>,
    pub leaf: Handle<StandardMaterial>,
}
#[derive(Resource, Default)]
pub struct NearPlants {
    pub instances: HashMap<(usize, usize, usize), [Entity; 2]>,
    pub timer: f32,
}
pub fn plant_transform(p: &Plant) -> Transform {
    Transform::from_xyz(p.x, p.y, p.z)
        .with_rotation(Quat::from_rotation_y(p.rotation))
        .with_scale(Vec3::splat(p.scale))
}
pub fn far_mesh(plants: &[Plant], captures: &[Impostor], center: Vec3) -> MeshData {
    let mut result = MeshData::default();
    for p in plants {
        let capture = &captures[p.variant];
        let transform = plant_transform(p).to_matrix();
        for side in 0..2 {
            let half = capture.width * 0.5;
            let y = capture.bottom;
            let top = y + capture.height;
            let mut points = [
                Vec3::new(-half, y, 0.),
                Vec3::new(half, y, 0.),
                Vec3::new(half, top, 0.),
                Vec3::new(-half, top, 0.),
            ];
            if side == 1 {
                for v in &mut points {
                    *v = Vec3::new(0., v.y, v.x);
                }
            }
            for v in &mut points {
                *v = transform.transform_point3(*v) - center;
            }
            let u = (p.variant % 4) as f32 * 0.25 + side as f32 * 0.125;
            let v = (p.variant / 4) as f32 * 0.25;
            result.quad(
                points,
                [[u, v + 0.25], [u + 0.125, v + 0.25], [u + 0.125, v], [u, v]],
                Vec3::Y,
                [p.rotation.sin(), p.rotation.cos(), 1., p.y],
            );
            result.uvs1.extend([[p.x, p.z]; 4]);
        }
    }
    result
}
pub fn stream(
    mut commands: Commands,
    time: Res<Time>,
    state: Res<WorldState>,
    camera: Single<&Transform, With<PlayerCamera>>,
    mut near: ResMut<NearPlants>,
    mut far: ResMut<Assets<FarMaterial>>,
) {
    near.timer += time.delta_secs();
    if near.timer < 0.05 {
        return;
    }
    near.timer = 0.;
    let pos = camera.translation;
    // Residency includes a margin outside visible ranges. We do not submit tens
    // of thousands of invisible full-detail entities merely because a tile is near.
    near.instances.retain(|&(tile, plant, lod), entities| {
        let p = &state.tiles[tile][plant];
        let d = pos.distance_squared(Vec3::new(p.x, p.y, p.z));
        let limit = if lod == 0 { 125.0_f32 } else { 305.0_f32 };
        let keep = d < limit * limit;
        if !keep {
            for &e in entities.iter() {
                commands.entity(e).despawn();
            }
        }
        keep
    });
    let cx = ((pos.x + 2000.) / 125.) as i32;
    let cz = ((pos.z + 2000.) / 125.) as i32;
    let mut created = 0;
    for z in (cz - 3).max(0)..=(cz + 3).min(31) {
        for x in (cx - 3).max(0)..=(cx + 3).min(31) {
            let tile = (z * 32 + x) as usize;
            let mut complete = true;
            for (plant, p) in state.tiles[tile].iter().enumerate() {
                let d = pos.distance_squared(Vec3::new(p.x, p.y, p.z));
                for lod in (0..2).rev() {
                    let limit = if lod == 0 { 110.0_f32 } else { 290.0_f32 };
                    if d > limit * limit {
                        continue;
                    }
                    let key = (tile, plant, lod);
                    if near.instances.contains_key(&key) {
                        continue;
                    }
                    if created >= 192 {
                        complete = false;
                        continue;
                    }
                    let proto = &state.prototypes[p.variant];
                    let transform = plant_transform(p);
                    let range = VisibilityRange {
                        start_margin: if lod == 0 { 0.0..0.0 } else { 70.0..90.0 },
                        end_margin: if lod == 0 { 70.0..90.0 } else { 200.0..260.0 },
                        use_aabb: false,
                    };
                    let entities = std::array::from_fn(|part| {
                        let (mesh, material) = if part == 0 {
                            (&proto.wood[lod], &proto.bark)
                        } else {
                            (&proto.leaves[lod], &proto.leaf)
                        };
                        commands
                            .spawn((
                                Mesh3d(mesh.clone()),
                                MeshMaterial3d(material.clone()),
                                transform,
                                range.clone(),
                            ))
                            .id()
                    });
                    near.instances.insert(key, entities);
                    created += 1;
                }
            }
            if let Some(mut m) = far.get_mut(&state.far_materials[tile]) {
                let ready = if complete { 1. } else { 0. };
                if m.extension.settings.z != ready {
                    m.extension.settings.z = ready;
                }
            }
        }
    }
}

pub fn constrain_player(
    state: Res<WorldState>,
    control: Res<bvl_engine::controller::Controller>,
    mut camera: Single<&mut Transform, With<PlayerCamera>>,
) {
    camera.translation.x = camera.translation.x.clamp(-1990., 1990.);
    camera.translation.z = camera.translation.z.clamp(-1990., 1990.);
    if control.flying {
        return;
    }
    let cx = ((camera.translation.x + 2000.) / 125.) as i32;
    let cz = ((camera.translation.z + 2000.) / 125.) as i32;
    for z in (cz - 1).max(0)..=(cz + 1).min(31) {
        for x in (cx - 1).max(0)..=(cx + 1).min(31) {
            for p in &state.tiles[(z * 32 + x) as usize] {
                if p.variant >= 12 {
                    continue;
                }
                let delta = Vec2::new(camera.translation.x - p.x, camera.translation.z - p.z);
                let radius = [0.12, 0.4, 0.7][p.variant / 4] * p.scale + 0.28;
                let distance = delta.length();
                if distance < radius && distance > 0.001 {
                    let push = delta / distance * (radius - distance);
                    camera.translation.x += push.x;
                    camera.translation.z += push.y;
                }
            }
        }
    }
    camera.translation.y = state
        .field
        .height(camera.translation.x, camera.translation.z)
        + 1.75;
}
pub fn grass(
    field: &Field,
    x: i32,
    z: i32,
    cancel: &bvl_engine::jobs::CancelToken,
) -> Option<Vec<MeshData>> {
    let mut groups = vec![MeshData::default(); 6];
    let mut seed = ((x as u32).wrapping_mul(73856093)) ^ ((z as u32).wrapping_mul(19349663));
    let mut rand = || {
        seed = seed.wrapping_add(0x6d2b79f5);
        let mut t = (seed ^ (seed >> 15)).wrapping_mul(1 | seed);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t));
        (t ^ (t >> 14)) as f32 / u32::MAX as f32
    };
    for i in 0..1800 {
        if i % 64 == 0 && cancel.cancelled() {
            return None;
        }
        let wx = x as f32 * 24. + rand() * 24.;
        let wz = z as f32 * 24. + rand() * 24.;
        let wear = field
            .sample(wx, wz, 1, false)
            .max(field.sample(wx, wz, 0, true));
        let canopy = field.sample(wx, wz, 1, true);
        if wear > 0.25
            || field.sample(wx, wz, 2, true) < 3.
            || field.sample(wx, wz, 3, false) > 0.6
            || rand() < canopy * 0.7
        {
            continue;
        }
        let species = if canopy > 0.55 {
            5
        } else {
            let r = rand();
            if r < 0.4 {
                0
            } else if r < 0.78 {
                1
            } else if r < 0.9 {
                2
            } else if r < 0.96 {
                3
            } else {
                4
            }
        };
        let h = match species {
            0 => 0.4,
            1 => 0.85,
            2 => 1.2,
            3 | 4 => 0.65,
            _ => 0.65,
        } * (0.65 + rand() * 0.7);
        let w = h * (0.8 + rand() * 0.4);
        let y = field.height(wx, wz) - 0.02;
        let tint = 0.8 + rand() * 0.2;
        let patch = 0.93 + 0.07 * (wx * 0.025 + (wz * 0.019).sin()).sin();
        let colour = [tint * patch, tint, tint * 0.92, 1.];
        let yaw = rand() * std::f32::consts::TAU;
        for side in 0..2 {
            let angle = yaw + side as f32 * std::f32::consts::FRAC_PI_2;
            let right = Vec3::new(angle.cos(), 0., angle.sin()) * w * 0.5;
            let root = Vec3::new(wx, y, wz);
            let tip = root + Vec3::Y * h;
            groups[species].quad(
                [root - right, root + right, tip + right, tip - right],
                [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
                Vec3::new(angle.sin() * 0.18, 0.97, angle.cos() * 0.18).normalize(),
                colour,
            );
        }
    }
    Some(groups)
}
