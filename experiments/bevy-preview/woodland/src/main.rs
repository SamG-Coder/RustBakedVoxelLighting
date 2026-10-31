mod grass_stream;
mod materials;
mod menu;
mod scene;
mod terrain;
mod vegetation;
mod world;
use bevy::prelude::*;
use std::{path::PathBuf, sync::Arc};
fn main() -> anyhow::Result<()> {
    let root = std::env::var_os("BVL_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"));
    let manifest: world::Manifest =
        serde_json::from_slice(&std::fs::read(root.join("world/manifest.json"))?)?;
    anyhow::ensure!(
        manifest.format == 1 && manifest.grid == 2001 && manifest.extent == 4000.,
        "Unsupported world format"
    );
    let field = Arc::new(world::Field::load(&root.join("world"), &manifest)?);
    let captures = serde_json::from_slice(&std::fs::read(root.join("world/impostors.json"))?)?;
    bvl_engine::application(bvl_engine::EngineConfig {
        title: "RustBakedVoxelLighting".into(),
        asset_directory: root.to_string_lossy().into_owned(),
    })
    .add_plugins((materials::SurfacePlugin, menu::MenuPlugin))
    .insert_resource(scene::Source {
        root,
        manifest,
        field,
        captures,
    })
    .init_resource::<vegetation::NearPlants>()
    .init_resource::<grass_stream::GrassStream>()
    .init_resource::<scene::QualitySettings>()
    .add_systems(Startup, scene::setup)
    .add_systems(
        Update,
        (
            vegetation::stream,
            grass_stream::update,
            scene::cycle,
            scene::ui,
            scene::quality,
            vegetation::constrain_player.after(bvl_engine::controller::update),
        ),
    )
    .run();
    Ok(())
}
