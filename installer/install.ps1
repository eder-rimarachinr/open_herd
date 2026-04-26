#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallDir = "$env:USERPROFILE\.phpenv"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoUrl   = "https://github.com/open-herd/phpenv"
$BinDir    = "$InstallDir\bin"

function Write-Info    { Write-Host "[phpenv] $args" -ForegroundColor Cyan    }
function Write-Success { Write-Host "[phpenv] $args" -ForegroundColor Green   }
function Write-Warn    { Write-Host "[phpenv] $args" -ForegroundColor Yellow  }
function Fail          { Write-Host "[phpenv] ERROR: $args" -ForegroundColor Red; exit 1 }

function Require-Command([string]$cmd) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        Fail "$cmd is required but not found in PATH."
    }
}

# ── Pre-flight ────────────────────────────────────────────────────────────────

if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Fail "Run this script as Administrator (needed to write the hosts file and install services)."
}

Write-Info "Checking dependencies..."
Require-Command mkcert

# ── Directories ───────────────────────────────────────────────────────────────

Write-Info "Creating directories under $InstallDir ..."
foreach ($sub in @("nginx\sites", "php", "certs", "logs", $BinDir)) {
    New-Item -ItemType Directory -Force -Path "$InstallDir\$sub" | Out-Null
}

# ── nginx ─────────────────────────────────────────────────────────────────────

$NginxArchive = "$env:TEMP\nginx.zip"
$NginxUrl     = "https://nginx.org/download/nginx-1.26.1.zip"

if (-not (Test-Path "$InstallDir\nginx\nginx.exe")) {
    Write-Info "Downloading nginx..."
    Invoke-WebRequest -Uri $NginxUrl -OutFile $NginxArchive -UseBasicParsing
    Expand-Archive -Path $NginxArchive -DestinationPath "$env:TEMP\nginx_extract" -Force
    $extracted = Get-ChildItem "$env:TEMP\nginx_extract" -Directory | Select-Object -First 1
    Copy-Item "$($extracted.FullName)\*" "$InstallDir\nginx\" -Recurse -Force
    Remove-Item $NginxArchive, "$env:TEMP\nginx_extract" -Recurse -Force
}

# ── mkcert CA ─────────────────────────────────────────────────────────────────

Write-Info "Installing local CA with mkcert..."
$env:CAROOT = "$InstallDir\certs"
mkcert -install

# ── Download daemon + CLI ─────────────────────────────────────────────────────

$Arch = if ([System.Environment]::Is64BitOperatingSystem) { "amd64" } else { "386" }

foreach ($bin in @("phpenv-daemon", "phpenv")) {
    $url  = "$RepoUrl/releases/download/$Version/$bin-windows-$Arch.exe"
    $dest = "$BinDir\$bin.exe"
    Write-Info "Downloading $bin..."
    Invoke-WebRequest -Uri $url -OutFile $dest -UseBasicParsing
}

# ── Windows Service (daemon) ──────────────────────────────────────────────────

$svcName = "phpenv-daemon"
if (Get-Service -Name $svcName -ErrorAction SilentlyContinue) {
    Write-Warn "Service $svcName already exists — skipping creation."
} else {
    Write-Info "Installing Windows service..."
    New-Service -Name $svcName `
                -DisplayName "phpenv Daemon" `
                -Description "Local PHP environment manager daemon" `
                -BinaryPathName "$BinDir\phpenv-daemon.exe" `
                -StartupType Automatic | Out-Null
    Start-Service -Name $svcName
}

# ── PATH ──────────────────────────────────────────────────────────────────────

$currentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($currentPath -notlike "*$BinDir*") {
    Write-Info "Adding $BinDir to user PATH..."
    [Environment]::SetEnvironmentVariable("PATH", "$currentPath;$BinDir", "User")
}

Write-Success "phpenv installed successfully!"
Write-Host ""
Write-Host "  Service:  Get-Service phpenv-daemon"
Write-Host "  CLI:      phpenv status"
Write-Host ""
Write-Warn "Open a new terminal for PATH changes to take effect."
