# build-portable.ps1 - Compila open_herd en una aplicación nativa usando Tauri

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host "==============================================" -ForegroundColor Cyan
Write-Host " Compilador Portable (Tauri) para open_herd" -ForegroundColor Cyan
Write-Host "==============================================" -ForegroundColor Cyan

# 1. Compilar el daemon de Go
Write-Host "`n[1/3] Compilando el motor de Go (phpenv-daemon)..." -ForegroundColor Yellow
Set-Location (Join-Path $root "daemon")
# Usamos -ldflags="-H windowsgui" para que el motor no abra una ventana negra de consola al ejecutarse
go build -ldflags="-H windowsgui" -o (Join-Path $root "gui\src-tauri\phpenv-daemon.exe") .

if (-not (Test-Path (Join-Path $root "gui\src-tauri\phpenv-daemon.exe"))) {
    Write-Host "ERROR: No se pudo compilar el ejecutable de Go." -ForegroundColor Red
    exit 1
}

# 2. Instalar dependencias del frontend si es necesario
Write-Host "`n[2/3] Verificando dependencias del Frontend (npm install)..." -ForegroundColor Yellow
Set-Location (Join-Path $root "gui")
npm install

# 3. Construir la app nativa con Tauri
Write-Host "`n[3/3] Empaquetando la aplicación nativa con Tauri (esto tardará unos minutos la primera vez)..." -ForegroundColor Yellow
npm run tauri build

$targetPath = Join-Path $root "gui\src-tauri\target\release\phpenv.exe"
$setupPath = Join-Path $root "gui\src-tauri\target\release\bundle\nsis\phpenv_0.1.0_x64-setup.exe"

Write-Host "`n==============================================" -ForegroundColor Green
Write-Host " ¡COMPILACIÓN EXITOSA!" -ForegroundColor Green
Write-Host "==============================================" -ForegroundColor Green

if (Test-Path $targetPath) {
    Write-Host "`nEjecutable Portable listo en:" -ForegroundColor Cyan
    Write-Host $targetPath
}

if (Test-Path $setupPath) {
    Write-Host "`nInstalador (Setup) listo en:" -ForegroundColor Cyan
    Write-Host $setupPath
}

Write-Host "`n¡Disfruta de tu nueva aplicación nativa!" -ForegroundColor Yellow
