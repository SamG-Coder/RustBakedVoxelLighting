use crate::assets::*;
use bytemuck::{Pod, Zeroable};
pub const STREAM_VERTICES: usize = 11_200_000;
pub const STREAM_INDICES: usize = 24_300_000;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    pub view_projection: [[f32; 4]; 4],
    pub light_projection: [[f32; 4]; 4],
    pub far_light_projection: [[f32; 4]; 4],
    pub inverse_view_projection: [[f32; 4]; 4],
    pub camera_time: [f32; 4],
    pub sun: [f32; 4],
    pub sky: [f32; 4],
    pub environment: [f32; 4],
    pub weather: [f32; 4],
    pub sun_colour: [f32; 4],
    pub geography_maps: [u32; 4],
}
pub struct DrawLists<'a> {
    pub opaque: &'a [Draw],
    pub shadows: &'a [Draw],
    pub far_shadows: &'a [Draw],
    pub water: &'a [Draw],
}
pub struct Upload<'a> {
    pub range: MeshRange,
    pub mesh: &'a MeshData,
}
