#![cfg(target_arch = "wasm32")]
use bvl_engine::gpu::{DrawLists, Renderer};
use glam::{Mat4, Vec3};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use woodland::{grass::Grass, map::Map, view::View, web_package::Package};
async fn fetch(path: &str) -> Result<Vec<u8>, JsValue> {
    let global = js_sys::global();
    let function = js_sys::Reflect::get(&global, &"woodlandAssetFetch".into())?
        .dyn_into::<js_sys::Function>()?;
    let promise = function
        .call1(&global, &path.into())?
        .dyn_into::<js_sys::Promise>()?;
    let response = wasm_bindgen_futures::JsFuture::from(promise)
        .await?
        .dyn_into::<web_sys::Response>()?;
    if !response.ok() {
        return Err(format!("{}: HTTP {}", path, response.status()).into());
    }
    let buffer = wasm_bindgen_futures::JsFuture::from(response.array_buffer()?).await?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}
#[wasm_bindgen]
pub struct BrowserGame {
    renderer: Renderer,
    map: Map,
    view: View,
    grass: Grass,
    camera: Vec3,
    yaw: f32,
    pitch: f32,
    keys: u32,
    mode: u8,
    last: f32,
    hour: f32,
    frames: u32,
    draws: usize,
}
#[wasm_bindgen]
impl BrowserGame {
    #[wasm_bindgen(js_name=create)]
    pub async fn create(
        canvas: web_sys::HtmlCanvasElement,
        progress: js_sys::Function,
    ) -> Result<BrowserGame, JsValue> {
        console_error_panic_hook::set_once();
        let status = |s: &str| {
            let _ = progress.call1(&JsValue::NULL, &s.into());
        };
        status("Starting Rust / WebGPU");
        let mut renderer = Renderer::new(canvas)
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        let p: Package = serde_json::from_slice(&fetch("assets/package.json").await?)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        if p.version != 1 {
            return Err("Unsupported browser package version".into());
        }
        status("Loading terrain, ecology and world placements");
        let map: Map = bincode::deserialize(&fetch("assets/map.bin").await?)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        status("Uploading terrain and tree meshes");
        renderer.mesh_buffer(&fetch("assets/vertices.bin").await?, true);
        renderer.mesh_buffer(&fetch("assets/indices.bin").await?, false);
        for (i, t) in p.textures.iter().enumerate() {
            status(&format!(
                "Uploading materials and mipmaps {}/{}",
                i + 1,
                p.textures.len()
            ));
            let raw = fetch(&format!("assets/texture-{i}.bin")).await?;
            renderer.texture(t.width, t.height, t.srgb, &t.sizes, &raw);
        }
        renderer.materials(&p.materials, map.geography_maps);
        renderer
            .bake_lighting(map.environment, map.geography_maps, |slice, total| {
                status(&format!(
                    "GPU baking full-map light transfer {slice}/{total}"
                ))
            })
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        let camera = Vec3::new(-40., map.field.surface_height(-40., 65.) + 4., 65.);
        let view = View::new(map.prototypes.len());
        let grass = Grass::new(p.vertices, p.indices);
        status("Ready");
        Ok(Self {
            renderer,
            map,
            view,
            grass,
            camera,
            yaw: 0.45,
            pitch: -0.1,
            keys: 0,
            mode: 0,
            last: 0.,
            hour: 10.,
            frames: 0,
            draws: 0,
        })
    }
    pub fn mode(&mut self, mode: u8) {
        self.mode = mode;
        self.keys = 0;
    }
    pub fn keys(&mut self, keys: u32) {
        self.keys = keys;
    }
    pub fn look(&mut self, x: f32, y: f32) {
        if self.mode > 0 {
            self.yaw += x * 0.002;
            self.pitch = (self.pitch - y * 0.002).clamp(-1.52, 1.52);
        }
    }
    pub fn time_of_day(&mut self, hour: f32) {
        self.hour = hour.rem_euclid(24.);
    }
    pub fn frame(&mut self, time: f32, width: u32, height: u32) -> Result<(), JsValue> {
        let dt = if self.last == 0. {
            0.
        } else {
            (time - self.last).clamp(0., 0.05)
        };
        self.last = time;
        self.renderer.resize(width, height);
        let forward = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        );
        if self.mode > 0 {
            let f = if self.mode == 1 {
                Vec3::new(forward.x, 0., forward.z).normalize()
            } else {
                forward
            };
            let right = Vec3::new(self.yaw.cos(), 0., self.yaw.sin());
            let mut v = Vec3::ZERO;
            for (bit, direction) in [
                (1, f),
                (2, -f),
                (4, -right),
                (8, right),
                (16, Vec3::Y),
                (32, -Vec3::Y),
            ] {
                if self.keys & bit != 0 {
                    v += direction;
                }
            }
            if v.length_squared() > 0. {
                self.camera += v.normalize()
                    * dt
                    * if self.keys & 64 != 0 {
                        35.
                    } else if self.mode == 1 {
                        4.5
                    } else {
                        12.
                    };
            }
            self.camera.x = self.camera.x.clamp(-1990., 1990.);
            self.camera.z = self.camera.z.clamp(-1990., 1990.);
            let ground = self.map.field.surface_height(self.camera.x, self.camera.z);
            if self.mode == 1 {
                self.camera.y = ground + 1.75;
            } else {
                self.camera.y = self.camera.y.max(ground + 0.3);
            }
        }
        self.hour = (self.hour + dt / 60.).rem_euclid(24.);
        let mut g = self.view.prepare(
            &self.map,
            self.camera,
            forward,
            width as f32 / height.max(1) as f32,
            time,
            self.hour,
        );
        let native_vp = Mat4::from_cols_array_2d(&g.view_projection);
        if let Some((range, page)) = self
            .grass
            .update(&self.map.field, self.camera, forward, time)
        {
            self.renderer.upload(range, &page.mesh);
        }
        self.grass.append(
            self.camera,
            time,
            native_vp,
            self.map.grass,
            &mut self.view.instances,
            &mut self.view.draws,
        );
        self.grass.append_shadows(
            self.camera,
            time,
            self.map.grass,
            &mut self.view.instances,
            &mut self.view.shadows,
        );
        // Vulkan's framebuffer Y flip is not part of WebGPU's clip convention.
        let flip = Mat4::from_scale(Vec3::new(1., -1., 1.));
        let vp = flip * native_vp;
        g.view_projection = vp.to_cols_array_2d();
        g.light_projection =
            (flip * Mat4::from_cols_array_2d(&g.light_projection)).to_cols_array_2d();
        g.far_light_projection =
            (flip * Mat4::from_cols_array_2d(&g.far_light_projection)).to_cols_array_2d();
        g.inverse_view_projection = vp.inverse().to_cols_array_2d();
        self.draws = self.view.draws.len() + self.view.water.len();
        self.renderer
            .render(
                &g,
                &self.view.instances,
                DrawLists {
                    opaque: &self.view.draws,
                    water: &self.view.water,
                    shadows: &self.view.shadows,
                    far_shadows: &self.view.far_shadows,
                },
            )
            .map_err(|e| JsValue::from_str(&e))?;
        self.frames += 1;
        Ok(())
    }
    pub fn stats(&self) -> String {
        format!(
            "Rust WASM · WebGPU · {} draws · {} grass pages · frame {} · {:0.0}, {:0.0}, {:0.0}",
            self.draws,
            self.grass.count(),
            self.frames,
            self.camera.x,
            self.camera.y,
            self.camera.z
        )
    }
}
