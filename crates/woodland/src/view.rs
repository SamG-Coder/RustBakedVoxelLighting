//! Game visibility and LOD policy, independent of Vulkan resource ownership.
use crate::map::Map;
use bvl_engine::{assets::*, gpu::Globals};
use glam::{Mat4, Vec3, Vec4};
pub struct View {
    pub instances: Vec<Instance>,
    pub draws: Vec<Draw>,
    pub shadows: Vec<Draw>,
    pub far_shadows: Vec<Draw>,
    pub water: Vec<Draw>,
    groups: Vec<Vec<Instance>>,
    shadow_groups: [Vec<Vec<Instance>>; 2],
}
impl View {
    pub fn new(prototypes: usize) -> Self {
        Self {
            instances: Vec::with_capacity(30000),
            draws: Vec::with_capacity(4000),
            shadows: Vec::with_capacity(1000),
            far_shadows: Vec::with_capacity(1000),
            shadow_groups: std::array::from_fn(|_| {
                (0..prototypes * 3)
                    .map(|_| Vec::with_capacity(2048))
                    .collect()
            }),
            water: Vec::with_capacity(400),
            groups: (0..prototypes * 3)
                .map(|_| Vec::with_capacity(2048))
                .collect(),
        }
    }
    fn add_plant(&mut self, id: usize, instance: Instance, visible: bool, casters: [bool; 2]) {
        if visible {
            self.groups[id].push(instance);
        }
        for (cascade, enabled) in casters.into_iter().enumerate() {
            if enabled {
                self.shadow_groups[cascade][id].push(instance);
            }
        }
    }
    pub fn prepare(
        &mut self,
        map: &Map,
        camera: Vec3,
        forward: Vec3,
        aspect: f32,
        time: f32,
        hour: f32,
    ) -> Globals {
        self.instances.clear();
        self.instances.push(Instance::default());
        self.draws.clear();
        self.shadows.clear();
        self.far_shadows.clear();
        for groups in &mut self.shadow_groups {
            for group in groups {
                group.clear();
            }
        }
        self.water.clear();
        for g in &mut self.groups {
            g.clear();
        }
        let mut projection = Mat4::perspective_rh(65_f32.to_radians(), aspect, 0.12, 6500.);
        projection.y_axis.y *= -1.;
        let vp = projection * Mat4::look_at_rh(camera, camera + forward, Vec3::Y);
        let planes = frustum(vp);
        let phase = (hour - 6.) / 24. * std::f32::consts::TAU;
        let sun = Vec3::new(phase.cos() * 0.6, phase.sin(), phase.cos() * 0.8).normalize();
        let daylight = ((sun.y + 0.12) / 0.09375).clamp(0., 1.);
        let light = if sun.y > 0. { sun } else { -sun };
        let target = Vec3::new(camera.x, map.field.height(camera.x, camera.z), camera.z);
        let orientation = Mat4::look_at_rh(light * 700., Vec3::ZERO, Vec3::Z);
        let shadow_matrix = |radius: f32| {
            let mut center = orientation.transform_point3(target);
            let texel = radius * 2. / 2048.;
            center.x = (center.x / texel).round() * texel;
            center.y = (center.y / texel).round() * texel;
            let target = orientation.inverse().transform_point3(center);
            let mut ortho = Mat4::orthographic_rh(-radius, radius, -radius, radius, 0.1, 1400.);
            ortho.y_axis.y *= -1.;
            ortho * Mat4::look_at_rh(target + light * 700., target, Vec3::Z)
        };
        let light_vp = shadow_matrix(64.);
        let far_light_vp = shadow_matrix(256.);
        let shadow_planes = [frustum(light_vp), frustum(far_light_vp)];
        for tile in &map.tiles {
            let distance = (tile.center - camera).length();
            let visible = intersects(&planes, tile.center + Vec3::Y * 15., 125.);
            let terrain = Draw {
                mesh: tile.terrain,
                material: map.ground,
                first_instance: 0,
                instances: 1,
            };
            if visible {
                self.draws.push(terrain);
                if tile.far.index_count > 0 {
                    self.draws.push(Draw {
                        mesh: tile.far,
                        material: map.far_material,
                        ..terrain
                    });
                }
            }
            if distance < 500. {
                if intersects(&shadow_planes[0], tile.center, 160.) {
                    self.shadows.push(terrain);
                }
                if intersects(&shadow_planes[1], tile.center, 160.) {
                    self.far_shadows.push(terrain);
                }
            }
            if distance > 620. {
                continue;
            }
            for p in &tile.plants {
                let pos = Vec3::new(p.x, p.y, p.z);
                let d = pos.distance(camera);
                if d > 480. {
                    continue;
                }
                // Caster selection is based on each light frustum, independently of the camera.
                let prototype = &map.prototypes[p.variant];
                let (rotation_sin, rotation_cos) = p.rotation.sin_cos();
                let c = prototype.bounds_center * p.scale;
                let center = pos
                    + Vec3::new(
                        c.x * rotation_cos + c.z * rotation_sin,
                        c.y,
                        -c.x * rotation_sin + c.z * rotation_cos,
                    );
                let radius = prototype.bounds_radius * p.scale.abs() + 8.;
                let casters = shadow_planes.map(|planes| intersects(&planes, center, radius));
                if !visible && !casters[0] && !casters[1] {
                    continue;
                }
                let (s, c) = p.rotation.sin_cos();
                let base = Instance {
                    position_scale: [p.x, p.y, p.z, p.scale],
                    rotation_tint: [s, c, 1., 1.],
                    ..Default::default()
                };
                let (lo, hi) = if d < 90. { (0, 1) } else { (1, 2) };
                let (a, b) = if lo == 0 { (70., 90.) } else { (200., 260.) };
                if d < b {
                    let blend = ((d - a) / (b - a)).clamp(0., 1.);
                    if blend < 1. {
                        self.add_plant(
                            p.variant * 3 + lo,
                            Instance {
                                fade: [1. - blend, 0., 0., 0.],
                                ..base
                            },
                            visible,
                            casters,
                        );
                    }
                    if blend > 0. {
                        self.add_plant(
                            p.variant * 3 + hi,
                            Instance {
                                fade: [blend, 1., 0., 0.],
                                ..base
                            },
                            visible,
                            casters,
                        );
                    }
                } else {
                    self.add_plant(
                        p.variant * 3 + 2,
                        Instance {
                            fade: [1. - ((d - 400.) / 80.).clamp(0., 1.), 0., 0., 0.],
                            ..base
                        },
                        visible,
                        casters,
                    );
                }
            }
        }
        for (id, group) in self.groups.iter().enumerate() {
            if group.is_empty() {
                continue;
            }
            let first = self.instances.len() as u32;
            self.instances.extend_from_slice(group);
            let p = &map.prototypes[id / 3];
            let level = id % 3;
            for part in 0..2 {
                let draw = Draw {
                    mesh: p.levels[level][part],
                    material: p.materials[part],
                    first_instance: first,
                    instances: group.len() as u32,
                };
                self.draws.push(draw);
            }
        }
        for cascade in 0..2 {
            // Front-to-back within each instanced batch preserves geometry and
            // coverage while improving early depth rejection of canopy layers.
            for group in &mut self.shadow_groups[cascade] {
                group.sort_unstable_by(|a, b| {
                    let a = Vec3::from_slice(&a.position_scale[..3]).dot(light);
                    let b = Vec3::from_slice(&b.position_scale[..3]).dot(light);
                    b.total_cmp(&a)
                });
            }
            for (id, group) in self.shadow_groups[cascade].iter().enumerate() {
                if group.is_empty() {
                    continue;
                }
                let first = self.instances.len() as u32;
                self.instances.extend_from_slice(group);
                let prototype = &map.prototypes[id / 3];
                let mut start = 0;
                while start < group.len() {
                    let depth_bin = (Vec3::from_slice(&group[start].position_scale[..3]).dot(light)
                        / 32.)
                        .floor() as i32;
                    let mut end = start + 1;
                    while end < group.len()
                        && (Vec3::from_slice(&group[end].position_scale[..3]).dot(light) / 32.)
                            .floor() as i32
                            == depth_bin
                    {
                        end += 1;
                    }
                    for part in 0..2 {
                        let draw = Draw {
                            mesh: prototype.levels[id % 3][part],
                            material: prototype.materials[part],
                            first_instance: first + start as u32,
                            instances: (end - start) as u32,
                        };
                        if cascade == 0 {
                            self.shadows.push(draw);
                        } else {
                            self.far_shadows.push(draw);
                        }
                    }
                    start = end;
                }
            }
        }
        for draws in [&mut self.shadows, &mut self.far_shadows] {
            draws.sort_unstable_by(|a, b| {
                let a = Vec3::from_slice(
                    &self.instances[a.first_instance as usize].position_scale[..3],
                )
                .dot(light);
                let b = Vec3::from_slice(
                    &self.instances[b.first_instance as usize].position_scale[..3],
                )
                .dot(light);
                b.total_cmp(&a)
            });
        }
        for &(center, mesh) in &map.rivers {
            if intersects(&planes, center, 150.) {
                self.water.push(Draw {
                    mesh,
                    material: map.water,
                    first_instance: 0,
                    instances: 1,
                });
            }
        }
        Globals {
            view_projection: vp.to_cols_array_2d(),
            light_projection: light_vp.to_cols_array_2d(),
            far_light_projection: far_light_vp.to_cols_array_2d(),
            inverse_view_projection: vp.inverse().to_cols_array_2d(),
            camera_time: [camera.x, camera.y, camera.z, time],
            sun: [light.x, light.y, light.z, 0.035 + daylight * 2.6],
            // Fixed day/night adaptation preserves readable moonlit surfaces after
            // the HDR tone-map shoulder/toe, without camera-dependent pumping.
            sky: [
                0.025 + daylight * 0.975,
                1.05 + (1. - daylight) * 4.95,
                daylight,
                hour,
            ],
            environment: map.environment,
            geography_maps: map.geography_maps,
            weather: [0., 0., 0., 0.],
            sun_colour: if sun.y > 0. {
                [
                    1.,
                    0.72 + sun.y.max(0.).sqrt() * 0.25,
                    0.47 + sun.y.max(0.).sqrt() * 0.43,
                    1.,
                ]
            } else {
                [0.48, 0.62, 1., 1.]
            },
        }
    }
}
pub(crate) fn frustum(m: Mat4) -> [Vec4; 6] {
    let r = m.transpose();
    let a = r.x_axis;
    let b = r.y_axis;
    let c = r.z_axis;
    let d = r.w_axis;
    [d + a, d - a, d + b, d - b, c, d - c].map(|p| p / p.truncate().length())
}
pub(crate) fn intersects(p: &[Vec4; 6], center: Vec3, radius: f32) -> bool {
    p.iter().all(|p| p.truncate().dot(center) + p.w >= -radius)
}
