#Requires -Version 5.1
# install.ps1 — Descarga e instala Open Herd desde GitHub Releases
#
# Uso (PowerShell como Administrador):
#   irm https://raw.githubusercontent.com/TU_USUARIO/open_herd/main/installer/install.ps1 | iex
#
# O con versión específica:
#   $env:OPENHERD_VERSION = "0.2.0"
#   irm https://...install.ps1 | iex

[CmdletBinding()]
param(
    [string]$Version = $env:OPENHERD_VERSION,
    [switch]$Portable,
    [string]$PortableDir = "C:\open-herd"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoOwner = "TU_USUARIO"        # ← cambia esto por tu usuario de GitHub
$RepoName  = "open_herd"
$ApiBase   = "https://api.github.com/repos/$RepoOwner/$RepoName"

function Write-Info    { Write-Host "[open-herd] $args" -ForegroundColor Cyan    }
function Write-Success { Write-Host "[open-herd] $args" -ForegroundColor Green   }
function Write-Warn    { Write-Host "[open-herd] $args" -ForegroundColor Yellow  }
function Fail          { Write-Host "[open-herd] ERROR: $args" -ForegroundColor Red; exit 1 }

# ── Resolución de versión ─────────────────────────────────────────────────────

if (-not $Version) {
    Write-Info "Buscando la última versión..."
    try {
        $release = Invoke-RestMethod "$ApiBase/releases/latest" -Headers @{ "User-Agent" = "open-herd-installer" }
        $Version = $release.tag_name -replace '^v', ''
        Write-Info "Última versión: $Version"
    } catch {
        Fail "No se pudo obtener la versión de GitHub: $_"
    }
}

# ── Modo portable ─────────────────────────────────────────────────────────────

if ($Portable) {
    Write-Info "Modo portable → instalando en $PortableDir"

    $zipName = "open-herd-v$Version-portable.zip"
    $zipUrl  = "https://github.com/$RepoOwner/$RepoName/releases/download/v$Version/$zipName"
    $zipTmp  = Join-Path $env:TEMP $zipName

    Write-Info "Descargando $zipName..."
    Invoke-WebRequest -Uri $zipUrl -OutFile $zipTmp -UseBasicParsing

    if (Test-Path $PortableDir) { Remove-Item $PortableDir -Recurse -Force }
    New-Item -ItemType Directory -Path $PortableDir | Out-Null
    Expand-Archive -Path $zipTmp -DestinationPath $PortableDir -Force
    Remove-Item $zipTmp

    Write-Success "Open Herd v$Version instalado en $PortableDir"
    Write-Host ""
    Write-Host "  Ejecuta: $PortableDir\open-herd.exe" -ForegroundColor White
    exit 0
}

# ── Instalador completo (NSIS) ────────────────────────────────────────────────

if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Fail "Ejecuta este script como Administrador para instalar Open Herd para todos los usuarios."
}

$setupName = "open-herd-v$Version-setup.exe"
$setupUrl  = "https://github.com/$RepoOwner/$RepoName/releases/download/v$Version/$setupName"
$setupTmp  = Join-Path $env:TEMP $setupName

Write-Info "Descargando $setupName..."
Invoke-WebRequest -Uri $setupUrl -OutFile $setupTmp -UseBasicParsing

Write-Info "Ejecutando instalador (modo silencioso)..."
$proc = Start-Process $setupTmp -ArgumentList "/S" -Wait -PassThru
if ($proc.ExitCode -ne 0) {
    Fail "El instalador terminó con código $($proc.ExitCode)"
}
Remove-Item $setupTmp -ErrorAction SilentlyContinue

Write-Success "Open Herd v$Version instalado correctamente."
Write-Host ""
Write-Host "  Abre el menú Inicio → Open Herd" -ForegroundColor White
