# Build a Windows distribution folder for the BerryOS device shell.
# QEMU stays external (GPLv2); the folder ships berry.exe + image + AAVMF.
param(
  [string]$TargetDir = "out/BerryOS-win64",
  [string]$Target = "x86_64-pc-windows-gnu"
)
$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")).Path
Set-Location $Root

$Exe = if ($env:BERRY_EXE) { $env:BERRY_EXE } else { "tools/berry/cargo-target/$Target/release/berry.exe" }
$Img = if ($env:BERRY_IMAGE) { $env:BERRY_IMAGE } else { "out/berryos.img" }
if (-not (Test-Path $Exe -PathType Leaf)) {
  throw "missing $Exe`n  build it: cargo build --release --target $Target`n  or set BERRY_EXE=C:\path\berry.exe"
}
if (-not (Test-Path $Img -PathType Leaf)) {
  throw "missing $Img`n  build it: just image`n  or set BERRY_IMAGE=C:\path\berryos.img"
}

$Fw = $env:AAVMF_CODE
if (-not $Fw) {
  $Candidates = @(
    "C:\Program Files\qemu\share\edk2-aarch64-code.fd",
    "C:\Program Files (x86)\qemu\share\edk2-aarch64-code.fd",
    (Join-Path $Root "out\AAVMF_CODE.fd")
  )
  $Fw = $Candidates | Where-Object { Test-Path $_ -PathType Leaf } | Select-Object -First 1
}
if (-not $Fw -or -not (Test-Path $Fw -PathType Leaf)) {
  throw "AAVMF code not found; install QEMU or set AAVMF_CODE=C:\path\AAVMF_CODE.fd"
}

New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
Copy-Item $Exe (Join-Path $TargetDir "berry.exe") -Force
Copy-Item $Img (Join-Path $TargetDir "berryos.img") -Force
Copy-Item $Fw  (Join-Path $TargetDir "AAVMF_CODE.fd") -Force

@'
@echo off
setlocal
set BERRY_IMAGE=%~dp0berryos.img
set AAVMF_CODE=%~dp0AAVMF_CODE.fd
"%~dp0berry.exe" shell %*
'@ | Set-Content (Join-Path $TargetDir "run-berry.cmd") -Encoding ASCII

@'
BerryOS device shell (Windows)
================================
Run: double-click run-berry.cmd, or:
  berry.exe shell
  berry.exe device start --profile phone
  berry.exe screenshot --out shot.png

Requirements:
- qemu-system-aarch64 on PATH, or set BERRY_QEMU to qemu-system-aarch64.exe.
  QEMU is GPLv2 and is NOT bundled here.
- AAVMF_CODE.fd + berryos.img are in this folder; run-berry.cmd sets them.
- Current berry.exe is a console binary. GUI subsystem/AttachConsole is a
  pending C1g tail; double-clicking may briefly show a console window.

CLI is the primary interface: device/screenshot/record/play/stats/input/power.
'@ | Set-Content (Join-Path $TargetDir "README.txt") -Encoding UTF8

Write-Host "packaged: $TargetDir"
Write-Host "  berry.exe + berryos.img + AAVMF_CODE.fd + run-berry.cmd"
