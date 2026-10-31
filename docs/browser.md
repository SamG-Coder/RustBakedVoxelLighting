# Rust / WebAssembly browser build

The browser executes the shared Rust world, camera, visibility/LOD and grass implementation. `bvl-engine` selects its WebGPU backend with `cfg(target_arch = "wasm32")`; desktop retains direct Vulkan. There is no Three.js dependency. JavaScript handles DOM/input, decompression and animation-frame scheduling.

## Build and run

Run `tools/build-web.ps1` on Windows, then `python -m http.server 5180 --directory web`. The first build needs native source assets (`tools/fetch-assets.ps1`). `-SkipAssets` reuses an already exported world. wasm-bindgen 0.2.105 matches the pinned bindings dependency.

The Pages workflow builds WASM from source, downloads a checksum-pinned world export, and deploys `web/`. It deliberately does not run tests in Actions. `tools/package-web.py --archive` produces the losslessly compressed versioned world pack. Browser DecompressionStream handles .bin.gz independently of server Content-Encoding headers. All URLs are relative and work under the repository Pages path.

## Rendering implemented

- Original terrain, tree/bush prototypes, four tree LODs, grass species and coverage mipmaps.
- Full-map terrain-relative directional voxel transport, baked by a sliced WebGPU compute dispatch; sun and moon relight the coefficients without rebaking.
- Filtered near/far shadow maps, grass casters, leaf light transmission and normal maps.
- Native atmosphere/cloud density and cloud-shadow projection, sun/moon and 24-minute day/night cycle.
- Linear RGBA16F HDR, 4x MSAA/foliage alpha-to-coverage, anisotropic mip filtering and a single tone-map/sRGB display conversion.
- Native terrain texture variation, biome/litter/road/riverbank blending and distant grass colour merging.
- Flowing river normal maps, screen-space reflection with atmosphere fallback, depth-dependent absorption/refraction.
- Half-resolution local volumetric light integration with depth-aware upsampling.

## Explicit limits

Browser WebGPU has no direct Vulkan/RTX ray-query or NVIDIA DLSS path here. The native ray-traced probe updates, off-screen ray reflections and ray contact shadows remain native-only. Browser reflections fall back to the atmosphere outside the screen. These are not full native RTX parity.

The compressed initial world download is about 423 MiB and expands to about 901 MiB; it is a desktop preview, not an optimized streaming download. Grass uses the same bounded page generation, but the current WASM job backend executes one cooperative job at a time rather than using threaded workers. Lighting transfer is recomputed on launch rather than persisted in IndexedDB. GPU pages remain bounded. No guarantee of 60 FPS on other hardware is implied.

## Validation

Native workspace tests and WASM clippy/build were run locally. Browser shader validation, real rendering and interaction were checked on an NVIDIA Blackwell adapter. The sRGB canvas view is explicitly declared: browser canvas formats normally report UNORM, so assuming automatic encoding produces incorrectly dark midtones.
