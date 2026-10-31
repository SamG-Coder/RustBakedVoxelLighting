# Direct Vulkan bring-up evidence — 31 October 2026

Hardware: NVIDIA GeForce RTX 5080. Driver reports Vulkan 1.4.351; application requests Vulkan 1.3. Rendering resolution: 1600 × 900, 4× MSAA. Active dependency tree uses ash 0.38 and winit 0.30, with no Bevy/wgpu dependency.

The source world contains 145,331 trees and 11,717 shrubs over 16 km². Static shared geometry contains 2,806,556 vertices and 10,134,909 indices. Twenty-two textures and 39 material records are uploaded. Nearby plant representations are instanced; distant cards are grouped into geographic pages. These counts do not imply that all source geometry is drawn each frame.

## Release runtime check

`vulkan-release-run.log` records a 35-second scripted traversal with native validation and synchronization validation enabled. After five seconds the camera travels at approximately 26 metres/second across the real height field, while grass generation/upload/cancellation continues. Screenshot capture occurs at 18 seconds.

| Frame-interval statistic | Result |
| --- | ---: |
| Median | 8.80 ms |
| 95th percentile | 10.27 ms |
| 99th percentile | 10.85 ms |

The median is approximately 114 frames/second. The end of that traversal had 1,061 scene draws, 6,745 instance records, and 99 resident grass pages. No Vulkan validation errors or synchronization hazards were reported in this run. Earlier bring-up runs found a missing shader-demote feature enable and an acquire/layout-transition dependency; both were fixed before this release run.

These are frame intervals from one bounded traversal, not GPU timestamps, exhaustive world coverage, an apples-to-apples Bevy benchmark, or proof of long-run memory stability. PNG capture performs synchronous readback and encoding. Window compositing and other applications can affect frame intervals.

## Code checks

- `cargo test --workspace`: six tests pass, covering invalid mesh data, geographic sampling, bounded/cancelled jobs, worker panic recovery, grass index/capacity/determinism, and river exclusion.
- `cargo clippy --workspace --all-targets -- -D warnings`: passes.
- `cargo build --release --workspace`: passes; GLSL compiles to SPIR-V for Vulkan 1.3.

## Visual scope

Native screenshot: `evidence/vulkan-release.png`. Trees, shrubs, terrain, river surface, and generated grass are present. Grass colour/density and water still differ from the browser renderer; full native voxel GI and advanced water are outstanding. See the README parity section. The initial Bevy experiment is retained outside the active Cargo workspace for reference.
