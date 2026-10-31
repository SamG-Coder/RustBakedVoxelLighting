mod grass;
mod habitat;
mod lighting;
mod map;
mod plant_catalog;
mod view;
mod world;
use anyhow::Result;
use bvl_engine::gpu::{DrawLists, Renderer, Shaders, Upload};
use glam::Vec3;
use std::{collections::HashSet, path::PathBuf, time::Instant};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};
// Packaged builds load their own assets regardless of the launch working directory.
// The source-tree fallback retains cargo run support during development.
fn asset_root() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("BVL_ASSETS") {
        return Ok(PathBuf::from(path));
    }
    let executable = std::env::current_exe()?;
    let bundled = executable.parent().unwrap().join("assets");
    if bundled.join("world/manifest.json").is_file() {
        return Ok(bundled);
    }
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    anyhow::ensure!(
        development.join("world/manifest.json").is_file(),
        "World assets are missing. Extract the complete Windows release beside woodland.exe, \
         or run tools/fetch-assets.ps1 from the source checkout. BVL_ASSETS can override the location."
    );
    Ok(development)
}
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Menu,
    Walk,
    Fly,
}
struct Game {
    window: Window,
    renderer: Renderer,
    map: map::Map,
    grass: grass::Grass,
    view: view::View,
    camera: Vec3,
    yaw: f32,
    pitch: f32,
    keys: HashSet<KeyCode>,
    mode: Mode,
    last: Instant,
    start: Instant,
    title_time: Instant,
    frames: u32,
    capture: Option<PathBuf>,
    captured: bool,
    elapsed_ms: Vec<f32>,
    deferred_upload: Option<(bvl_engine::assets::MeshRange, grass::Page)>,
    auto_walk: bool,
    profile_seconds: f32,
    profile_enabled: bool,
    hour: f32,
    cycle: bool,
    wetness: f32,
    rain: bool,
    rtx: u32,
}
impl Game {
    fn new(event: &ActiveEventLoop) -> Result<Self> {
        let window = event.create_window(
            Window::default_attributes()
                .with_title("RustBakedVoxelLighting | loading Vulkan")
                .with_inner_size(PhysicalSize::new(1600, 900))
                .with_resizable(std::env::var_os("BVL_CAPTURE").is_none()),
        )?;
        let root = asset_root()?;
        println!("Assets: {}", root.display());
        let (map, scene) = map::load(&root)?;
        let shaders = Shaders {
            probe_update: include_bytes!(concat!(env!("OUT_DIR"), "/probe-update.comp.spv")),
            transport: include_bytes!(concat!(env!("OUT_DIR"), "/transport.comp.spv")),
            vertex: include_bytes!(concat!(env!("OUT_DIR"), "/world.vert.spv")),
            fragment: include_bytes!(concat!(env!("OUT_DIR"), "/world.frag.spv")),
            rt_fragment: include_bytes!(concat!(env!("OUT_DIR"), "/world-rt.frag.spv")),
            rt_water_fragment: include_bytes!(concat!(env!("OUT_DIR"), "/water-rt.frag.spv")),
            water_fragment: include_bytes!(concat!(env!("OUT_DIR"), "/water.frag.spv")),
            shadow: include_bytes!(concat!(env!("OUT_DIR"), "/shadow.frag.spv")),
            sky_vertex: include_bytes!(concat!(env!("OUT_DIR"), "/sky.vert.spv")),
            tonemap_fragment: include_bytes!(concat!(env!("OUT_DIR"), "/tonemap.frag.spv")),
            volume_fragment: include_bytes!(concat!(env!("OUT_DIR"), "/volume.frag.spv")),
            sky_fragment: include_bytes!(concat!(env!("OUT_DIR"), "/sky.frag.spv")),
        };
        let renderer = Renderer::new(&window, &scene, shaders)?;
        drop(scene);
        let grass = grass::Grass::new(renderer.stream_vertex_start, renderer.stream_index_start);
        let view = view::View::new(map.prototypes.len());
        let mut camera = Vec3::new(-40., map.field.height(-40., 65.) + 4., 65.);
        let mut yaw = 0.45;
        let mut pitch = -0.1;
        if std::env::var_os("BVL_RIVER_REVIEW").is_some()
            && let Some((center, _)) = map.rivers.iter().min_by(|a, b| {
                a.0.distance_squared(camera)
                    .total_cmp(&b.0.distance_squared(camera))
            })
        {
            let target = *center;
            camera = target + Vec3::new(12., 4., 18.);
            camera.y = camera.y.max(map.field.height(camera.x, camera.z) + 2.);
            let direction = (target - camera).normalize();
            yaw = direction.x.atan2(-direction.z);
            pitch = direction.y.asin();
        }
        let now = Instant::now();
        if std::env::var_os("BVL_GRASS_REVIEW").is_some() {
            camera = Vec3::new(-40., map.field.height(-40., 65.) + 1.7, 65.);
            yaw = 0.9;
            pitch = -0.35;
        }
        if std::env::var_os("BVL_LIGHT_REVIEW").is_some() {
            camera = Vec3::new(-40., map.field.height(-40., 65.) + 1.7, 65.);
            yaw = 2.5;
            pitch = 0.12;
        }
        let capture = std::env::var_os("BVL_CAPTURE").map(PathBuf::from);
        println!(
            "Ready: direct Vulkan. Enter = walk, F = fly, Esc = release mouse, WASD, Q/E altitude, Shift = fast. N = day/night, P = pause clock, [/] = hour, R = wet/dry weather. X = RTX mode (screen-space / ray reflections / reflections + shadows)."
        );
        Ok(Self {
            window,
            renderer,
            map,
            grass,
            view,
            camera,
            yaw,
            pitch,
            keys: HashSet::with_capacity(16),
            mode: Mode::Menu,
            last: now,
            start: now,
            title_time: now,
            frames: 0,
            capture,
            captured: false,
            elapsed_ms: Vec::with_capacity(20000),
            deferred_upload: None,
            auto_walk: std::env::var_os("BVL_AUTOWALK").is_some(),
            hour: std::env::var("BVL_HOUR")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10.85),
            cycle: true,
            wetness: std::env::var("BVL_WETNESS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.),
            rtx: std::env::var("BVL_RTX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1)
                .min(2),
            rain: std::env::var_os("BVL_WETNESS").is_some(),
            profile_enabled: std::env::var_os("BVL_PROFILE_SECONDS").is_some()
                || std::env::var_os("BVL_CAPTURE").is_some(),
            profile_seconds: std::env::var("BVL_PROFILE_SECONDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(23.),
        })
    }
    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if mode == Mode::Menu {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
            self.window.set_cursor_visible(true);
        } else {
            let capture = self
                .window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Confined));
            self.window.set_cursor_visible(capture.is_err());
        }
    }
    fn frame(&mut self) -> Result<bool> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f32();
        self.last = now;
        let dt = elapsed.min(0.05);
        let time = self.start.elapsed().as_secs_f32();
        if self.auto_walk && time > 5. {
            self.camera.x = -40. + (time - 5.) * 8.;
            self.camera.z = 65. - (time - 5.) * 25.;
            self.camera.y = self.map.field.height(self.camera.x, self.camera.z) + 4.;
        }
        let forward = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        );
        let right = Vec3::new(self.yaw.cos(), 0., self.yaw.sin());
        if self.mode != Mode::Menu {
            let direction = if self.mode == Mode::Fly {
                forward
            } else {
                Vec3::new(forward.x, 0., forward.z).normalize()
            };
            let mut motion = Vec3::ZERO;
            if self.keys.contains(&KeyCode::KeyW) {
                motion += direction;
            }
            if self.keys.contains(&KeyCode::KeyS) {
                motion -= direction;
            }
            if self.keys.contains(&KeyCode::KeyA) {
                motion -= right;
            }
            if self.keys.contains(&KeyCode::KeyD) {
                motion += right;
            }
            if self.mode == Mode::Fly {
                if self.keys.contains(&KeyCode::KeyE) {
                    motion.y += 1.;
                }
                if self.keys.contains(&KeyCode::KeyQ) {
                    motion.y -= 1.;
                }
            }
            let speed = if self.keys.contains(&KeyCode::ShiftLeft) {
                if self.mode == Mode::Fly { 65. } else { 8. }
            } else if self.mode == Mode::Fly {
                18.
            } else {
                3.8
            };
            self.camera += motion.normalize_or_zero() * dt * speed;
            self.camera.x = self.camera.x.clamp(-1990., 1990.);
            self.camera.z = self.camera.z.clamp(-1990., 1990.);
            let ground = self.map.field.height(self.camera.x, self.camera.z) + 1.75;
            if self.mode == Mode::Walk {
                self.camera.y = ground;
            } else {
                self.camera.y = self.camera.y.max(ground);
            }
        }
        if self.cycle {
            self.hour = (self.hour + dt / 60.).rem_euclid(24.);
        }
        let target_wet = if self.rain { 1. } else { 0. };
        self.wetness += (target_wet - self.wetness) * (1. - (-dt / 20.).exp());
        let hour = self.hour;
        let mut globals = self.view.prepare(
            &self.map,
            self.camera,
            forward,
            self.renderer.aspect(),
            time,
            hour,
        );
        globals.weather[2] = if self.renderer.supports_ray_queries() {
            self.rtx as f32
        } else {
            0.
        };
        globals.weather[3] = if self.renderer.supports_probe_gi() {
            1.
        } else {
            0.
        };
        globals.weather[0] = self.wetness;
        globals.weather[1] = if self.rain { 1. } else { 0. };
        if self.deferred_upload.is_none() {
            self.deferred_upload = self
                .grass
                .update(&self.map.field, self.camera, forward, time);
        }
        self.grass.append(
            self.camera,
            time,
            glam::Mat4::from_cols_array_2d(&globals.view_projection),
            self.map.grass,
            &mut self.view.instances,
            &mut self.view.draws,
        );
        let grass_shadow_start = self.view.shadows.len();
        self.grass.append_shadows(
            self.camera,
            time,
            self.map.grass,
            &mut self.view.instances,
            &mut self.view.shadows,
        );
        self.view
            .far_shadows
            .extend_from_slice(&self.view.shadows[grass_shadow_start..]);
        let capture = if time > 18. && !self.captured {
            self.capture.as_deref()
        } else {
            None
        };
        let uploaded = self.deferred_upload.as_ref().map(|(range, page)| Upload {
            range: *range,
            mesh: &page.mesh,
        });
        if !self.renderer.draw(
            &globals,
            &self.view.instances,
            DrawLists {
                opaque: &self.view.draws,
                shadows: &self.view.shadows,
                far_shadows: &self.view.far_shadows,
                water: &self.view.water,
            },
            uploaded,
            capture,
        )? {
            self.renderer.resize(&self.window)?;
        } else {
            self.deferred_upload = None;
        }
        if capture.is_some() {
            self.captured = true;
        }
        self.frames += 1;
        if time > 5. && self.elapsed_ms.len() < 20000 {
            self.elapsed_ms.push(elapsed * 1000.);
        }
        if self.title_time.elapsed().as_secs_f32() > 1. {
            let fps = self.frames as f32 / self.title_time.elapsed().as_secs_f32();
            self.window.set_title(&format!("RustBakedVoxelLighting | Vulkan | {fps:.0} FPS | {} draws | {} grass pages | Enter: Walk / F: Fly / Esc: Release",(self.view.draws.len()+self.view.water.len()),self.grass.count()));
            self.frames = 0;
            self.title_time = now;
        }
        if self.profile_enabled && time > self.profile_seconds {
            self.elapsed_ms.sort_by(f32::total_cmp);
            let n = self.elapsed_ms.len();
            if n > 0 {
                println!(
                    "{}x{} profile frame intervals: median {:.2} ms, p95 {:.2} ms, p99 {:.2} ms; {} draws, {} instances, {} grass pages",
                    self.window.inner_size().width,
                    self.window.inner_size().height,
                    self.elapsed_ms[n / 2],
                    self.elapsed_ms[n * 95 / 100],
                    self.elapsed_ms[n * 99 / 100],
                    (self.view.draws.len() + self.view.water.len()),
                    self.view.instances.len(),
                    self.grass.count()
                );
            }
            return Ok(false);
        }
        Ok(true)
    }
}
#[derive(Default)]
struct App {
    game: Option<Game>,
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event: &ActiveEventLoop) {
        if self.game.is_none() {
            match Game::new(event) {
                Ok(g) => self.game = Some(g),
                Err(e) => {
                    eprintln!("Startup failed: {e:#}");
                    event.exit();
                }
            }
        }
    }
    fn window_event(&mut self, event: &ActiveEventLoop, _id: WindowId, message: WindowEvent) {
        let Some(g) = self.game.as_mut() else {
            return;
        };
        match message {
            WindowEvent::CloseRequested => event.exit(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } if g.capture.is_none() && g.mode == Mode::Menu => g.set_mode(Mode::Fly),
            WindowEvent::Resized(size) => {
                if size.width > 0
                    && size.height > 0
                    && let Err(e) = g.renderer.resize(&g.window)
                {
                    eprintln!("Resize: {e:#}");
                    event.exit();
                }
            }
            WindowEvent::Focused(false) => {
                g.keys.clear();
                g.set_mode(Mode::Menu);
            }
            WindowEvent::KeyboardInput { event: key, .. } if g.capture.is_none() => {
                if let PhysicalKey::Code(code) = key.physical_key {
                    if key.state == ElementState::Pressed {
                        if g.mode == Mode::Menu
                            && matches!(
                                code,
                                KeyCode::KeyW
                                    | KeyCode::KeyA
                                    | KeyCode::KeyS
                                    | KeyCode::KeyD
                                    | KeyCode::KeyQ
                                    | KeyCode::KeyE
                            )
                        {
                            g.set_mode(Mode::Fly);
                        }
                        g.keys.insert(code);
                        if !key.repeat {
                            match code {
                                KeyCode::KeyX => {
                                    g.rtx = (g.rtx + 1) % 3;
                                    println!(
                                        "RTX mode {}: 0=screen-space, 1=ray reflections, 2=ray reflections and shadows",
                                        g.rtx
                                    );
                                }
                                KeyCode::KeyN => g.hour = (g.hour + 12.).rem_euclid(24.),
                                KeyCode::KeyP => g.cycle = !g.cycle,
                                KeyCode::KeyR => g.rain = !g.rain,
                                KeyCode::BracketLeft => g.hour = (g.hour - 1.).rem_euclid(24.),
                                KeyCode::BracketRight => g.hour = (g.hour + 1.).rem_euclid(24.),
                                KeyCode::Enter => g.set_mode(Mode::Walk),
                                KeyCode::KeyF => g.set_mode(Mode::Fly),
                                KeyCode::Escape => g.set_mode(Mode::Menu),
                                _ => {}
                            }
                        }
                    } else {
                        g.keys.remove(&code);
                    }
                }
            }
            WindowEvent::RedrawRequested
                if g.window.inner_size().width > 0 && g.window.inner_size().height > 0 =>
            {
                match g.frame() {
                    Ok(true) => {}
                    Ok(false) => event.exit(),
                    Err(e) => {
                        eprintln!("Render failed: {e:#}");
                        event.exit();
                    }
                }
            }
            _ => {}
        }
    }
    fn device_event(&mut self, _: &ActiveEventLoop, _: winit::event::DeviceId, event: DeviceEvent) {
        if let Some(g) = self.game.as_mut()
            && g.capture.is_none()
            && g.mode != Mode::Menu
            && let DeviceEvent::MouseMotion { delta } = event
        {
            g.yaw += delta.0 as f32 * 0.002;
            g.pitch = (g.pitch - delta.1 as f32 * 0.002).clamp(-1.52, 1.52);
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(g) = &self.game {
            g.window.request_redraw();
        }
    }
}
fn main() -> Result<()> {
    let event = EventLoop::new()?;
    event.set_control_flow(ControlFlow::Poll);
    event.run_app(&mut App::default())?;
    Ok(())
}
