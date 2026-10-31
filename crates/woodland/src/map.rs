//! Woodland content assembly. Vulkan types never enter this module.
use crate::world::{Field, Impostor, Manifest, Plant};
use anyhow::Result;
use bvl_engine::assets::*;
use glam::Vec3;
use std::{collections::HashMap, path::Path, sync::Arc};
pub struct Prototype {
    pub levels: Vec<[MeshRange; 2]>,
    pub materials: [u32; 2],
    pub bounds_center: Vec3,
    pub bounds_radius: f32,
}
pub struct Tile {
    pub center: Vec3,
    pub terrain: MeshRange,
    pub far: MeshRange,
    pub plants: Vec<Plant>,
}
pub struct Map {
    pub field: Arc<Field>,
    pub prototypes: Vec<Prototype>,
    pub tiles: Vec<Tile>,
    pub rivers: Vec<(Vec3, MeshRange)>,
    pub environment: [f32; 4],
    pub geography_maps: [u32; 4],
    pub ground: u32,
    pub far_material: u32,
    pub water: u32,
    pub grass: u32,
}
fn material(scene: &mut SceneData, m: Material) -> u32 {
    let id = scene.materials.len() as u32;
    scene.materials.push(m);
    id
}
fn texture(
    scene: &mut SceneData,
    cache: &mut HashMap<String, u32>,
    root: &Path,
    name: &str,
) -> Result<u32> {
    if let Some(&id) = cache.get(name) {
        return Ok(id);
    }
    let id = scene.textures.len() as u32;
    scene.textures.push(TextureData::read(
        &root.join("textures").join(format!("{name}.png")),
        !name.contains("normal"),
    )?);
    cache.insert(name.into(), id);
    Ok(id)
}
pub fn load(root: &Path) -> Result<(Map, SceneData)> {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(root.join("world/manifest.json"))?)?;
    anyhow::ensure!(
        manifest.extent == 4000. && manifest.grid == 2001 && manifest.format == 1,
        "Unsupported world format"
    );
    println!(
        "Importing source {}: {} trees / {} shrubs",
        manifest.source_commit,
        manifest.trees.len(),
        manifest.shrubs.len()
    );
    let field = Arc::new(Field::load(&root.join("world"), &manifest)?);
    let captures: Vec<Impostor> =
        serde_json::from_slice(&std::fs::read(root.join("world/impostors.json"))?)?;
    let mut scene = SceneData::default();
    let (voxels, heights) = crate::lighting::build(&field, &manifest, &captures);
    scene.textures.push(voxels);
    scene.textures.push(heights);
    let environment = [
        0.,
        1.,
        crate::lighting::N as f32,
        crate::lighting::LAYERS as f32,
    ];
    scene.transport = Some(TransportBake {
        occupancy: 0,
        heights: 1,
        size: crate::lighting::N as u32,
        layers: crate::lighting::LAYERS as u32,
    });
    let bank = scene.textures.len() as u32;
    let bank_pixels = std::fs::read(root.join("world/riverbank.rgba"))?;
    anyhow::ensure!(
        bank_pixels.len() == field.size * field.size * 4,
        "Riverbank control map dimensions mismatch"
    );
    scene.textures.push(TextureData {
        width: field.size as u32,
        height: field.size as u32,
        pixels: bank_pixels,
        srgb: false,
        mips: Vec::new(),
    });
    let mut geography_maps = [bank, 0, 0, 0];
    let mut cache = HashMap::new();
    let mut ground_maps = [0; 4];
    for (i, n) in ["ground-meadow", "dirt", "scree", "woodland-litter"]
        .iter()
        .enumerate()
    {
        ground_maps[i] = texture(&mut scene, &mut cache, root, n)?;
    }
    // Full-resolution geography is sampled per pixel, independently of mesh tessellation.
    let controls = scene.textures.len() as u32;
    let mut control_pixels = Vec::with_capacity(field.size * field.size * 4);
    for i in 0..field.size * field.size {
        for v in [
            field.values[i * 4 + 1],
            field.ecology[i * 4],
            field.values[i * 4 + 3],
            field.ecology[i * 4 + 1],
        ] {
            control_pixels.push((v.clamp(0., 1.) * 255.).round() as u8);
        }
    }
    scene.textures.push(TextureData {
        width: field.size as u32,
        height: field.size as u32,
        pixels: control_pixels,
        srgb: false,
        mips: Vec::new(),
    });
    geography_maps[1] = controls;
    geography_maps[2] = scene.textures.len() as u32;
    scene
        .textures
        .push(crate::lighting::ground_visibility(&field));
    geography_maps[3] = scene.textures.len() as u32;
    scene
        .textures
        .push(crate::lighting::root_heightfield(&field));
    let habitat_map = scene.textures.len() as u32;
    let mut pixels = Vec::with_capacity(512 * 512 * 4);
    for z in 0..512 {
        for x in 0..512 {
            let x = x as f32 / 511. * 4000. - 2000.;
            let z = z as f32 / 511. * 4000. - 2000.;
            let h = crate::habitat::sample(
                x,
                z,
                field.sample(x, z, 2, false),
                field.sample(x, z, 1, true),
                field.sample(x, z, 3, true),
                field.sample(x, z, 2, true),
            );
            for c in &h.colour[..3] {
                pixels.push((c * 127.5).clamp(0., 255.).round() as u8);
            }
            pixels.push((h.woodland * 255.).clamp(0., 255.).round() as u8);
        }
    }
    scene.textures.push(TextureData {
        width: 512,
        height: 512,
        pixels,
        srgb: false,
        mips: Vec::new(),
    });
    // Whole-map diffuse grass cover, derived from alpha-weighted LINEAR source colours.
    // No sun colour is stored here; day/night and shadows remain live across every LOD.
    let means: Vec<[f32; 3]> =
        serde_json::from_slice(&std::fs::read(root.join("world/grass-means.json"))?)?;
    anyhow::ensure!(
        means.len() == crate::plant_catalog::NAMES.len(),
        "Grass colour bake/catalog mismatch"
    );
    let cover_map = scene.textures.len() as u32;
    let mut pixels = Vec::with_capacity(512 * 512 * 4);
    for z in 0..512 {
        for x in 0..512 {
            let x = x as f32 / 511. * 4000. - 2000.;
            let z = z as f32 / 511. * 4000. - 2000.;
            let river = field.sample(x, z, 2, true);
            let h = crate::habitat::sample(
                x,
                z,
                field.sample(x, z, 2, false),
                field.sample(x, z, 1, true),
                field.sample(x, z, 3, true),
                river,
            );
            for c in 0..3 {
                let colour = h
                    .weights
                    .iter()
                    .zip(&means)
                    .map(|(w, m)| w * m[c])
                    .sum::<f32>()
                    * h.colour[c];
                // Store in sRGB for precision in the dark greens; Vulkan decodes once.
                let encoded = if colour <= 0.0031308 {
                    colour * 12.92
                } else {
                    1.055 * colour.powf(1. / 2.4) - 0.055
                };
                pixels.push((encoded * 255.).clamp(0., 255.).round() as u8);
            }
            let coverage = h.density
                * (1. - crate::habitat::smooth(0.10, 0.32, field.sample(x, z, 1, false)))
                * (1. - crate::habitat::smooth(0.12, 0.78, field.sample(x, z, 0, true)))
                * (1. - crate::habitat::smooth(0.45, 0.64, field.sample(x, z, 3, false)))
                * crate::habitat::smooth(8., 14., river);
            pixels.push((coverage * 255.).clamp(0., 255.).round() as u8);
        }
    }
    scene.textures.push(TextureData {
        width: 512,
        height: 512,
        pixels,
        srgb: true,
        mips: Vec::new(),
    });
    let dirt_normal = texture(&mut scene, &mut cache, root, "dirt-normal")?;
    let rock_normal = texture(&mut scene, &mut cache, root, "rock-normal")?;
    let ground = material(
        &mut scene,
        Material {
            textures: ground_maps,
            extra: [controls, habitat_map, dirt_normal, rock_normal],
            settings: [4., 0., 0.95, cover_map as f32],
            ..Default::default()
        },
    );
    let mut prototypes = Vec::new();
    for variant in &manifest.variants {
        let bark = texture(&mut scene, &mut cache, root, &variant.bark)?;
        let leaf = texture(&mut scene, &mut cache, root, &variant.leaf)?;
        let bark_normal = texture(
            &mut scene,
            &mut cache,
            root,
            &format!("{}-normal", variant.bark),
        )?;
        let leaf_name = format!("{}-normal", variant.leaf);
        let leaf_normal = if root
            .join("textures")
            .join(format!("{leaf_name}.png"))
            .exists()
        {
            Some(texture(&mut scene, &mut cache, root, &leaf_name)?)
        } else {
            None
        };
        let bark = material(
            &mut scene,
            Material {
                textures: [bark; 4],
                extra: [bark_normal, 1, 0, 0],
                settings: [0., 0.4, 0.95, 3.],
                ..Default::default()
            },
        );
        let leaf = material(
            &mut scene,
            Material {
                textures: [leaf; 4],
                extra: [
                    leaf_normal.unwrap_or(0),
                    u32::from(leaf_normal.is_some()),
                    0,
                    0,
                ],
                settings: [1., 0.35, 0.92, 0.],
                ..Default::default()
            },
        );
        let mut levels = Vec::new();
        for level in &variant.levels {
            levels.push([
                scene.mesh(MeshData::read(&root.join("world").join(&level.wood))?),
                scene.mesh(MeshData::read(&root.join("world").join(&level.leaf))?),
            ]);
        }
        let mut minimum = Vec3::splat(f32::INFINITY);
        let mut maximum = Vec3::splat(f32::NEG_INFINITY);
        for level in &levels {
            for range in level {
                for &index in &scene.indices
                    [range.first_index as usize..(range.first_index + range.index_count) as usize]
                {
                    let p = Vec3::from_array(
                        scene.vertices[(range.vertex_offset as usize) + index as usize].position,
                    );
                    minimum = minimum.min(p);
                    maximum = maximum.max(p);
                }
            }
        }
        prototypes.push(Prototype {
            bounds_center: (minimum + maximum) * 0.5,
            bounds_radius: (maximum - minimum).length() * 0.5,
            levels,
            materials: [bark, leaf],
        });
    }
    let atlas = texture(&mut scene, &mut cache, root, "impostor-atlas")?;
    let normals = texture(&mut scene, &mut cache, root, "impostor-normal-atlas")?;
    let far_material = material(
        &mut scene,
        Material {
            textures: [atlas; 4],
            extra: [normals, 0, 0, 0],
            settings: [3., 0.32, 0.95, 0.],
            ..Default::default()
        },
    );
    let mut populations = vec![Vec::new(); 1024];
    for mut p in manifest.trees.into_iter().chain(manifest.shrubs) {
        p.y = field.surface_height(p.x, p.z) - 0.035;
        let x = ((p.x + 2000.) / 125.).clamp(0., 31.) as usize;
        let z = ((p.z + 2000.) / 125.).clamp(0., 31.) as usize;
        populations[z * 32 + x].push(p);
    }
    let mut tiles = Vec::with_capacity(1024);
    for (id, plants) in populations.into_iter().enumerate() {
        let x = (id % 32) as i32;
        let z = (id / 32) as i32;
        let center = Vec3::new(
            x as f32 * 125. - 1937.5,
            field.height(x as f32 * 125. - 1937.5, z as f32 * 125. - 1937.5),
            z as f32 * 125. - 1937.5,
        );
        let terrain = scene.mesh(terrain(&field, x, z));
        let far = scene.mesh(far_trees(&plants, &captures));
        tiles.push(Tile {
            center,
            terrain,
            far,
            plants,
        });
    }
    let water_normal = texture(&mut scene, &mut cache, root, "river-normal")?;
    let water = material(
        &mut scene,
        Material {
            textures: [water_normal; 4],
            settings: [5., 0., 0.1, 0.],
            ..Default::default()
        },
    );
    let mut rivers = Vec::new();
    for name in manifest.rivers {
        let mut mesh = MeshData::read(&root.join("world").join(&name))?;
        let flow = std::fs::read(root.join("world").join(format!("{name}.flow")))?;
        anyhow::ensure!(
            flow.len() == mesh.vertices.len() * 16,
            "River flow/geometry mismatch: {name}"
        );
        for (v, bytes) in mesh.vertices.iter_mut().zip(flow.as_chunks::<16>().0) {
            v.color = std::array::from_fn(|i| {
                f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
            });
            anyhow::ensure!(v.color.iter().all(|v| v.is_finite()), "Invalid river flow");
        }
        let center = mesh
            .vertices
            .iter()
            .map(|v| Vec3::from(v.position))
            .sum::<Vec3>()
            / mesh.vertices.len().max(1) as f32;
        rivers.push((center, scene.mesh(mesh)));
    }
    let normal = texture(&mut scene, &mut cache, root, "grass-lod-normal")?;
    let atlas = texture(&mut scene, &mut cache, root, "grass-lod-atlas")?;
    let grass = material(
        &mut scene,
        Material {
            textures: [atlas; 4],
            extra: [normal, 0, 0, 0],
            settings: [2., 0.34, 0.94, 0.],
            ..Default::default()
        },
    );
    // Acceleration geometry is shared per prototype; placements remain instances.
    let identity = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.];
    for p in &prototypes {
        for part in 0..2 {
            scene.ray_meshes.push(RayMesh {
                mesh: p.levels[1][part],
                material: p.materials[part],
            });
        }
    }
    for tile in &tiles {
        let id = scene.ray_meshes.len() as u32;
        scene.ray_meshes.push(RayMesh {
            mesh: tile.terrain,
            material: ground,
        });
        scene.ray_instances.push(RayInstance {
            mesh: id,
            transform: identity,
        });
        for p in &tile.plants {
            let (sin, cos) = p.rotation.sin_cos();
            let a = p.scale;
            let transform = [
                cos * a,
                0.,
                sin * a,
                p.x,
                0.,
                a,
                0.,
                p.y,
                -sin * a,
                0.,
                cos * a,
                p.z,
            ];
            for part in 0..2 {
                scene.ray_instances.push(RayInstance {
                    mesh: p.variant as u32 * 2 + part,
                    transform,
                });
            }
        }
    }
    Ok((
        Map {
            field,
            prototypes,
            tiles,
            rivers,
            ground,
            environment,
            geography_maps,
            far_material,
            water,
            grass,
        },
        scene,
    ))
}
fn ground_vertex(field: &Field, x: f32, z: f32) -> Vertex {
    let n = Vec3::new(
        field.height(x - 0.7, z) - field.height(x + 0.7, z),
        1.4,
        field.height(x, z - 0.7) - field.height(x, z + 0.7),
    )
    .normalize();
    Vertex {
        position: [x, field.height(x, z), z],
        normal: n.to_array(),
        uv: [x * 0.5, z * 0.5],
        color: [
            field
                .sample(x, z, 1, false)
                .max(field.sample(x, z, 0, true) * 0.7),
            field.sample(x, z, 2, false),
            field.sample(x, z, 3, false),
            field.sample(x, z, 1, true),
        ],
        uv1: [0.; 2],
    }
}
fn terrain(field: &Field, cx: i32, cz: i32) -> MeshData {
    let mut mesh = MeshData::default();
    const SEGMENTS: usize = 32;
    for z in 0..=SEGMENTS {
        for x in 0..=SEGMENTS {
            let wx = cx as f32 * 125. - 2000. + x as f32 * 125. / SEGMENTS as f32;
            let wz = cz as f32 * 125. - 2000. + z as f32 * 125. / SEGMENTS as f32;
            mesh.vertices.push(ground_vertex(field, wx, wz));
        }
    }
    for z in 0..SEGMENTS {
        for x in 0..SEGMENTS {
            let a = (z * (SEGMENTS + 1) + x) as u32;
            let c = a + SEGMENTS as u32 + 1;
            let corners = [a, c, c + 1, a + 1];
            let points = corners.map(|i| Vec3::from(mesh.vertices[i as usize].position));
            let mids = std::array::from_fn::<_, 4, _>(|i| (points[i] + points[(i + 1) % 4]) * 0.5);
            // A shared-edge decision gives adjacent cells and pages identical edge vertices.
            // Only river corridors need source-heightfield-scale geometry.
            let refine = field.refined_edges(points[0].x, points[0].z);
            if !refine.iter().any(|v| *v) {
                mesh.indices.extend([a, c, a + 1, a + 1, c, c + 1]);
                continue;
            }
            let center = (points[0] + points[2]) * 0.5;
            let center_index = mesh.vertices.len() as u32;
            mesh.vertices.push(ground_vertex(field, center.x, center.z));
            let mut ring = [0_u32; 8];
            let mut count = 0;
            for i in 0..4 {
                ring[count] = corners[i];
                count += 1;
                if refine[i] {
                    ring[count] = mesh.vertices.len() as u32;
                    count += 1;
                    mesh.vertices
                        .push(ground_vertex(field, mids[i].x, mids[i].z));
                }
            }
            for i in 0..count {
                mesh.indices
                    .extend([center_index, ring[i], ring[(i + 1) % count]]);
            }
        }
    }
    mesh
}
fn far_trees(plants: &[Plant], captures: &[Impostor]) -> MeshData {
    let mut mesh = MeshData::default();
    for p in plants {
        let c = &captures[p.variant];
        let half = c.width * 0.5;
        let bottom = c.bottom;
        let top = bottom + c.height;
        let (s, co) = p.rotation.sin_cos();
        for side in 0..2 {
            let mut points = [
                Vec3::new(-half, bottom, 0.),
                Vec3::new(half, bottom, 0.),
                Vec3::new(half, top, 0.),
                Vec3::new(-half, top, 0.),
            ];
            for point in &mut points {
                if side == 1 {
                    *point = Vec3::new(0., point.y, point.x);
                }
                *point = Vec3::new(
                    point.x * co + point.z * s,
                    point.y,
                    -point.x * s + point.z * co,
                ) * p.scale
                    + Vec3::new(p.x, p.y, p.z);
            }
            let u = (p.variant % 4) as f32 * 0.25 + side as f32 * 0.125;
            let v = (p.variant / 4) as f32 * 0.25;
            mesh.quad(
                points,
                [[u, v + 0.25], [u + 0.125, v + 0.25], [u + 0.125, v], [u, v]],
                Vec3::Y,
                [s, co, 1., p.y],
                [p.x, p.z],
            );
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grounding_matches_rendered_triangles_including_refined_banks() {
        let size = 33;
        let mut values = Vec::new();
        let mut ecology = Vec::new();
        for z in 0..size {
            for x in 0..size {
                let wx = x as f32 * 125. - 2000.;
                let h = (x as f32 * 0.8).sin() * 18.
                    + (z as f32 * 0.6).cos() * 12.
                    + x as f32 * z as f32 * 0.3;
                values.extend([h, 0., 0.5, 0.]);
                ecology.extend([0., 0., (wx + 100.).abs() * 0.5, 0.]);
            }
        }
        let field = Field {
            size,
            extent: 4000.,
            values,
            ecology,
        };
        for cx in [15, 16] {
            let mesh = terrain(&field, cx, 16);
            for t in mesh.indices.as_chunks::<3>().0 {
                let a = Vec3::from(mesh.vertices[t[0] as usize].position);
                let b = Vec3::from(mesh.vertices[t[1] as usize].position);
                let c = Vec3::from(mesh.vertices[t[2] as usize].position);
                let p = a * 0.2 + b * 0.3 + c * 0.5;
                assert!(
                    (field.surface_height(p.x, p.z) - p.y).abs() < 0.001,
                    "triangle mismatch at {p:?}"
                );
            }
        }
    }
}
