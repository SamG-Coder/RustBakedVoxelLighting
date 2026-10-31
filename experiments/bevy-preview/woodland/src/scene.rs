use crate::materials::{
    FarFade, FarMaterial, Layers, Liquid, TerrainMaterial, WaterMaterial, Wind, WindMaterial,
};
use crate::{
    terrain,
    vegetation::{self, Prototype},
    world::{Field, Impostor, Manifest, Plant},
};
use bevy::{
    camera::{Exposure, Hdr, visibility::VisibilityRange},
    core_pipeline::tonemapping::Tonemapping,
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, CascadeShadowConfigBuilder,
        DirectionalLightShadowMap, SunDisk, atmosphere::ScatteringMedium,
    },
    pbr::AtmosphereSettings,
    post_process::bloom::Bloom,
    prelude::*,
};
use bvl_engine::{
    assets::{MeshData, texture},
    controller::{Controller, PlayerCamera},
};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

#[derive(Resource)]
pub struct Source {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub field: Arc<Field>,
    pub captures: Vec<Impostor>,
}
#[derive(Resource)]
pub struct WorldState {
    pub field: Arc<Field>,
    pub tiles: Vec<Vec<Plant>>,
    pub prototypes: Vec<Prototype>,
    pub grass_materials: Vec<Handle<WindMaterial>>,
    pub far_materials: Vec<Handle<FarMaterial>>,
}
#[derive(Component)]
pub struct Sun;
#[derive(Component)]
pub struct Moon;
#[derive(Resource)]
pub struct Clock {
    pub hour: f32,
    pub running: bool,
}
#[derive(Component)]
pub struct Status;
pub fn setup(
    mut commands: Commands,
    source: Res<Source>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut wind_materials: ResMut<Assets<WindMaterial>>,
    mut far_materials: ResMut<Assets<FarMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut media: ResMut<Assets<ScatteringMedium>>,
    mut controller: ResMut<Controller>,
) {
    info!(
        "Loading native world from {}",
        source.manifest.source_commit
    );
    let mut textures = HashMap::new();
    let mut tex = |name: &str, alpha: bool| -> Handle<Image> {
        textures
            .entry(name.to_string())
            .or_insert_with(|| {
                images.add(
                    texture(
                        &source.root.join("textures").join(format!("{name}.png")),
                        !name.contains("normal"),
                        alpha,
                    )
                    .expect("Texture import"),
                )
            })
            .clone()
    };
    let ground = terrain_materials.add(TerrainMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.95,
            ..default()
        },
        extension: Layers {
            ground: tex("ground-meadow", false),
            soil: tex("dirt", false),
            rock: tex("scree", false),
            litter: tex("woodland-litter", false),
            scale: Vec4::new(0.5, 0., 0., 0.),
        },
    });
    for z in 0..32 {
        for x in 0..32 {
            commands.spawn((
                Mesh3d(meshes.add(terrain::tile(&source.field, x, z, 32).into_mesh())),
                MeshMaterial3d(ground.clone()),
            ));
        }
    }
    let mut prototypes = Vec::new();
    for variant in &source.manifest.variants {
        let bark = materials.add(StandardMaterial {
            base_color_texture: Some(tex(&variant.bark, false)),
            perceptual_roughness: 0.92,
            ..default()
        });
        let leaf = materials.add(StandardMaterial {
            base_color_texture: Some(tex(&variant.leaf, true)),
            alpha_mode: AlphaMode::Mask(0.4),
            double_sided: true,
            cull_mode: None,
            diffuse_transmission: 0.32,
            perceptual_roughness: 0.85,
            ..default()
        });
        let mut wood = Vec::new();
        let mut leaves = Vec::new();
        for level in &variant.levels {
            wood.push(
                meshes.add(
                    MeshData::read(&source.root.join("world").join(&level.wood))
                        .unwrap()
                        .into_mesh(),
                ),
            );
            leaves.push(
                meshes.add(
                    MeshData::read(&source.root.join("world").join(&level.leaf))
                        .unwrap()
                        .into_mesh(),
                ),
            );
        }
        prototypes.push(Prototype {
            wood,
            leaves,
            bark,
            leaf,
        });
    }
    let far_normal = tex("impostor-normal-atlas", false);
    let far = StandardMaterial {
        base_color_texture: Some(tex("impostor-atlas", true)),
        alpha_mode: AlphaMode::Mask(0.4),
        double_sided: true,
        cull_mode: None,
        diffuse_transmission: 0.2,
        perceptual_roughness: 0.95,
        ..default()
    };
    let mut far_handles = Vec::with_capacity(1024);
    let mut tiles = vec![Vec::new(); 1024];
    for p in source.manifest.trees.iter().chain(&source.manifest.shrubs) {
        let x = ((p.x + 2000.) / 125.).clamp(0., 31.) as usize;
        let z = ((p.z + 2000.) / 125.).clamp(0., 31.) as usize;
        tiles[z * 32 + x].push(p.clone());
    }
    for (id, plants) in tiles.iter().enumerate() {
        let x = (id % 32) as f32 * 125. - 1937.5;
        let z = (id / 32) as f32 * 125. - 1937.5;
        let center = Vec3::new(x, source.field.height(x, z), z);
        let far_handle = far_materials.add(FarMaterial {
            base: far.clone(),
            extension: FarFade {
                normals: far_normal.clone(),
                settings: Vec4::new(200., 260., 0., 0.),
            },
        });
        far_handles.push(far_handle.clone());
        if !plants.is_empty() {
            commands.spawn((
                Mesh3d(
                    meshes.add(vegetation::far_mesh(plants, &source.captures, center).into_mesh()),
                ),
                MeshMaterial3d(far_handle),
                bevy::light::NotShadowCaster,
                Transform::from_translation(center),
                VisibilityRange {
                    start_margin: 0.0..0.0,
                    end_margin: 6000.0..6500.0,
                    use_aabb: false,
                },
            ));
        }
    }
    let water = water_materials.add(WaterMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.035, 0.10, 0.11),
            perceptual_roughness: 0.12,
            reflectance: 0.5,
            ..default()
        },
        extension: Liquid {
            settings: Vec4::new(0.12, 0., 0., 0.),
        },
    });
    for name in &source.manifest.rivers {
        commands.spawn((
            Mesh3d(
                meshes.add(
                    MeshData::read(&source.root.join("world").join(name))
                        .unwrap()
                        .into_mesh(),
                ),
            ),
            MeshMaterial3d(water.clone()),
        ));
    }
    let grass_materials = [
        "meadow",
        "meadow-long",
        "meadow-seeds",
        "flowers-yellow",
        "flowers-purple",
        "fern",
    ]
    .map(|name| {
        wind_materials.add(WindMaterial {
            base: StandardMaterial {
                base_color_texture: Some(tex(name, true)),
                alpha_mode: AlphaMode::Mask(0.4),
                double_sided: true,
                cull_mode: None,
                perceptual_roughness: 0.93,
                diffuse_transmission: 0.25,
                ..default()
            },
            extension: Wind {
                settings: Vec4::new(0.10, 0., 0., 0.),
            },
        })
    })
    .to_vec();
    commands.insert_resource(WorldState {
        field: source.field.clone(),
        tiles,
        prototypes,
        grass_materials,
        far_materials: far_handles,
    });
    commands.insert_resource(Clock {
        hour: 10.5,
        running: true,
    });
    commands.insert_resource(GlobalAmbientLight {
        brightness: 50.,
        ..default()
    });
    commands.insert_resource(DirectionalLightShadowMap { size: 2048 });
    commands.spawn(Atmosphere::earth(
        media.add(ScatteringMedium::earth(256, 256)),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 100000.,
            shadow_maps_enabled: true,
            ..default()
        },
        Sun,
        SunDisk::EARTH,
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            first_cascade_far_bound: 20.,
            maximum_distance: 300.,
            ..default()
        }
        .build(),
        Transform::from_xyz(-1., 1., -0.5).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 0.,
            color: Color::srgb(0.65, 0.75, 1.),
            shadow_maps_enabled: false,
            ..default()
        },
        Moon,
        Transform::default(),
    ));
    let start = Vec3::new(120., source.field.height(120., 160.) + 18., 160.);
    let target = Vec3::new(0., source.field.height(0., 0.) + 10., 0.);
    let camera = Transform::from_translation(start).looking_at(target, Vec3::Y);
    let (yaw, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
    controller.yaw = yaw;
    controller.pitch = pitch;
    commands.spawn((
        Camera3d::default(),
        PlayerCamera,
        camera,
        Hdr,
        Msaa::Sample4,
        Exposure { ev100: 13. },
        Tonemapping::AcesFitted,
        AtmosphereSettings::default(),
        AtmosphereEnvironmentMapLight::default(),
        Bloom::NATURAL,
        Projection::Perspective(PerspectiveProjection {
            far: 7000.,
            ..default()
        }),
    ));
    commands.spawn((
        Text::new("Preparing woodland"),
        TextFont {
            font_size: 16.0.into(),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            left: px(24),
            bottom: px(15),
            ..default()
        },
        Status,
    ));
}
pub fn cycle(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut clock: ResMut<Clock>,
    mut sun: Single<(&mut Transform, &mut DirectionalLight), (With<Sun>, Without<Moon>)>,
    mut moon: Single<(&mut Transform, &mut DirectionalLight), (With<Moon>, Without<Sun>)>,
    mut camera: Single<&mut Exposure, With<PlayerCamera>>,
) {
    if keys.just_pressed(KeyCode::F2) {
        clock.hour = (clock.hour + 4.) % 24.;
    }
    if keys.just_pressed(KeyCode::F3) {
        clock.running = !clock.running;
    }
    if clock.running {
        clock.hour = (clock.hour + time.delta_secs() / 60.) % 24.;
    }
    let angle = (clock.hour - 6.) * std::f32::consts::PI / 12.;
    let direction = Vec3::new(
        -0.7808688 * angle.cos(),
        angle.sin(),
        -0.624695 * angle.cos(),
    );
    let daylight = ((direction.y + 0.06) / 0.26).clamp(0., 1.);
    sun.0.rotation = Transform::from_translation(direction)
        .looking_at(Vec3::ZERO, Vec3::Y)
        .rotation;
    sun.1.illuminance = 100000. * daylight;
    moon.0.rotation = Transform::from_translation(-direction)
        .looking_at(Vec3::ZERO, Vec3::Y)
        .rotation;
    moon.1.illuminance = (1. - daylight) * 0.5;
    camera.ev100 = 5. + 8. * daylight;
}
pub fn ui(
    time: Res<Time>,
    clock: Res<Clock>,
    controller: Res<Controller>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    mut text: Single<&mut Text, With<Status>>,
    mut timer: Local<f32>,
) {
    *timer += time.delta_secs();
    if *timer < 0.5 {
        return;
    }
    *timer = 0.;
    let fps = diagnostics
        .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|v| v.smoothed())
        .unwrap_or(0.);
    text.0 = format!(
        "{fps:0.0} FPS   •   {}   •   {:02}:{:02}   •   4 × 4 km",
        if controller.flying { "Fly" } else { "Walk" },
        clock.hour as u32,
        ((clock.hour % 1.) * 60.) as u32
    );
}
#[derive(Resource, Default)]
pub struct QualitySettings {
    pub ssao: bool,
    pub dirty: bool,
}
pub fn quality(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    camera: Single<Entity, With<PlayerCamera>>,
    mut settings: ResMut<QualitySettings>,
) {
    if keys.just_pressed(KeyCode::F4) {
        settings.ssao = !settings.ssao;
        settings.dirty = true;
    }
    if !settings.dirty {
        return;
    }
    settings.dirty = false;
    if settings.ssao {
        commands.entity(*camera).insert((
            Msaa::Off,
            bevy::pbr::ScreenSpaceAmbientOcclusion::default(),
            bevy::anti_alias::taa::TemporalAntiAliasing::default(),
        ));
    } else {
        commands
            .entity(*camera)
            .remove::<(
                bevy::pbr::ScreenSpaceAmbientOcclusion,
                bevy::anti_alias::taa::TemporalAntiAliasing,
            )>()
            .insert(Msaa::Sample4);
    }
}
