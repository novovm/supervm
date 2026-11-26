#Requires -Version 7.0
# Offline regression. The source directory contains public reports from a completed equal-weight trial.
param(
    [Parameter(Mandatory)][string] $OriginalEvidence,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string] $OriginalPlanSha256,
    [Parameter(Mandatory)][string] $Harness,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string] $HarnessSha256,
    [Parameter(Mandatory)][string] $OutputDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Check([bool] $Value, [string] $Message) { if (!$Value) { throw $Message } }
function File-Pin([string] $Path) { (Get-FileHash -LiteralPath $Path).Hash.ToLowerInvariant() }
function Write-Json([string] $Path, $Value) {
    $Value | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath $Path -Encoding utf8NoBOM
}
function Expect-Rejection([scriptblock] $Action, [string] $Message) {
    try { & $Action | Out-Null } catch {
        if ($_.Exception.Message.Contains($Message)) { return }
        throw
    }
    throw "Expected rejection: $Message"
}
$runner = Join-Path $PSScriptRoot '../novovm-distributed-validator-catchup.ps1'
$verify = Join-Path $PSScriptRoot '../novovm-verify-distributed-validator-catchup.ps1'
$source = Join-Path $OriginalEvidence 'plan.json'
Check ((File-Pin $source) -ceq $OriginalPlanSha256) 'Original plan digest mismatch'
Check ((File-Pin $Harness) -ceq $HarnessSha256) 'Harness digest mismatch'
Check (!(Test-Path -LiteralPath $OutputDirectory)) 'Use a new output directory'
$root = (New-Item -ItemType Directory -Path $OutputDirectory).FullName
$original = Get-Content -LiteralPath $source -Raw | ConvertFrom-Json
$oldResult = & $verify -PlanFile $source -PlanSha256 $OriginalPlanSha256 -EvidenceDirectory $OriginalEvidence | ConvertFrom-Json
Check ($oldResult.result -ceq 'PASS' -and $oldResult.quorum_weight -eq 3) 'Original equal-weight acceptance failed'
Write-Json (Join-Path $root 'original-acceptance.json') $oldResult

# Both dry plans use identical public material and test domain to isolate the change of weights.
$run = [guid]::NewGuid().ToString('N')
$chain = 7000000000L + [Security.Cryptography.RandomNumberGenerator]::GetInt32(1000000000)
Check ($chain -ne $original.chain_id) 'Run again with a new test chain ID'
$slots = @(foreach ($slot in $original.slots) {
    $path = Join-Path $root "$($slot.role)-public.json"
    Write-Json $path ([ordered]@{role=$slot.role; run_id=$run; chain_id=$chain; address=$slot.address
        validator=$slot.validator; transport=$slot.transport; aoem_sha256=$original.aoem_sha256; ctl_sha256=$original.ctl_sha256})
    $path
})
$now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
$common = @{ Action='Plan'; RunId=$run; ChainId=$chain; PublicSlots=$slots; StartUnix=$now; EndUnix=$now+600
    NodeSha256=$original.node_sha256; AoemSha256=$original.aoem_sha256; CtlSha256=$original.ctl_sha256
    Harness=$Harness; HarnessSha256=$HarnessSha256 }
$results = @{}
foreach ($scenario in 'Equal','UnequalPairs') {
    $dir = (New-Item -ItemType Directory -Path (Join-Path $root $scenario)).FullName
    $plan = Join-Path $dir 'plan.json'
    & $runner @common -PlanFile $plan -WeightScenario $scenario | Out-Null
    $genesis = Join-Path $dir 'genesis.json'
    Copy-Item -LiteralPath "$plan.genesis.json" -Destination $genesis
    $arguments = @{PlanFile=$plan; PlanSha256=(File-Pin $plan); EvidenceDirectory=$dir; GenesisSha256=(File-Pin $genesis); PlanOnly=$true}
    $check = & $verify @arguments | ConvertFrom-Json
    Check ($check.result -ceq 'PLAN_ONLY') 'Plan-only check claimed an execution result'
    $results[$scenario] = $check
    Write-Json (Join-Path $dir 'review.json') $check
}
$low = $results.Equal
$unequal = $results.UnequalPairs
Check ($low.total_weight -eq 4 -and $low.quorum_weight -eq 3 -and $low.a1_c1_weight -eq 2 -and $low.a2_c1_weight -eq 2) 'Equal weight derivation failed'
Check ($unequal.total_weight -eq 7 -and $unequal.quorum_weight -eq 5 -and $unequal.a1_c1_weight -eq 3 -and $unequal.a2_c1_weight -eq 5) 'Unequal pair derivation failed'
Check ($low.genesis -cne $unequal.genesis -and $low.plan_sha256 -cne $unequal.plan_sha256) 'Weights were not committed to genesis and plan'
$bad = (New-Item -ItemType Directory -Path (Join-Path $root 'negative')).FullName
$badGenesis = Join-Path $bad 'genesis.json'
$document = Get-Content -LiteralPath $genesis -Raw | ConvertFrom-Json
$document.genesis.validators[1].weight = 1
Write-Json $badGenesis $document
$badArgs = $arguments.Clone()
$badArgs.EvidenceDirectory = $bad
$badArgs.GenesisSha256 = File-Pin $badGenesis
Expect-Rejection { & $verify @badArgs } 'Plan differs from genesis validator'
$badArgs = $arguments.Clone()
$badArgs.GenesisSha256 = $low.genesis_sha256
Expect-Rejection { & $verify @badArgs } 'Genesis file digest differs'
$badArgs = $arguments.Clone()
$badArgs.Remove('GenesisSha256')
Expect-Rejection { & $verify @badArgs } 'Supply the independent genesis file digest'

# Mutate only the public archive report on a disposable copy of the authenticated equal-weight run.
$badArchiveDir = (New-Item -ItemType Directory -Path (Join-Path $root 'archive-controls')).FullName
Get-ChildItem -LiteralPath $OriginalEvidence -File -Filter '*.json' | Copy-Item -Destination $badArchiveDir
$archivePath = Join-Path $badArchiveDir 'C1-persisted.json'
$archive = Get-Content -LiteralPath $archivePath -Raw | ConvertFrom-Json
$archive.archive.signed_weight++
Write-Json $archivePath $archive
$checkArchive = { & $verify -PlanFile $source -PlanSha256 $OriginalPlanSha256 -EvidenceDirectory $badArchiveDir }
Expect-Rejection $checkArchive 'Missing durable head'
$archive.archive.signed_weight--
$archive.archive.signers[1] = $archive.archive.signers[0]
Write-Json $archivePath $archive
Expect-Rejection $checkArchive 'Duplicate or empty QC signers'
$archive.archive.signers[1] = '0' * 64
Write-Json $archivePath $archive
Expect-Rejection $checkArchive 'Unknown QC signer'
Write-Output "PASS: equal original run, two native-derived plans, changed commitments, unequal same-size pairs, three plan and three archive rejection controls. No node or wallet operation."
