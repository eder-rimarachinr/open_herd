#Requires -Version 5.1
# build-portable.ps1
# Genera dos distribuciones de Open Herd:
#   - Instalador completo : NSIS setup.exe  (datos en %USERPROFILE%\.phpenv)
#   - ZIP portable        : extraer y ejecutar (datos en ./data/ junto al exe)
#
# Uso (desde la raiz del repo):
#   .\scripts\build-portable.ps1
#   .\scripts\build-portable.ps1 -SkipIcons        # salta la regeneracion de iconos
#   .\scripts\build-portable.ps1 -SkipFrontend     # reutiliza el build de npm anterior
#
# Salida en release/ (dist/ es la salida de Vite).

param(
    [switch]$SkipIcons,
    [switch]$SkipFrontend,
    [switch]$Help
)

if ($Help) {
    Write-Host "build-portable.ps1 -- Construye el instalador y ZIP portable de Open Herd"
    Write-Host ""
    Write-Host "  -SkipIcons      No regenera iconos desde assets\logo.png"
    Write-Host "  -SkipFrontend   Reutiliza el bundle de npm/Vite anterior"
    Write-Host "  -Help           Muestra esta ayuda"
    exit 0
}

$ErrorActionPreference = "Stop"

$root    = Split-Path $PSScriptRoot -Parent
$distDir = Join-Path $root "release"

# -- Leer version desde Cargo.toml (unica fuente de verdad) -------------------
$cargoToml = Get-Content (Join-Path $root "src-tauri\Cargo.toml") -Raw
if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
    $version = $Matches[1]
} else {
    Write-Host "ERROR: No se pudo leer la version de Cargo.toml" -ForegroundColor Red
    exit 1
}

$releaseDir = Join-Path $root "src-tauri\target\release"
$appExe     = "open-herd.exe"

Write-Host ""
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "  Open Herd -- build v$version" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host ""

# -- Pre-flight checks --------------------------------------------------------
Write-Host "[preflight] Verificando herramientas..." -ForegroundColor Yellow

foreach ($cmd in @("node", "npm", "cargo", "rustc")) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        Write-Host "ERROR: '$cmd' no encontrado en PATH." -ForegroundColor Red
        exit 1
    }
}

$logoSrc = Join-Path $root "assets\logo.png"
if (-not (Test-Path $logoSrc)) {
    Write-Host "ERROR: assets\logo.png no encontrado." -ForegroundColor Red
    Write-Host "       Coloca un PNG cuadrado (1024x1024 recomendado) como assets\logo.png" -ForegroundColor Yellow
    exit 1
}

Write-Host "  node    $(node --version)" -ForegroundColor DarkGray
Write-Host "  npm     $(npm --version)" -ForegroundColor DarkGray
Write-Host "  cargo   $(cargo --version)" -ForegroundColor DarkGray
Write-Host "  version $version" -ForegroundColor DarkGray
Write-Host ""

# -- 1. Iconos ----------------------------------------------------------------
if (-not $SkipIcons) {
    Write-Host "[1/4] Generando iconos desde assets\logo.png..." -ForegroundColor Yellow
    Set-Location $root
    npx tauri icon "assets\logo.png"
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: tauri icon fallo." -ForegroundColor Red; exit 1
    }
    Write-Host "  Iconos generados." -ForegroundColor Green
} else {
    Write-Host "[1/4] Iconos omitidos (-SkipIcons)." -ForegroundColor DarkGray
}

# -- 2. Limpiar cache ---------------------------------------------------------
Write-Host "[2/4] Limpiando cache de build..." -ForegroundColor Yellow
$stalePattern = Join-Path $root "src-tauri\target\release\build\open-herd-*"
Get-Item $stalePattern -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force
Write-Host "  Cache limpio." -ForegroundColor Green

# -- 3. Build frontend + Tauri ------------------------------------------------
Write-Host "[3/4] Compilando frontend + Tauri..." -ForegroundColor Yellow
Set-Location $root

if (-not $SkipFrontend) {
    Write-Host "  npm install..." -ForegroundColor DarkGray
    npm install
    if ($LASTEXITCODE -ne 0) { Write-Host "ERROR: npm install fallo." -ForegroundColor Red; exit 1 }
}

Write-Host "  npm run tauri build..." -ForegroundColor DarkGray
npm run tauri build
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Tauri build fallo." -ForegroundColor Red; exit 1
}
Write-Host "  Build completado." -ForegroundColor Green

# -- 4. Empaquetar ------------------------------------------------------------
Write-Host "[4/4] Empaquetando distribuciones..." -ForegroundColor Yellow

$builtExe = Join-Path $releaseDir $appExe
if (-not (Test-Path $builtExe)) {
    Write-Host "ERROR: $appExe no encontrado en $builtExe" -ForegroundColor Red
    exit 1
}

if (Test-Path $distDir) { Remove-Item $distDir -Recurse -Force }
New-Item -ItemType Directory -Path $distDir | Out-Null

# -- Instalador NSIS ----------------------------------------------------------
# Tauri genera: "{productName}_{version}_x64-setup.exe"
# productName "Open Herd" -> "Open Herd_0.1.0_x64-setup.exe"
$nsisDir   = Join-Path $releaseDir "bundle\nsis"
$nsisSetup = Get-ChildItem $nsisDir -Filter "*setup.exe" -ErrorAction SilentlyContinue |
             Select-Object -First 1

if ($nsisSetup) {
    $installerOut = Join-Path $distDir "open-herd-v$version-setup.exe"
    Copy-Item $nsisSetup.FullName $installerOut
    $sizeMB = [math]::Round((Get-Item $installerOut).Length / 1MB, 1)
    Write-Host "  Instalador : open-herd-v$version-setup.exe  ($sizeMB MB)" -ForegroundColor Green
} else {
    Write-Host "  AVISO: instalador NSIS no encontrado en $nsisDir -- omitiendo." -ForegroundColor DarkYellow
    Write-Host "         Asegurate de que NSIS este instalado en Windows." -ForegroundColor DarkYellow
}

# -- Instalador MSI (WiX) ------------------------------------------------------
# Tauri genera: "{productName}_{version}_x64_en-US.msi"
$msiDir = Join-Path $releaseDir "bundle\msi"
$msi    = Get-ChildItem $msiDir -Filter "*.msi" -ErrorAction SilentlyContinue |
          Select-Object -First 1

if ($msi) {
    $msiOut = Join-Path $distDir "open-herd-v$version-x64.msi"
    Copy-Item $msi.FullName $msiOut
    $msiSizeMB = [math]::Round((Get-Item $msiOut).Length / 1MB, 1)
    Write-Host "  MSI        : open-herd-v$version-x64.msi  ($msiSizeMB MB)" -ForegroundColor Green
} else {
    Write-Host "  AVISO: instalador MSI no encontrado en $msiDir -- omitiendo." -ForegroundColor DarkYellow
}

# -- ZIP portable -------------------------------------------------------------
$portableStage = Join-Path $distDir "_portable_stage"
New-Item -ItemType Directory -Path $portableStage | Out-Null
New-Item -ItemType Directory -Path (Join-Path $portableStage "data") | Out-Null

Copy-Item $builtExe (Join-Path $portableStage "open-herd.exe")

# data/config.json vacio -> activa modo portable (datos en ./data/ en vez de ~/.phpenv)
Set-Content -Path (Join-Path $portableStage "data\config.json") -Value "{}" -Encoding utf8

# README para el ZIP
$readmeContent  = "Open Herd v$version - Modo Portable`r`n"
$readmeContent += "=====================================`r`n"
$readmeContent += "Ejecuta open-herd.exe desde esta carpeta.`r`n"
$readmeContent += "Los datos se guardan en la carpeta data/ junto al exe.`r`n"
$readmeContent += "`r`n"
$readmeContent += "Para instalar como app normal usa el instalador open-herd-v$version-setup.exe.`r`n"
Set-Content -Path (Join-Path $portableStage "README.txt") -Value $readmeContent -Encoding utf8

# Comprimir
$portableZip = Join-Path $distDir "open-herd-v$version-portable.zip"
Compress-Archive -Path "$portableStage\*" -DestinationPath $portableZip -Force
Remove-Item $portableStage -Recurse -Force

$zipSizeMB = [math]::Round((Get-Item $portableZip).Length / 1MB, 1)
Write-Host "  Portable   : open-herd-v$version-portable.zip  ($zipSizeMB MB)" -ForegroundColor Green

# -- Resumen ------------------------------------------------------------------
Write-Host ""
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "  Listo -- archivos en release/" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Instalador: datos en %USERPROFILE%\.phpenv" -ForegroundColor White
Write-Host "  Portable:   datos en ./data/ junto al exe" -ForegroundColor White
Write-Host ""
