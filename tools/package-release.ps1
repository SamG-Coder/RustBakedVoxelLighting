param([string]$Version = 'v0.1.0')
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?$') { throw 'Invalid release version' }
$repo = Split-Path $PSScriptRoot -Parent
Push-Location $repo
try {
    if (-not (Test-Path 'assets/world/manifest.json')) { throw 'Download or export assets first.' }
    cargo build --release --locked -p woodland
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $stage = Join-Path $repo "dist/stage-$Version"
    if (Test-Path $stage) { throw "Packaging directory already exists: $stage. Use a fresh version or rename it first." }
    $bundle = Join-Path $stage 'RustBakedVoxelLighting'
    New-Item -ItemType Directory -Path $bundle -Force | Out-Null
    Copy-Item 'target/release/woodland.exe' $bundle
    Copy-Item 'assets' $bundle -Recurse
    Copy-Item 'LICENSE','README.md','THIRD_PARTY_NOTICES.md' $bundle
    New-Item -ItemType Directory -Path "$bundle/docs" | Out-Null
    Copy-Item 'docs/asset-provenance.md','docs/asset-prompts.md' "$bundle/docs"
    Copy-Item 'docs/images' "$bundle/docs" -Recurse
    python tools/collect-licenses.py "$bundle/THIRD_PARTY_LICENSES"
    if ($LASTEXITCODE -ne 0) { throw 'Dependency notice collection failed' }
    @'
RustBakedVoxelLighting - Native Vulkan preview

Extract the entire archive, then open woodland.exe. Assets must stay beside it.
Requires 64-bit Windows, a compatible Vulkan 1.3 GPU/driver, and Microsoft Visual
C++ x64 runtime (https://aka.ms/vs/17/release/vc_redist.x64.exe).
No Rust compiler or Vulkan SDK is required to run this build.

Enter: walk | F: fly | Esc: release mouse
WASD: move | Mouse: look | Q/E: fly down/up | Shift: faster
N: switch day/night | P: pause day cycle | [ / ]: adjust time
R: wet/dry | X: reflection/shadow mode

Startup loads the world and prepares GPU resources. Please allow several seconds.
This is a development preview, tested on an RTX 5080. DLSS and full browser
gameplay parity are not implemented. See README.md for scope and limitations.
'@ | Set-Content "$bundle/START-HERE.txt" -Encoding utf8
    $archive = Join-Path $repo "dist/RustBakedVoxelLighting-$Version-windows-x64.zip"
    if (Test-Path $archive) { throw "Archive already exists: $archive" }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $archive, [IO.Compression.CompressionLevel]::Optimal, $false)
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($archive))" | Set-Content 'dist/SHA256SUMS.txt' -Encoding ascii
    Get-Item $archive | Select-Object FullName,Length
} finally { Pop-Location }
