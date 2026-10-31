//! Game UI and mode selection; the engine owns only generic input capture.
use crate::scene::{Clock, QualitySettings};
use bevy::prelude::*;
use bvl_engine::controller::Controller;

#[derive(Component)]
pub struct MenuRoot;
#[derive(Component)]
struct MainPage;
#[derive(Component)]
struct SettingsPage;
#[derive(Component, Clone, Copy)]
enum Action {
    Walk,
    Fly,
    Settings,
    Back,
    Quality,
    Time,
    Cycle,
}

fn setup(mut commands: Commands) {
    let root = commands
        .spawn((
            MenuRoot,
            Node {
                position_type: PositionType::Absolute,
                left: px(36),
                top: px(45),
                width: px(360),
                padding: UiRect::all(px(26)),
                flex_direction: FlexDirection::Column,
                row_gap: px(14),
                ..default()
            },
            BackgroundColor(Color::srgba(0.035, 0.065, 0.045, 0.92)),
        ))
        .id();
    commands.spawn((
        Text::new("RustBakedVoxelLighting"),
        TextFont {
            font_size: 24.0.into(),
            ..default()
        },
        ChildOf(root),
    ));
    commands.spawn((
        Text::new("A woodland, now native."),
        TextFont {
            font_size: 17.0.into(),
            ..default()
        },
        ChildOf(root),
    ));
    let main = commands
        .spawn((
            MainPage,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    for (text, action) in [
        ("New Game", Action::Walk),
        ("Fly", Action::Fly),
        ("Settings", Action::Settings),
    ] {
        button(&mut commands, main, text, action);
    }
    let settings = commands
        .spawn((
            SettingsPage,
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    for (text, action) in [
        ("Graphics: toggle SSAO / TAA", Action::Quality),
        ("World: advance 4 hours", Action::Time),
        ("World: pause / resume day", Action::Cycle),
        ("Back", Action::Back),
    ] {
        button(&mut commands, settings, text, action);
    }
    commands.spawn((Text::new("WASD move | Mouse look\nQ / E fly down / up | Shift faster\nTab walk / fly | Esc menu\nF2 time | F3 cycle | F4 SSAO\nF12 screenshot\n\nNative migration preview"), TextFont{font_size:15.0.into(),..default()},ChildOf(root)));
}
fn button(commands: &mut Commands, parent: Entity, text: &str, action: Action) {
    let entity = commands
        .spawn((
            bevy::ui_widgets::Button,
            action,
            Node {
                min_height: px(46),
                padding: UiRect::all(px(12)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.16, 0.22, 0.13)),
            ChildOf(parent),
        ))
        .id();
    commands.spawn((
        Text::new(text),
        TextFont {
            font_size: 18.0.into(),
            ..default()
        },
        ChildOf(entity),
    ));
}
fn update(
    buttons: Query<&Action, Added<bevy::ui::Pressed>>,
    mut controller: ResMut<Controller>,
    mut clock: ResMut<Clock>,
    mut quality: ResMut<QualitySettings>,
    mut root: Single<&mut Node, (With<MenuRoot>, Without<MainPage>, Without<SettingsPage>)>,
    mut main: Single<&mut Node, (With<MainPage>, Without<MenuRoot>, Without<SettingsPage>)>,
    mut settings: Single<&mut Node, (With<SettingsPage>, Without<MenuRoot>, Without<MainPage>)>,
) {
    for action in &buttons {
        match action {
            Action::Walk | Action::Fly => {
                controller.flying = matches!(action, Action::Fly);
                controller.captured = true;
                controller.velocity = Vec3::ZERO;
            }
            Action::Settings => {
                main.display = Display::None;
                settings.display = Display::Flex;
            }
            Action::Back => {
                main.display = Display::Flex;
                settings.display = Display::None;
            }
            Action::Quality => {
                quality.ssao = !quality.ssao;
                quality.dirty = true;
            }
            Action::Time => clock.hour = (clock.hour + 4.) % 24.,
            Action::Cycle => clock.running = !clock.running,
        }
    }
    root.display = if controller.captured {
        Display::None
    } else {
        Display::Flex
    };
}

pub struct MenuPlugin;
impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, update.after(bvl_engine::controller::update));
    }
}
