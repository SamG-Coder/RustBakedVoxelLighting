//! Bounded grass pages. CPU work runs off-thread; at most one page is uploaded per frame.
use crate::{
    habitat::{self, smooth},
    plant_catalog::{COUNT, SIZES},
    world::Field,
};
use bvl_engine::{
    assets::*,
    jobs::{CancelToken, JobPool},
};
use glam::{Mat4, Vec3, Vec3Swizzles};
use std::{collections::HashMap, sync::Arc};
pub const VERTICES: usize = (COUNT + COUNT / 4) * 16;
pub const FAR_VERTICES: usize = 190 * 8;
pub const FAR_INDICES: usize = 190 * 12;
const NEAR_SLOTS: usize = 128;
const FAR_SLOTS: usize = 768;
pub const INDICES: usize = (COUNT + COUNT / 4) * 36;
const PAGE: f32 = 24.;
pub struct Page {
    pub key: (i32, i32, bool),
    pub mesh: MeshData,
    pub min_y: f32,
    pub max_y: f32,
    pub parts: [(u32, u32); 2],
}
struct Resident {
    key: (i32, i32, bool),
    parts: [(u32, u32); 2],
    min_y: f32,
    max_y: f32,
    born: f32,
}
pub struct Grass {
    pool: JobPool<Page>,
    pending: HashMap<u64, ((i32, i32, bool), CancelToken)>,
    slots: Vec<Option<Resident>>,
    next: u64,
    last_cell: Option<(i32, i32)>,
    candidates: Vec<((i32, i32, bool), f32)>,
    pub vertex_start: usize,
    pub index_start: usize,
}
impl Grass {
    pub fn new(vertex_start: usize, index_start: usize) -> Self {
        const {
            assert!(
                NEAR_SLOTS * VERTICES + FAR_SLOTS * FAR_VERTICES
                    <= bvl_engine::gpu::STREAM_VERTICES
            );
        };
        const {
            assert!(
                NEAR_SLOTS * INDICES + FAR_SLOTS * FAR_INDICES <= bvl_engine::gpu::STREAM_INDICES
            );
        };
        Self {
            pool: JobPool::new(2, 8),
            pending: HashMap::with_capacity(16),
            slots: (0..NEAR_SLOTS + FAR_SLOTS).map(|_| None).collect(),
            next: 0,
            last_cell: None,
            candidates: Vec::with_capacity(800),
            vertex_start,
            index_start,
        }
    }
    pub fn update(
        &mut self,
        field: &Arc<Field>,
        camera: Vec3,
        forward: Vec3,
        time: f32,
    ) -> Option<(MeshRange, Page)> {
        let cx = (camera.x / PAGE).floor() as i32;
        let cz = (camera.z / PAGE).floor() as i32;
        let wanted = |key: (i32, i32, bool)| {
            let r = if key.2 { 13 } else { 5 };
            (key.0 - cx).abs() <= r && (key.1 - cz).abs() <= r
        };
        for (key, token) in self.pending.values() {
            if !wanted(*key) {
                token.cancel();
            }
        }
        for slot in &mut self.slots {
            if slot.as_ref().is_some_and(|r| !wanted(r.key)) {
                *slot = None;
            }
        }
        let mut upload = None;
        while let Some(done) = self.pool.poll() {
            self.pending.remove(&done.id);
            if done.value.is_none() {
                self.last_cell = None;
            }
            if let Some(page) = done.value
                && wanted(page.key)
                && let Some(slot) = self
                    .slots
                    .iter()
                    .enumerate()
                    .position(|(i, s)| s.is_none() && (i >= NEAR_SLOTS) == page.key.2)
            {
                let range = MeshRange {
                    first_index: (self.index_start + index_offset(slot)) as u32,
                    index_count: page.mesh.indices.len() as u32,
                    vertex_offset: (self.vertex_start + vertex_offset(slot)) as i32,
                };
                self.slots[slot] = Some(Resident {
                    key: page.key,
                    parts: page.parts,
                    min_y: page.min_y,
                    max_y: page.max_y,
                    born: time,
                });
                upload = Some((range, page));
                break;
            }
        }
        if self.last_cell != Some((cx, cz)) {
            self.last_cell = Some((cx, cz));
            self.candidates.clear();
            for far in [false, true] {
                let radius = if far { 12 } else { 4 };
                for z in cz - radius..=cz + radius {
                    for x in cx - radius..=cx + radius {
                        let key = (x, z, far);
                        if self.slots.iter().flatten().any(|r| r.key == key)
                            || self.pending.values().any(|(k, _)| *k == key)
                        {
                            continue;
                        }
                        let delta = Vec3::new(
                            (x as f32 + 0.5) * PAGE - camera.x,
                            0.,
                            (z as f32 + 0.5) * PAGE - camera.z,
                        );
                        let priority = delta.length_squared()
                            * (if delta.dot(forward) > 0. { 0.65 } else { 1.4 });
                        if !far || delta.length_squared() < 300. * 300. {
                            self.candidates
                                .push((key, priority + if far { 1600. } else { 0. }));
                        }
                    }
                }
            }
            self.candidates.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        }
        while self.pending.len() < 6 {
            let Some((key, priority)) = self.candidates.pop() else {
                break;
            };
            let token = CancelToken::default();
            let field = field.clone();
            let id = self.next;
            self.next += 1;
            if self.pool.submit(id, token.clone(), move |cancel| {
                if key.2 {
                    build_far(&field, key, cancel)
                } else {
                    build(&field, key, cancel)
                }
            }) {
                self.pending.insert(id, (key, token));
            } else {
                self.candidates.push((key, priority));
                break;
            }
        }
        upload
    }
    pub fn append(
        &self,
        camera: Vec3,
        time: f32,
        vp: Mat4,
        material: u32,
        instances: &mut Vec<Instance>,
        draws: &mut Vec<Draw>,
    ) {
        let planes = crate::view::frustum(vp);
        for (slot, r) in self.slots.iter().enumerate() {
            let Some(r) = r else { continue };
            // Conservative vertical bounds are supplied by the generated page.
            let center = Vec3::new(
                (r.key.0 as f32 + 0.5) * PAGE,
                r.min_y + (r.max_y - r.min_y) * 0.5,
                (r.key.1 as f32 + 0.5) * PAGE,
            );
            let radius = (22_f32.powi(2) + ((r.max_y - r.min_y) * 0.5).powi(2)).sqrt();
            if !crate::view::intersects(&planes, center, radius) {
                continue;
            }
            let distance = (center.xz() - camera.xz()).length();
            let modes: &[usize] = if r.key.2 { &[2] } else { &[0, 1] };
            for &mode in modes {
                if (mode == 0 && distance > 87.)
                    || (mode == 1 && (distance < 23. || distance > 118.))
                    || (mode == 2 && (distance < 58. || distance > 302.))
                {
                    continue;
                }
                let part = if mode == 1 { r.parts[1] } else { r.parts[0] };
                if part.1 == 0 {
                    continue;
                }
                let first = instances.len() as u32;
                instances.push(Instance {
                    position_scale: [r.key.0 as f32 * PAGE, 0., r.key.1 as f32 * PAGE, 1.],
                    fade: [((time - r.born) / 0.6).clamp(0., 1.), 0., mode as f32, 0.],
                    ..Default::default()
                });
                draws.push(Draw {
                    mesh: MeshRange {
                        first_index: (self.index_start + index_offset(slot)) as u32 + part.0,
                        index_count: part.1,
                        vertex_offset: (self.vertex_start + vertex_offset(slot)) as i32,
                    },
                    material,
                    first_instance: first,
                    instances: 1,
                });
            }
        }
    }
    // Casters include off-screen nearby pages; turning the camera cannot remove their shadows.
    pub fn append_shadows(
        &self,
        camera: Vec3,
        time: f32,
        material: u32,
        instances: &mut Vec<Instance>,
        draws: &mut Vec<Draw>,
    ) {
        for (slot, resident) in self.slots[..NEAR_SLOTS].iter().enumerate() {
            let Some(r) = resident else { continue };
            let center =
                glam::Vec2::new((r.key.0 as f32 + 0.5) * PAGE, (r.key.1 as f32 + 0.5) * PAGE);
            if center.distance(camera.xz()) > 78. || r.parts[0].1 == 0 {
                continue;
            }
            let first = instances.len() as u32;
            instances.push(Instance {
                position_scale: [r.key.0 as f32 * PAGE, 0., r.key.1 as f32 * PAGE, 1.],
                fade: [((time - r.born) / 0.6).clamp(0., 1.), 0., 0., 0.],
                ..Default::default()
            });
            draws.push(Draw {
                mesh: MeshRange {
                    first_index: (self.index_start + index_offset(slot)) as u32 + r.parts[0].0,
                    index_count: r.parts[0].1,
                    vertex_offset: (self.vertex_start + vertex_offset(slot)) as i32,
                },
                material,
                first_instance: first,
                instances: 1,
            });
        }
    }
    pub fn count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}
fn vertex_offset(slot: usize) -> usize {
    if slot < NEAR_SLOTS {
        slot * VERTICES
    } else {
        NEAR_SLOTS * VERTICES + (slot - NEAR_SLOTS) * FAR_VERTICES
    }
}
fn index_offset(slot: usize) -> usize {
    if slot < NEAR_SLOTS {
        slot * INDICES
    } else {
        NEAR_SLOTS * INDICES + (slot - NEAR_SLOTS) * FAR_INDICES
    }
}
fn finish_page(key: (i32, i32, bool), mesh: MeshData, parts: [(u32, u32); 2]) -> Page {
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for v in &mesh.vertices {
        min_y = min_y.min(v.position[1]);
        max_y = max_y.max(v.position[1]);
    }
    if mesh.vertices.is_empty() {
        min_y = 0.;
        max_y = 0.;
    }
    Page {
        key,
        mesh,
        parts,
        min_y,
        max_y,
    }
}
fn build_far(field: &Field, key: (i32, i32, bool), cancel: &CancelToken) -> Option<Page> {
    let mut seed =
        (key.0 as u32).wrapping_mul(73856093) ^ (key.1 as u32).wrapping_mul(19349663) ^ 91271;
    let mut mesh = MeshData::default();
    for i in 0..190 {
        if i % 32 == 0 && cancel.cancelled() {
            return None;
        }
        let lx = random(&mut seed) * PAGE;
        let lz = random(&mut seed) * PAGE;
        let x = key.0 as f32 * PAGE + lx;
        let z = key.1 as f32 * PAGE + lz;
        let river = field.sample(x, z, 2, true);
        if field.sample(x, z, 1, false) > 0.12
            || field.sample(x, z, 3, false) > 0.64
            || river < 10.
            || field.sample(x, z, 0, true) > 0.4
        {
            continue;
        }
        let canopy = field.sample(x, z, 1, true);
        let h = habitat::sample(
            x,
            z,
            field.sample(x, z, 2, false),
            canopy,
            field.sample(x, z, 3, true),
            river,
        );
        if random(&mut seed) > h.density {
            continue;
        }
        // Pick the closest source habitat mixture, rather than flattening every biome to meadow.
        let mut best = 0;
        let mut score = f32::MAX;
        for (j, mix) in crate::plant_catalog::MIXES.iter().enumerate() {
            let error = h
                .weights
                .iter()
                .zip(mix)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>();
            if error < score {
                score = error;
                best = j;
            }
        }
        let width = 4.2 * (0.62 + random(&mut seed) * 0.62) * h.width;
        let height = 2.4 * h.height * (0.76 + random(&mut seed) * 0.48);
        let a = h.flow + random(&mut seed) * std::f32::consts::PI;
        let y = field.surface_height(x, z) - 0.035;
        let flip = random(&mut seed) > 0.5;
        for side in 0..2 {
            let angle = a + side as f32 * std::f32::consts::FRAC_PI_2;
            let offset = Vec3::new(angle.cos() * width * 0.5, 0., angle.sin() * width * 0.5);
            let base = Vec3::new(lx, y, lz);
            let mut left = base - offset;
            let mut right = base + offset;
            left.y = field.surface_height(x - offset.x, z - offset.z) - 0.035;
            right.y = field.surface_height(x + offset.x, z + offset.z) - 0.035;
            mesh.quad(
                [
                    left,
                    right,
                    right + Vec3::Y * height,
                    left + Vec3::Y * height,
                ],
                if flip {
                    [[1., 1.], [0., 1.], [0., 0.], [1., 0.]]
                } else {
                    [[0., 1.], [1., 1.], [1., 0.], [0., 0.]]
                },
                Vec3::new(lx, lz, (12 + best) as f32),
                [
                    h.colour[0],
                    h.colour[1],
                    h.colour[2],
                    if flip { -height } else { height },
                ],
                [angle, width],
            );
        }
    }
    let count = mesh.indices.len() as u32;
    Some(finish_page(key, mesh, [(0, count), (0, 0)]))
}
// Same two-stream Mulberry32 sequence used by the source population.
fn random(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_add(0x6d2b79f5);
    let mut t = (*seed ^ (*seed >> 15)).wrapping_mul(1 | *seed);
    t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t));
    (t ^ (t >> 14)) as f32 / 4294967296.
}
fn build(field: &Field, key: (i32, i32, bool), cancel: &CancelToken) -> Option<Page> {
    let base_seed =
        (key.0 as u32).wrapping_mul(73856093) ^ (key.1 as u32).wrapping_mul(19349663) ^ 91271;
    let mut mesh = MeshData::default();
    let mut medium = Vec::new();
    let mut detail_indices = Vec::new();
    for pass in 0..2 {
        let mut seed = if pass == 0 {
            base_seed
        } else {
            base_seed ^ 0x6a09e667
        };
        let count = if pass == 0 { COUNT } else { COUNT / 4 };
        for n in 0..count {
            if n % 64 == 0 && cancel.cancelled() {
                return None;
            }
            let lx = random(&mut seed) * PAGE;
            let lz = random(&mut seed) * PAGE;
            let x = key.0 as f32 * PAGE + lx;
            let z = key.1 as f32 * PAGE + lz;
            let wear = field.sample(x, z, 1, false);
            let slope = field.sample(x, z, 3, false);
            let root = field.sample(x, z, 0, true);
            let river = field.sample(x, z, 2, true);
            if wear >= 0.1 + random(&mut seed) * 0.22
                || slope >= 0.64
                || river <= 8.
                || random(&mut seed) <= smooth(0.12, 0.78, root) * 0.94
            {
                continue;
            }
            let canopy = field.sample(x, z, 1, true);
            let moisture = field.sample(x, z, 2, false);
            let h = habitat::sample(x, z, moisture, canopy, field.sample(x, z, 3, true), river);
            if random(&mut seed) > h.density {
                continue;
            }
            let group = habitat::choose(
                &h.weights,
                if pass == 0 {
                    random(&mut seed) * 0.8
                } else {
                    0.8 + random(&mut seed) * 0.2
                },
            );
            let form = SIZES[group];
            let height = (form[0] + random(&mut seed) * (form[1] - form[0])) * h.height;
            let width = (form[2] + random(&mut seed) * (form[3] - form[2])) * h.width;
            let variation = 0.94 + random(&mut seed) * 0.12;
            let colour = [
                h.colour[0] * variation,
                h.colour[1] * variation,
                h.colour[2] * variation,
                1.,
            ];
            let y = field.surface_height(x, z) - 0.035;
            let angle = h.flow + (random(&mut seed) - 0.5) * 2.5;
            let first_index = mesh.indices.len();
            for side in 0..2 {
                let a = angle + side as f32 * std::f32::consts::FRAC_PI_2;
                let offset = Vec3::new(a.cos() * width * 0.5, 0., -a.sin() * width * 0.5);
                let forward = Vec3::new(a.sin(), 0., a.cos()) * width;
                let base = Vec3::new(lx, y, lz);
                let edge_heights = [-1., 1.].map(|sign| {
                    field.surface_height(x + offset.x * sign, z + offset.z * sign) - 0.035
                });
                let first = mesh.vertices.len() as u32;
                let mirrored = (height * 127.1)
                    .sin()
                    .mul_add(43_758.547, 0.)
                    .rem_euclid(1.)
                    < 0.5;
                // Same three-segment bowed carrier as browser PlaneGeometry(1,1,1,3).
                for row in 0..=3 {
                    let v = row as f32 / 3.;
                    let center = base + Vec3::Y * height * v + forward * (v * v * 0.32);
                    for (edge, edge_height) in edge_heights.iter().enumerate() {
                        let mut position = center + offset * (edge as f32 * 2. - 1.);
                        position.y += (edge_height - y) * (1. - v).powi(2);
                        mesh.vertices.push(Vertex {
                            position: position.to_array(),
                            normal: [lx, lz, group as f32],
                            uv: [
                                if mirrored {
                                    1. - edge as f32
                                } else {
                                    edge as f32
                                },
                                1. - v,
                            ],
                            color: [
                                colour[0],
                                colour[1],
                                colour[2],
                                if mirrored { -height } else { height },
                            ],
                            uv1: [a, width],
                        });
                    }
                }
                for row in 0..3 {
                    let b = first + row * 2;
                    mesh.indices.extend([b, b + 1, b + 3, b, b + 3, b + 2]);
                }
            }
            if n % 3 == 0 {
                medium.extend_from_slice(&mesh.indices[first_index..]);
            } else {
                detail_indices.extend_from_slice(&mesh.indices[first_index..]);
            }
        }
    }
    let middle_count = medium.len() as u32;
    medium.extend(detail_indices);
    mesh.indices = medium;
    let parts = [(0, mesh.indices.len() as u32), (0, middle_count)];
    debug_assert!(mesh.vertices.len() <= VERTICES && mesh.indices.len() <= INDICES);
    Some(finish_page(key, mesh, parts))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(river: f32) -> Field {
        Field {
            size: 2,
            extent: 100.,
            values: [0., 0., 0.6, 0.].repeat(4),
            ecology: [0., 0., river, 0.].repeat(4),
        }
    }
    #[test]
    fn grass_card_bases_follow_uneven_rendered_ground() {
        let mut field = field(100.);
        for (i, h) in [-8., 12., 24., -4.].into_iter().enumerate() {
            field.values[i * 4] = h;
        }
        for far in [false, true] {
            let page = if far {
                build_far(&field, (0, 0, true), &CancelToken::default())
            } else {
                build(&field, (0, 0, false), &CancelToken::default())
            }
            .unwrap();
            assert!(!page.mesh.vertices.is_empty());
            for vertex in page.mesh.vertices.iter().filter(|v| v.uv[1] == 1.) {
                let [x, y, z] = vertex.position;
                assert!((y + 0.035 - field.surface_height(x, z)).abs() < 0.0001);
            }
        }
    }

    #[test]
    fn pages_are_deterministic_bounded_and_have_valid_indices() {
        let a = build(&field(100.), (0, 0, false), &CancelToken::default()).unwrap();
        let b = build(&field(100.), (0, 0, false), &CancelToken::default()).unwrap();
        assert!(!a.mesh.vertices.is_empty());
        assert!(a.mesh.vertices.len() <= VERTICES && a.mesh.indices.len() <= INDICES);
        assert!(
            a.mesh
                .indices
                .iter()
                .all(|&i| (i as usize) < a.mesh.vertices.len())
        );
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.mesh.vertices),
            bytemuck::cast_slice::<_, u8>(&b.mesh.vertices)
        );
        assert_eq!(a.mesh.indices, b.mesh.indices);
        assert_eq!(a.parts[0].1 as usize, a.mesh.indices.len());
    }
    #[test]
    fn distant_pages_fit_their_arena_and_use_colony_cells() {
        let p = build_far(&field(100.), (0, 0, true), &CancelToken::default()).unwrap();
        assert!(!p.mesh.vertices.is_empty());
        assert!(p.mesh.vertices.len() <= FAR_VERTICES && p.mesh.indices.len() <= FAR_INDICES);
        assert!(
            p.mesh
                .vertices
                .iter()
                .all(|v| (12.0..16.0).contains(&v.normal[2]))
        );
        assert!(
            p.mesh
                .indices
                .iter()
                .all(|&i| (i as usize) < p.mesh.vertices.len())
        );
        assert!(vertex_offset(NEAR_SLOTS) >= NEAR_SLOTS * VERTICES);
        assert!(index_offset(NEAR_SLOTS) >= NEAR_SLOTS * INDICES);
        assert!(
            build_far(&field(0.), (0, 0, true), &CancelToken::default())
                .unwrap()
                .mesh
                .indices
                .is_empty()
        );
        let near = build(&field(100.), (0, 0, false), &CancelToken::default()).unwrap();
        assert!(near.parts[1].1 > 0 && near.parts[1].1 < near.parts[0].1 / 2);
    }
    #[test]
    fn water_exclusion_and_cancellation() {
        assert!(
            build(&field(0.), (0, 0, false), &CancelToken::default())
                .unwrap()
                .mesh
                .vertices
                .is_empty()
        );
        let cancel = CancelToken::default();
        cancel.cancel();
        assert!(build(&field(100.), (0, 0, false), &cancel).is_none());
    }
}
