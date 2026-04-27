# build-portable.ps1 - Compila open_herd en una aplicación nativa usando Tauri

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host "==============================================" -ForegroundColor Cyan
Write-Host " Compilador Portable (Tauri) para open_herd" -ForegroundColor Cyan
Write-Host "==============================================" -ForegroundColor Cyan

# 1. Crear el icono vacío requerido por Tauri en Windows
Write-Host "`n[1/4] Generando icono temporal para Tauri..." -ForegroundColor Yellow
$iconsDir = Join-Path $root "gui\src-tauri\icons"
if (-not (Test-Path $iconsDir)) {
    New-Item -ItemType Directory -Path $iconsDir | Out-Null
}
$iconPath = Join-Path $iconsDir "icon.ico"
if (-not (Test-Path $iconPath)) {
    $base64 = "AAABAAEAAQEAAAEAIAAwAAAAFgAAACgAAAABAAAAAgAAAAEAIAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAwAAAAA=="
    [IO.File]::WriteAllBytes($iconPath, [Convert]::FromBase64String($base64))
}

# 2. Compilar el daemon de Go
Write-Host "`n[2/4] Compilando el motor de Go (phpenv-daemon)..." -ForegroundColor Yellow
Set-Location (Join-Path $root "daemon")
# Usamos -ldflags="-H windowsgui" para que el motor no abra una ventana negra de consola al ejecutarse
go build -ldflags="-H windowsgui" -o (Join-Path $root "gui\src-tauri\phpenv-daemon.exe") .

if (-not (Test-Path (Join-Path $root "gui\src-tauri\phpenv-daemon.exe"))) {
    Write-Host "ERROR: No se pudo compilar el ejecutable de Go." -ForegroundColor Red
    exit 1
}

# 3. Limpiar cache corrupta del compilador de Windows (RC.exe)
Write-Host "`n[3/5] Limpiando cache corrupta de compilacion..." -ForegroundColor Yellow
$buildCache = Join-Path $root "gui\src-tauri\target\release\build\phpenv-gui-*"
if (Test-Path $buildCache) {
    Remove-Item -Path $buildCache -Recurse -Force -ErrorAction SilentlyContinue
}

# 4. Instalar dependencias del frontend si es necesario
Write-Host "`n[4/5] Verificando dependencias del Frontend (npm install)..." -ForegroundColor Yellow
Set-Location (Join-Path $root "gui")
npm install

# 5. Construir la app nativa con Tauri
Write-Host "`n[5/5] Empaquetando la aplicacion nativa con Tauri..." -ForegroundColor Yellow
npm run tauri build

$targetPath = Join-Path $root "gui\src-tauri\target\release\phpenv.exe"
$setupPath = Join-Path $root "gui\src-tauri\target\release\bundle\nsis\phpenv_0.1.0_x64-setup.exe"

Write-Host "`n==============================================" -ForegroundColor Green
Write-Host " COMPILACION EXITOSA!" -ForegroundColor Green
Write-Host "==============================================" -ForegroundColor Green

if (Test-Path $targetPath) {
    Write-Host "`nEjecutable Portable listo en:" -ForegroundColor Cyan
    Write-Host $targetPath
}

if (Test-Path $setupPath) {
    Write-Host "`nInstalador (Setup) listo en:" -ForegroundColor Cyan
    Write-Host $setupPath
}

Write-Host "`nDisfruta de tu nueva aplicacion nativa!" -ForegroundColor Yellow
