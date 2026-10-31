//! Woodland surface definitions. Engine services have no knowledge of these materials.
use bevy::{
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
};
pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, Layers>;
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, Liquid>;
pub type WindMaterial = ExtendedMaterial<StandardMaterial, Wind>;
pub type FarMaterial = ExtendedMaterial<StandardMaterial, FarFade>;
pub struct SurfacePlugin;
impl Plugin for SurfacePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            MaterialPlugin::<TerrainMaterial>::default(),
            MaterialPlugin::<WaterMaterial>::default(),
            MaterialPlugin::<WindMaterial>::default(),
            MaterialPlugin::<FarMaterial>::default(),
        ));
    }
}
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Layers {
    #[texture(100)]
    #[sampler(101)]
    pub ground: Handle<Image>,
    #[texture(102)]
    #[sampler(103)]
    pub soil: Handle<Image>,
    #[texture(104)]
    #[sampler(105)]
    pub rock: Handle<Image>,
    #[texture(106)]
    #[sampler(107)]
    pub litter: Handle<Image>,
    #[uniform(108)]
    pub scale: Vec4,
}
impl MaterialExtension for Layers {
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wesl".into()
    }
}
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Liquid {
    #[uniform(100)]
    pub settings: Vec4,
}
impl MaterialExtension for Liquid {
    fn fragment_shader() -> ShaderRef {
        "shaders/water.wesl".into()
    }
}
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Wind {
    #[uniform(100)]
    pub settings: Vec4,
}
impl MaterialExtension for Wind {
    fn vertex_shader() -> ShaderRef {
        "shaders/wind.wesl".into()
    }
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct FarFade {
    #[texture(101)]
    #[sampler(102)]
    pub normals: Handle<Image>,
    #[uniform(100)]
    pub settings: Vec4,
}
impl MaterialExtension for FarFade {
    fn fragment_shader() -> ShaderRef {
        "shaders/far-fragment.wesl".into()
    }
    fn vertex_shader() -> ShaderRef {
        "shaders/far.wesl".into()
    }
}
