param(
  [string]$PackageDir = "artifacts\network-overlay-gate\novovm-public-relay-v0",
  [string]$BindAddr = "0.0.0.0:41030",
  [string]$NodeId = "public-relay-1",
  [switch]$Release
)

$ErrorActionPreference = "Stop"

$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $repo
try {
  if ($Release) {
    cargo build -q -p novovm-node --bin supervm-network-overlay-gate --release
    $binary = "target\release\supervm-network-overlay-gate.exe"
  } else {
    cargo build -q -p novovm-node --bin supervm-network-overlay-gate
    $binary = "target\debug\supervm-network-overlay-gate.exe"
  }

  if (!(Test-Path $binary)) {
    throw "relay binary missing: $binary"
  }

  New-Item -ItemType Directory -Force -Path $PackageDir | Out-Null
  Copy-Item -Force $binary (Join-Path $PackageDir "supervm-network-overlay-gate.exe")

  $env:NOVOVM_OVERLAY_GATE_MODE = "headless-public-relay-deploy-package-matrix"
  $env:NOVOVM_HEADLESS_RELAY_PACKAGE_DIR = $PackageDir
  $env:NOVOVM_HEADLESS_RELAY_BIND_ADDR = $BindAddr
  $env:NOVOVM_HEADLESS_RELAY_NODE_ID = $NodeId
  $env:NOVOVM_OVERLAY_GATE_REPORT_PATH = Join-Path $PackageDir "reports\headless-package-matrix.json"

  & ".\$binary"
}
finally {
  Pop-Location
}
