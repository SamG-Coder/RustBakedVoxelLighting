//! Reusable native services. This crate must never depend on the woodland crate.
pub mod assets;
pub mod controller;
pub mod jobs;
use bevy::{
    diagnostic::FrameTimeDiagnosticsPlugin,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    window::PresentMode,
};
pub struct EngineConfig {
    pub title: String,
    pub asset_directory: String,
}
#[derive(Resource)]
struct CapturePath(Option<String>);
pub fn application(config: EngineConfig) -> App {
    let mut args = std::env::args();
    let capture = args.find(|s| s == "--capture").and_then(|_| args.next());
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: config.asset_directory,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: config.title,
                    resolution: (1600, 900).into(),
                    present_mode: PresentMode::AutoVsync,
                    ..default()
                }),
                ..default()
            }),
    )
    .insert_resource(CapturePath(capture))
    .add_plugins(FrameTimeDiagnosticsPlugin::default())
    .init_resource::<controller::Controller>()
    .add_systems(Startup, report_adapter)
    .add_systems(Update, (controller::update, screenshot));
    if std::env::args().any(|s| s == "--profile") {
        app.add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin)
            .add_systems(Update, report_diagnostics);
    }
    app
}
fn screenshot(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut captured: Local<bool>,
    mut exit: MessageWriter<AppExit>,
    capture: Res<CapturePath>,
) {
    let automation = capture.0.as_ref();
    if keys.just_pressed(KeyCode::F12)
        || automation.is_some() && time.elapsed_secs() > 45. && !*captured
    {
        *captured = true;
        let path = automation
            .cloned()
            .unwrap_or_else(|| "screenshot.png".into());
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    if automation.is_some() && time.elapsed_secs() > 52. {
        exit.write(AppExit::Success);
    }
}

fn report_diagnostics(
    time: Res<Time>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    mut last: Local<f32>,
    meshes: Query<&Mesh3d>,
) {
    if time.elapsed_secs() - *last < 10. {
        return;
    }
    *last = time.elapsed_secs();
    println!(
        "PROFILE elapsed={:.1} mesh_entities={}",
        time.elapsed_secs(),
        meshes.iter().count()
    );
    for diagnostic in diagnostics.iter() {
        if let Some(value) = diagnostic.average() {
            println!("{} = {:.3} {}", diagnostic.path(), value, diagnostic.suffix);
        }
    }
}

fn report_adapter(adapter: Res<bevy::render::renderer::RenderAdapterInfo>) { println!("GPU_ADAPTER name={} backend={:?} driver={} {}",adapter.name,adapter.backend,adapter.driver,adapter.driver_info); }
