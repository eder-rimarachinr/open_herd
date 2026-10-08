# dev.ps1 — lanza la app en modo desarrollo (daemon embebido en Tauri).
# Uso (desde cualquier carpeta): .\scripts\dev.ps1

Set-Location (Split-Path $PSScriptRoot -Parent)
npm run tauri dev
