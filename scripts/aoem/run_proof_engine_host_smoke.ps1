param(
  [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path,
  [string]$LibraryPath = "",
  [switch]$SkipWorkerAdapter
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($LibraryPath)) {
  $LibraryPath = Join-Path $RepoRoot "aoem\windows\core\bin\aoem_ffi.dll"
}

$packageRoot = Join-Path $RepoRoot "aoem"
$worker = Join-Path $packageRoot "bin\windows-x86_64\aoem-proof-worker.exe"
$workerLibrary = Join-Path $RepoRoot "aoem\windows\core\bin\aoem_ffi.dll"
$zkJobs = Join-Path $packageRoot "worker-adapter\examples\jobs.zk_merkle.jsonl"
$workerOutput = Join-Path $env:TEMP "supervm_aoem_zk_merkle_proofs.jsonl"

Write-Host "SUPERVM AOEM fullmax embedded proof engine smoke"
Write-Host "library=$LibraryPath"

Push-Location $RepoRoot
try {
  cargo run -p aoem-bindings --example proof_engine_host_smoke -- --dll $LibraryPath

  if (-not $SkipWorkerAdapter) {
    if (!(Test-Path -LiteralPath $worker)) {
      throw "missing worker adapter: $worker"
    }
    if (!(Test-Path -LiteralPath $workerLibrary)) {
      throw "missing worker adapter library: $workerLibrary"
    }
    if (!(Test-Path -LiteralPath $zkJobs)) {
      throw "missing zk Merkle worker jobs: $zkJobs"
    }
    & $worker --library $workerLibrary --input $zkJobs --output $workerOutput --batch-count 4
  }
} finally {
  Pop-Location
}
