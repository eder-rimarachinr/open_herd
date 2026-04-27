# dev.ps1 — lanza el daemon como Administrador y el frontend en la terminal actual.
# Uso: .\dev.ps1

$root = $PSScriptRoot
$daemonDir = Join-Path $root "daemon"
$guiDir    = Join-Path $root "gui"

# Verifica que Go esté en el PATH
if (-not (Get-Command go -ErrorAction SilentlyContinue)) {
    $env:PATH += ";C:\Program Files\Go\bin"
}

Write-Host "Iniciando daemon como Administrador..." -ForegroundColor Cyan
Start-Process powershell -Verb RunAs -ArgumentList `
    '-NoExit', '-Command', `
    "cd '$daemonDir'; `$env:PATH += ';C:\Program Files\Go\bin'; go run ."

Write-Host "Iniciando frontend en http://localhost:1420 ..." -ForegroundColor Cyan
Set-Location $guiDir
npm run dev
