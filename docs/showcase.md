# Matched browser/native fly-through

The shared Rust camera sampler in `crates/woodland/src/showcase.rs` controls position, view direction, terrain clearance, animation time and sun hour. The route lasts 120 seconds after a 30-second warm-up. It covers woodland, the river channel, canopy and evening light.

Browser: choose **Record comparison fly-through**. Records the real 1600 x 900 canvas at up to 60 frames/s using MediaRecorder. Download when finished. Esc returns to normal use after recording. Recording speed follows wall time; it is not offline frame rendering.

Native: build release, then run `tools/record-native.ps1`. Requires FFmpeg with NVENC on this recording machine. It launches the actual Vulkan renderer, captures the client area via gdigrab, and logs the capture packet timestamps. `evidence/native-ready.txt` holds the wall-clock epoch of camera route time zero. Trim using that epoch minus the first captured packet timestamp; do not assume process launch is the first captured frame. Native RTX reflections and probe GI remain enabled where supported, while the browser uses its WebGPU fallbacks. This is a visual comparison, not a controlled FPS benchmark.

Both captures use the same exported world, camera FOV, 1600 x 900 render resolution, dry weather and sun clock. No color correction is applied to hide differences. Browser and native rendering are not claimed to have full feature parity.

## Exported comparison (31 October 2026)

- Both source captures are 1600 x 900, live wall-clock playback. Browser MediaRecorder requests 60 fps; the native capture is variable-rate up to 60 fps. The edit presents them on a common 60 fps timeline, repeating frames when a source does not supply one. No frame interpolation or colour correction.
- Native warm-up offset: 29.933333 seconds. Checked at browser times 12, 55 and 95 seconds using SIFT feature correspondences on 800 x 450 frames (median positional errors 0.72, 0.58 and 0.56 pixels at the nearest tested offsets). This is visual alignment, not a claim of hardware-synchronised capture.
- `tools/align-comparison.py` reproduces the correspondence check (requires OpenCV and NumPy). `tools/edit-comparison.py` creates the 1920 x 1080 social edit and 3200 x 1020 full-resolution side-by-side master (requires Pillow and FFmpeg/NVENC).
- Final footage is approximately two minutes (119.8 seconds). No added audio. The native capture has RTX reflection/probe features enabled; browser fallbacks differ intentionally. It is a visual comparison, not an FPS benchmark.
