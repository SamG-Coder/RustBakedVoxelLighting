param([switch]$SkipAssets)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
function Check-Exit { if ($LASTEXITCODE -ne 0) { throw "Build step failed ($LASTEXITCODE)" } }
rustup target add wasm32-unknown-unknown
Check-Exit
if (!(Get-Command wasm-bindgen -ErrorAction SilentlyContinue) -or ((wasm-bindgen --version) -ne 'wasm-bindgen 0.2.105')) {
 cargo install wasm-bindgen-cli --locked --version 0.2.105
 Check-Exit
}
if (!$SkipAssets) { cargo run --release -p woodland --example export_web; Check-Exit }
python tools/package-web.py
Check-Exit
cargo build --release -p woodland-browser --target wasm32-unknown-unknown
Check-Exit
wasm-bindgen --target web --out-dir web/pkg target/wasm32-unknown-unknown/release/woodland_browser.wasm
Check-Exit
Write-Host 'Serve web/ over localhost or HTTPS. Example: python -m http.server 5180 --directory web'
