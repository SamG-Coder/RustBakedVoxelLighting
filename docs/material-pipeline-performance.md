# Material pipeline optimization - 31 October 2026

Target: native Rust/Vulkan renderer; RTX 5080, 1600 x 900, 4x MSAA.
This comparison starts from the local grass seating/shadow stability fixes,
not the older public v0.1.0 build.

## Changes

- Specialize the existing vertex/fragment shader source by material kind at
  pipeline creation. Five surface variants and two cutout shadow variants
  remove irrelevant branches; texture bindings, shader formulas and render
  states remain shared. Unknown material kinds retain the dynamic fallback.
- Draw solid terrain/bark before alpha-tested vegetation and sort tree
  instances front-to-back within each batch. No geometry or draws removed.
- Reject grass tufts in the shadow vertex pass when the existing per-tuft
  fade would discard every fragment. The fragment check remains as a guard.
- Water keeps its own pipeline and is not routed through surface variants.
- Pipelines are created at startup and destroyed with the renderer. Sorting
  uses existing buffers without an additional per-frame allocation.

## Measurements

Static forest view, real-time wind/day cycle, each run 25 seconds, frame
statistics exclude the first five seconds. GPU timestamps enabled.
Environment: BVL_GRASS_REVIEW=1, BVL_HOUR=10, BVL_GPU_PROFILE=1,
BVL_PROFILE_SECONDS=25. No validation layer in these timing runs.

| Build | Median ms | p95 ms | p99 ms |
| --- | ---: | ---: | ---: |
| Before | 12.63 | 13.07 | 13.36 |
| Draw order only | 12.16 | 12.62 | 13.02 |
| Final | 10.99 | 11.40 | 11.71 |

Both before/final end with 986 scene draws, 12,807 instance records, and
574 resident grass pages. Main scene GPU samples fell from roughly
6.5-7.2 ms to 5.0-5.6 ms. Shadow cost remains about 5.3-5.9 ms and is the
largest remaining pass. The GI update is normally around 0.1 ms.

A separate 30-second auto-walk with Vulkan and synchronization validation
and a screenshot completed with no reported validation errors: median
10.60 ms, p95 12.52 ms, p99 13.16 ms. It ends with 727 draws and 655 grass
pages; it is not the same workload as the static comparison. The screenshot
was reviewed for terrain, vegetation and water rendering.

All 12 workspace tests, Clippy with warnings denied, release build and
`git diff --check` pass. Logs and screenshot are under ignored `evidence/`:
`perf-before.log`, `perf-final.log`, `perf-quality-walk.log`, and
`perf-quality-walk.png`. Benchmarks are bounded local runs, not guarantees
for every viewpoint, display resolution, or long-run streaming condition.

A final repeat of the preserved before executable measured 12.59 ms median,
13.01 ms p95 and 13.42 ms p99, with the same draw/instance/page counts. This
supports the improvement being repeatable rather than a one-run baseline anomaly.

## Follow-up: continuous sun shadows

The subsequent shadow pass replaces hard texel rounding with smooth interpolation
between adjacent projection anchors and uses a translated 5x5 tent PCF kernel
packed into nine hardware-linear lookups. This keeps the sun moving continuously
and softens shadow edges without blurring surface textures. Tests cover anchor
continuity and the packed filter weights. The final suite has 14 passing tests;
Clippy and release build pass. A 24-second day-cycle run with Vulkan/synchronization
validation reported no errors and measured 11.20 ms median, 12.03 ms p95,
12.70 ms p99. Evidence: `soft-sun-shadows.log` and `soft-sun-shadows.png`.
This is a separate validation run, not a direct replacement for the performance
comparison above; a still image cannot establish the absence of all motion flicker.
