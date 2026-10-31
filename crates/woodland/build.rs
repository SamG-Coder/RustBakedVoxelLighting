use std::{env, path::PathBuf, process::Command};
fn main() {
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    for name in [
        "transport.comp",
        "probe-update.comp",
        "world.vert",
        "world.frag",
        "water.frag",
        "world-rt.frag",
        "water-rt.frag",
        "shadow.frag",
        "sky.vert",
        "sky.frag",
        "tonemap.frag",
        "volume.frag",
    ] {
        let source = PathBuf::from("shaders").join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        let status = Command::new("glslc")
            .arg("--target-env=vulkan1.3")
            .arg("-O")
            .arg(&source)
            .arg("-o")
            .arg(output.join(format!("{name}.spv")))
            .status()
            .expect("Install Vulkan SDK and put glslc on PATH");
        assert!(status.success(), "GLSL compilation failed: {name}");
    }
    println!("cargo:rerun-if-changed=shaders/common.glsl");
    println!("cargo:rerun-if-changed=shaders/shadow-sampling.glsl");
    println!("cargo:rerun-if-changed=shaders/world-body.glsl");
    println!("cargo:rerun-if-changed=shaders/reflections.glsl");
    println!("cargo:rerun-if-changed=shaders/ray-query.glsl");
    println!("cargo:rerun-if-changed=shaders/lighting.glsl");
    println!("cargo:rerun-if-changed=shaders/atmosphere.glsl");
    println!("cargo:rerun-if-changed=shaders/surface.glsl");
    println!("cargo:rerun-if-changed=shaders/probe-cache.glsl");
}
