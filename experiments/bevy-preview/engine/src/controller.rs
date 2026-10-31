//! Camera input is generic. The game implements terrain following/collisions.
use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
#[derive(Component)]
pub struct PlayerCamera;
#[derive(Resource)]
pub struct Controller {
    pub captured: bool,
    pub flying: bool,
    pub speed: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub velocity: Vec3,
}
impl Default for Controller {
    fn default() -> Self {
        Self {
            captured: false,
            flying: true,
            speed: 15.,
            yaw: 0.,
            pitch: -0.12,
            velocity: Vec3::ZERO,
        }
    }
}
pub fn update(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<AccumulatedMouseMotion>,
    mut control: ResMut<Controller>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut camera: Single<&mut Transform, With<PlayerCamera>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        control.captured = false;
    }
    if control.captured && keys.just_pressed(KeyCode::Tab) {
        control.flying = !control.flying;
        control.velocity = Vec3::ZERO;
    }
    cursor.visible = !control.captured;
    cursor.grab_mode = if control.captured {
        CursorGrabMode::Locked
    } else {
        CursorGrabMode::None
    };
    if !control.captured {
        return;
    }
    control.yaw -= mouse.delta.x * 0.002;
    control.pitch = (control.pitch - mouse.delta.y * 0.002).clamp(-1.53, 1.53);
    camera.rotation = Quat::from_euler(EulerRot::YXZ, control.yaw, control.pitch, 0.);
    let mut forward = *camera.forward();
    let right = *camera.right();
    if !control.flying {
        forward.y = 0.;
        forward = forward.normalize_or_zero();
    }
    let mut direction = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        direction += forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        direction -= forward;
    }
    if keys.pressed(KeyCode::KeyD) {
        direction += right;
    }
    if keys.pressed(KeyCode::KeyA) {
        direction -= right;
    }
    if control.flying {
        if keys.pressed(KeyCode::KeyE) {
            direction.y += 1.;
        }
        if keys.pressed(KeyCode::KeyQ) {
            direction.y -= 1.;
        }
    }
    let speed = if control.flying { control.speed } else { 4.5 }
        * if keys.pressed(KeyCode::ShiftLeft) {
            3.0
        } else {
            1.0
        };
    let dt = time.delta_secs().min(0.05);
    let desired = direction.normalize_or_zero() * speed;
    control.velocity = control.velocity.lerp(desired, 1. - (-dt * 14.).exp());
    camera.translation += control.velocity * dt;
}
