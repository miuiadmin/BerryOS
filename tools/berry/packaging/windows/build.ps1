# Build a Windows distribution folder for the BerryOS device shell.
# QEMU stays external (GPLv2); the folder ships berry.exe + image + AAVMF.
param(
  [string]$TargetDir = "out/BerryOS-win64",
  [string]$Target = "x86_64-pc-windows-gnu"
)
$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")
Set-Location $Root
$Exe = if ($env:BERRY_EXE) { $env:BERRY_EXE } else { "tools/berry/cargo-target/$Target/release/berry.exe" }
$Img = if ($env:BERRY_IMAGE) { $env:BERRY_IMAGE } else { "out/berryos.img" }
if (-not (Test-Path $Exe)) { throw "missing $Exe — cross-build with cargo --target $Target, or set BERRY_EXE" }
if (-not (Test-Path $Img)) { throw "missing $Img — run: just image" }
New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
Copy-Item $Exe (Join-Path $TargetDir "berry.exe") -Force
Copy-Item $Img (Join-Path $TargetDir "berryos.img") -Force
$Fw = $env:AAVMF_CODE
if (-not $Fw) {
  $Candidates = @(
    "C:\Program Files\qemu\share\edk2-aarch64-code.fd",
    "C:\Program Files (x86)\qemu\share\edk2-aarch64-code.fd"
  )
  $Fw = $Candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if ($Fw -and (Test-Path $Fw)) { Copy-Item $Fw (Join-Path $TargetDir "AAVMF_CODE.fd") -Force }
else { Write-Warning "AAVMF code not found; set AAVMF_CODE" }
@"
BerryOS device shell (Windows)
- Double-click berry.exe (GUI subsystem/AttachConsole is a pending tail; console may appear).
- QEMU is external (GPLv2): install qemu-system-aarch64 and put it on PATH,
  or set BERRY_QEMU.
- AAVMF_CODE.fd and berryos.img are bundled here.
"@ | Set-Content (Join-Path $TargetDir "README.txt")
Write-Host "packaged: $TargetDir"
