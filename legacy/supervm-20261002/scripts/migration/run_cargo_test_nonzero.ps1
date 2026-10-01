param(
  [Parameter(Mandatory = $true)]
  [string]$Package,

  [Parameter(Mandatory = $true)]
  [string]$Filter
)

$ErrorActionPreference = "Continue"

$cargoCommand = "cargo test -p `"$Package`" `"$Filter`" -- --test-threads=1 2>&1"
$output = & cmd.exe /d /s /c $cargoCommand
$exitCode = $LASTEXITCODE
$output | ForEach-Object { Write-Output $_ }

if ($exitCode -ne 0) {
  exit $exitCode
}

$matched = 0
foreach ($line in $output) {
  if ($line -match "running\s+(\d+)\s+tests?") {
    $matched += [int]$Matches[1]
  }
}

if ($matched -eq 0) {
  Write-Output "ERROR: cargo test matched 0 tests for package '$Package' filter '$Filter'"
  exit 64
}
