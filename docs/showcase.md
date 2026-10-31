# Matched browser/native fly-through

The shared Rust camera sampler in `crates/woodland/src/showcase.rs` controls position, view direction, terrain clearance, animation time and sun hour. The route lasts 120 seconds after a 30-second warm-up. It covers woodland, the river channel, canopy and evening light.

Browser: choose **Record comparison fly-through**. Records the real 1600 x 900 canvas at up to 60 frames/s using MediaRecorder. Download when finished. Esc returns to normal use after recording. Recording speed follows wall time; it is not offline frame rendering.

Native: build release, then run `tools/record-native.ps1`. Requires FFmpeg with NVENC on this recording machine. It launches the actual Vulkan renderer, captures the client area via gdigrab, and logs the capture packet timestamps. `evidence/native-ready.txt` holds the wall-clock epoch of camera route time zero. Trim using that epoch minus the first captured packet timestamp; do not assume process launch is the first captured frame. Native RTX reflections and probe GI remain enabled where supported, while the browser uses its WebGPU fallbacks. This is a visual comparison, not a controlled FPS benchmark.

Both captures use the same exported world, camera FOV, 1600 x 900 render resolution, dry weather and sun clock. No color correction is applied to hide differences. Browser and native rendering are not claimed to have full feature parity.
