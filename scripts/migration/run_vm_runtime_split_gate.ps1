param(
    [string]$RepoRoot = "",
    [string]$OutputDir = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\vm-runtime-split-gate"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$requiredCrates = @(
    "crates\novovm-protocol\Cargo.toml",
    "crates\novovm-consensus\Cargo.toml",
    "crates\novovm-network\Cargo.toml",
    "crates\novovm-adapter-api\Cargo.toml"
)
$requiredChecks = @()
foreach ($rel in $requiredCrates) {
    $full = Join-Path $RepoRoot $rel
    $requiredChecks += [ordered]@{
        path = $rel
        exists = [bool](Test-Path $full)
    }
}
$requiredMissing = @($requiredChecks | Where-Object { -not $_.exists })
$requiredPass = [bool]($requiredMissing.Count -eq 0)

$legacyVmRuntimePath = Join-Path $RepoRoot "src\vm-runtime"
$legacyVmRuntimePresent = [bool](Test-Path $legacyVmRuntimePath)

$legacyRefMatches = @()
$cargoTomls = Get-ChildItem -Path $RepoRoot -Recurse -Filter Cargo.toml -File
foreach ($cargo in $cargoTomls) {
    $hit = Select-String -Path $cargo.FullName -Pattern "vm-runtime|supervm-" -SimpleMatch -ErrorAction SilentlyContinue
    foreach ($m in $hit) {
        $legacyRefMatches += [ordered]@{
            file = $m.Path
            line = [int]$m.LineNumber
            text = $m.Line.Trim()
        }
    }
}

$allowedLegacyPatterns = @(
    "supervm-consensus",
    "supervm-network",
    "supervm-distributed",
    "supervm-dist-coordinator",
    "supervm-node",
    "supervm-protocol",
    "supervm-sdk",
    "supervm-chainlinker-api"
)
$legacyCargoViolations = @()
foreach ($m in $legacyRefMatches) {
    $isAllowed = $false
    foreach ($p in $allowedLegacyPatterns) {
        if ($m.text -like "*$p*") {
            $isAllowed = $true
            break
        }
    }
    if ($m.text -like "*vm-runtime*") {
        $isAllowed = $false
    }
    if (-not $isAllowed) {
        $legacyCargoViolations += $m
    }
}
$legacyRefPass = [bool]($legacyCargoViolations.Count -eq 0)

$pass = [bool](
    $requiredPass -and
    (-not $legacyVmRuntimePresent) -and
    $legacyRefPass
)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    required_crates = $requiredChecks
    required_crates_pass = $requiredPass
    legacy_vm_runtime_present = $legacyVmRuntimePresent
    legacy_vm_runtime_path = "src/vm-runtime"
    legacy_cargo_ref_pass = $legacyRefPass
    legacy_cargo_ref_violation_count = $legacyCargoViolations.Count
    legacy_cargo_ref_violations = $legacyCargoViolations
}

$summaryJson = Join-Path $OutputDir "vm-runtime-split-gate-summary.json"
$summaryMd = Join-Path $OutputDir "vm-runtime-split-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# VM Runtime Split Gate Summary",
    "",
    "- generated_at_utc: $($summary.generated_at_utc)",
    "- pass: $($summary.pass)",
    "- required_crates_pass: $($summary.required_crates_pass)",
    "- legacy_vm_runtime_present: $($summary.legacy_vm_runtime_present)",
    "- legacy_cargo_ref_pass: $($summary.legacy_cargo_ref_pass)",
    "- legacy_cargo_ref_violation_count: $($summary.legacy_cargo_ref_violation_count)",
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "vm-runtime split gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  required_crates_pass: $($summary.required_crates_pass)"
Write-Host "  legacy_vm_runtime_present: $($summary.legacy_vm_runtime_present)"
Write-Host "  legacy_cargo_ref_pass: $($summary.legacy_cargo_ref_pass)"
Write-Host "  legacy_cargo_ref_violation_count: $($summary.legacy_cargo_ref_violation_count)"
Write-Host "  summary_json: $summaryJson"

if (-not $pass) {
    throw "vm-runtime split gate FAILED"
}

Write-Host "vm-runtime split gate PASS"
