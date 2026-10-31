param([string]$Version = 'v0.1.0')
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?$') { throw 'Invalid release version' }
$repo = Split-Path $PSScriptRoot -Parent
$destination = Join-Path $repo 'assets'
if (Test-Path $destination) {
    if (Test-Path "$destination/world/manifest.json") { Write-Host 'Existing assets retained.'; exit 0 }
    throw 'An incomplete assets directory exists. Move it aside before downloading.'
}
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('bvl-assets-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
$name = "RustBakedVoxelLighting-$Version-windows-x64.zip"
$base = "https://github.com/SamG-Coder/RustBakedVoxelLighting/releases/download/$Version"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
try {
    Write-Host "Downloading $Version world assets..."
    Invoke-WebRequest "$base/SHA256SUMS.txt" -OutFile "$temporary/SHA256SUMS.txt" -UseBasicParsing
    Invoke-WebRequest "$base/$name" -OutFile "$temporary/$name" -UseBasicParsing
    $expected = Get-Content "$temporary/SHA256SUMS.txt" | Where-Object { $_ -match ('^[a-fA-F0-9]{64}\s+' + [regex]::Escape($name) + '$') }
    if (@($expected).Count -ne 1) { throw 'Missing or ambiguous archive checksum' }
    $hash = (Get-FileHash "$temporary/$name" -Algorithm SHA256).Hash
    if ($hash -ne ($expected -split '\s+')[0]) { throw 'Release archive checksum mismatch' }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::ExtractToDirectory("$temporary/$name", "$temporary/extracted")
    $assets = "$temporary/extracted/RustBakedVoxelLighting/assets"
    if (-not (Test-Path "$assets/world/manifest.json")) { throw 'Archive does not contain the world assets' }
    Move-Item -LiteralPath $assets -Destination $destination
    Write-Host 'Verified assets installed. Run cargo run --release -p woodland.'
} finally {
    # The deletion target is a unique child of the system temp directory, never the repository.
    $resolved = [IO.Path]::GetFullPath($temporary)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and
        ([IO.Path]::GetFileName($resolved) -like 'bvl-assets-*')) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
