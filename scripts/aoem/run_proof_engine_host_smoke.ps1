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
$workerLibrary = $LibraryPath
$publicJobs = Join-Path $packageRoot "worker-adapter\examples\jobs.merkle.jsonl"
$zkJobs = Join-Path $packageRoot "worker-adapter\examples\jobs.zk_merkle.jsonl"
$testRoot = Join-Path $RepoRoot "target\proof-envelope-smoke"
New-Item -ItemType Directory -Force -Path $testRoot | Out-Null
$workerOutput = Join-Path $testRoot "private-rejected.jsonl"
$publicOutput = Join-Path $testRoot "public-diagnostics.jsonl"

Write-Host "SUPERVM AOEM fullmax embedded proof engine smoke"
Write-Host "library=$LibraryPath"

Push-Location $RepoRoot
try {
  cargo run --locked -p aoem-bindings --example proof_engine_host_smoke -- --dll $LibraryPath
  if ($LASTEXITCODE -ne 0) { throw "Rust envelope smoke failed" }

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
    & $worker --library $workerLibrary --input $publicJobs --output $publicOutput --batch-count 4
    if ($LASTEXITCODE -ne 0) { throw "public envelope diagnostic failed" }
    $publicRows = @(Get-Content $publicOutput | ForEach-Object { $_ | ConvertFrom-Json })
    $successes = @($publicRows | Where-Object status -eq 'ok')
    if ($successes.Count -eq 0) { throw "no public diagnostic output" }
    foreach ($row in $successes) {
      if ($row.verification_scope -ne 'envelope_integrity_only_not_zk' -or
          $row.accepted -ne $false -or $row.proof_verified -ne $false -or
          $row.cryptographic_proof_verified -ne $false -or
          $row.envelope_integrity_verified -ne $true) {
        throw "public diagnostic falsely claims cryptographic proof acceptance"
      }
    }
    & $worker --library $workerLibrary --input $zkJobs --output $workerOutput --batch-count 4
    if ($LASTEXITCODE -ne 1) { throw "private profile must return rejection, not success" }
    $rows = @(Get-Content $workerOutput | ForEach-Object { $_ | ConvertFrom-Json })
    if (@($rows | Where-Object error -eq 'unsupported_private_membership_proof').Count -eq 0) {
      throw "private profile rejection missing"
    }
    foreach ($row in $rows) {
      if ($row.status -ne 'error' -or $row.proof_written -ne $false -or
          $row.error -notin @('unsupported_private_membership_proof', 'malformed_payload')) {
        throw "private worker emitted unexpected result"
      }
    }
    Write-Output 'SUPERVM_AOEM_PRIVATE_PROFILE_REJECTION|unsupported=ok|proof_written=false|replacement_zk=false'
  }
} finally {
  Pop-Location
}
exit 0
