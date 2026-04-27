# build.ps1
# Produces two distributions for open_herd:
#   - Full installer : NSIS setup exe, installs to Program Files, data in ~/.phpenv
#   - Portable ZIP   : extract anywhere and run, data stored in data/ next to the exe

$ErrorActionPreference = "Stop"
$root      = $PSScriptRoot
$version   = "0.1.0"   # keep in sync with tauri.conf.json
$triple    = "x86_64-pc-windows-msvc"
$appExe    = "phpenv-gui.exe"
$distDir   = Join-Path $root "dist"

Write-Host "============================================" -ForegroundColor Cyan
Write-Host "  open_herd build — installer + portable"    -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan

# ── 1. Icons ──────────────────────────────────────────────────────────────────
Write-Host "`n[1/5] Generating icons from logo.png..." -ForegroundColor Yellow
$logoSrc = Join-Path $root "logo.png"
if (-not (Test-Path $logoSrc)) {
    Write-Host "ERROR: logo.png not found at project root." -ForegroundColor Red
    exit 1
}
Set-Location (Join-Path $root "gui")
npx tauri icon "..\logo.png"

# ── 2. Build Go daemon sidecar ────────────────────────────────────────────────
Write-Host "`n[2/5] Building Go daemon (phpenv-daemon)..." -ForegroundColor Yellow
Set-Location (Join-Path $root "daemon")
$sidecarDest = Join-Path $root "gui\src-tauri\phpenv-daemon-$triple.exe"
go build -ldflags="-H windowsgui" -o $sidecarDest .
if (-not (Test-Path $sidecarDest)) {
    Write-Host "ERROR: Go build failed." -ForegroundColor Red
    exit 1
}

# ── 3. Clean stale Tauri build cache ─────────────────────────────────────────
Write-Host "`n[3/5] Cleaning stale build cache..." -ForegroundColor Yellow
$stale = Join-Path $root "gui\src-tauri\target\release\build\phpenv-gui-*"
if (Test-Path $stale) { Remove-Item -Path $stale -Recurse -Force -ErrorAction SilentlyContinue }

# ── 4. Frontend deps + Tauri build ───────────────────────────────────────────
Write-Host "`n[4/5] Building frontend + Tauri app..." -ForegroundColor Yellow
Set-Location (Join-Path $root "gui")
npm install
npm run tauri build

# ── 5. Package outputs ────────────────────────────────────────────────────────
Write-Host "`n[5/5] Packaging outputs..." -ForegroundColor Yellow

$releaseDir  = Join-Path $root "gui\src-tauri\target\release"
$builtExe    = Join-Path $releaseDir $appExe
$nsisSetup   = Join-Path $releaseDir "bundle\nsis\phpenv_${version}_x64-setup.exe"

if (-not (Test-Path $builtExe)) {
    Write-Host "ERROR: $appExe not found at $builtExe" -ForegroundColor Red
    exit 1
}

# Create dist/ output folder
if (Test-Path $distDir) { Remove-Item $distDir -Recurse -Force }
New-Item -ItemType Directory -Path $distDir | Out-Null

# ── Full installer ────────────────────────────────────────────────────────────
if (Test-Path $nsisSetup) {
    $installerOut = Join-Path $distDir "open-herd-v$version-setup.exe"
    Copy-Item $nsisSetup $installerOut
    Write-Host "  Installer : $installerOut" -ForegroundColor Green
} else {
    Write-Host "  WARNING: NSIS installer not found, skipping." -ForegroundColor DarkYellow
}

# ── Portable ZIP ──────────────────────────────────────────────────────────────
$portableStage = Join-Path $distDir "portable"
New-Item -ItemType Directory -Path $portableStage | Out-Null
New-Item -ItemType Directory -Path (Join-Path $portableStage "data") | Out-Null

# Copy main exe
Copy-Item $builtExe (Join-Path $portableStage "open-herd.exe")

# Seed data/config.json — empty object is enough; the daemon fills in defaults.
# Its presence is what signals "portable mode" to both the GUI and the daemon.
Set-Content -Path (Join-Path $portableStage "data\config.json") -Value "{}" -Encoding utf8

# Compress
$portableZip = Join-Path $distDir "open-herd-v$version-portable.zip"
Compress-Archive -Path "$portableStage\*" -DestinationPath $portableZip
Remove-Item $portableStage -Recurse -Force

Write-Host "  Portable  : $portableZip" -ForegroundColor Green

# ── Summary ───────────────────────────────────────────────────────────────────
Write-Host "`n============================================" -ForegroundColor Cyan
Write-Host "  Done — outputs in dist\"                   -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Full install : open-herd-v$version-setup.exe"   -ForegroundColor White
Write-Host "    Data stored in  : ~/.phpenv"                   -ForegroundColor DarkGray
Write-Host ""
Write-Host "  Portable    : open-herd-v$version-portable.zip" -ForegroundColor White
Write-Host "    Data stored in  : <carpeta-extraida>/data/"    -ForegroundColor DarkGray
