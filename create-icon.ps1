$base64 = "AAABAAEAAQEAAAEAIAAwAAAAFgAAACgAAAABAAAAAgAAAAEAIAAAAAAACAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAwAAAAA=="
$bytes = [Convert]::FromBase64String($base64)
$iconsDir = Join-Path $PSScriptRoot "gui\src-tauri\icons"
if (-not (Test-Path $iconsDir)) {
    New-Item -ItemType Directory -Path $iconsDir | Out-Null
}
[IO.File]::WriteAllBytes((Join-Path $iconsDir "icon.ico"), $bytes)
Write-Host "icon.ico created successfully!"
