Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-NovovmRolloutPolicyBinary {
    $repoRoot = Split-Path -Parent $PSScriptRoot
    $candidates = @()
    if ($env:NOVOVM_ROLLOUT_POLICY_BIN) { $candidates += $env:NOVOVM_ROLLOUT_POLICY_BIN }
    if ($env:NOVOVM_POLICY_CLI_BIN) { $candidates += $env:NOVOVM_POLICY_CLI_BIN }
    $candidates += @(
        (Join-Path $repoRoot 'target\\release\\novovm-rollout-policy.exe'),
        (Join-Path $repoRoot 'target\\debug\\novovm-rollout-policy.exe'),
        'novovm-rollout-policy'
    )
    foreach ($candidate in $candidates) {
        if (-not $candidate) { continue }
        if ($candidate -eq 'novovm-rollout-policy') { return $candidate }
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    throw 'novovm-rollout-policy binary not found. Set NOVOVM_ROLLOUT_POLICY_BIN or NOVOVM_POLICY_CLI_BIN.'
}

$binary = Resolve-NovovmRolloutPolicyBinary
& $binary overlay relay-discovery-merge @args
if ($null -ne $LASTEXITCODE) { exit $LASTEXITCODE }
exit 0
