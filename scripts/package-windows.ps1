# Build a Windows NSIS installer for Orange.
#
# Prerequisites:
#   NSIS (https://nsis.sourceforge.io/) with EnVar plugin installed.
# Output:
#   target/windows/orange-<version>-x64-setup.exe

$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

# Extract workspace version
$cargo = Get-Content "Cargo.toml" -Raw
if ($cargo -match '(?m)^version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    Write-Error "could not extract version from Cargo.toml"
}

Write-Host "==> building Orange release binary"
cargo build --release --bin orange --bin orange-grep
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force -Path "target\windows" | Out-Null

$makensis = Get-Command "makensis" -ErrorAction SilentlyContinue
if (-not $makensis) {
    Write-Error "makensis not found in PATH. Install NSIS first."
}

Write-Host "==> running makensis"
& makensis "/DVERSION=$Version" "/DARCH=x64" "installer\windows\orange.nsi"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> output:"
Get-ChildItem "target\windows\*.exe"
