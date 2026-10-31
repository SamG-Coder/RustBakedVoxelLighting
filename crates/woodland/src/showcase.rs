//! Identical, wall-clock driven camera choreography for native and WASM captures.
use crate::map::Map;
use glam::Vec3;
pub const DURATION: f32 = 120.;
pub struct Pose {
    pub position: Vec3,
    pub forward: Vec3,
    pub hour: f32,
}
fn spline(a: Vec3, b: Vec3, c: Vec3, d: Vec3, t: f32) -> Vec3 {
    0.5 * ((2. * b)
        + (-a + c) * t
        + (2. * a - 5. * b + 4. * c - d) * t * t
        + (-a + 3. * b - 3. * c + d) * t * t * t)
}
pub fn sample(map: &Map, seconds: f32) -> Pose {
    let start = Vec3::new(-40., map.field.surface_height(-40., 65.) + 4., 65.);
    let ground = |x: f32, z: f32, h: f32| Vec3::new(x, map.field.surface_height(x, z) + h, z);
    // Authored against the exported world's channel, not river mesh bounds.
    let p = [
        start,
        ground(-100., 45., 12.),
        ground(-200., 55., 9.),
        ground(-238., 105., 6.),
        ground(-245., 175., 12.),
        ground(-180., 200., 65.),
        ground(-70., 130., 105.),
    ];
    let q = [
        ground(-100., 45., 8.),
        ground(-200., 55., 6.),
        ground(-242., 85., 3.),
        ground(-250., 160., 2.),
        ground(-246., 220., 4.),
        ground(-240., 140., 12.),
        ground(-235., 110., 15.),
    ];
    let u = (seconds / DURATION).clamp(0., 1.) * 6.;
    let i = (u.floor() as usize).min(5);
    let t = u - i as f32;
    let at = |v: &[Vec3; 7]| spline(v[i.saturating_sub(1)], v[i], v[i + 1], v[(i + 2).min(6)], t);
    let mut position = at(&p);
    position.y = position
        .y
        .max(map.field.surface_height(position.x, position.z) + 2.5);
    let forward = (at(&q) - position).normalize_or_zero();
    let dusk = ((seconds - 90.) / 30.).clamp(0., 1.);
    let hour = 10.85 + 7.35 * dusk * dusk * (3. - 2. * dusk);
    Pose {
        position,
        forward,
        hour,
    }
}
