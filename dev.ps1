# dev.ps1 — lanza la app en modo desarrollo (daemon embebido en Tauri).
# Uso: .\dev.ps1

$guiDir = Join-Path $PSScriptRoot "gui"
Set-Location $guiDir
npm run tauri dev
