#requires -Version 7.0
[CmdletBinding()]
param(
    [ValidateSet('same-bytes','continuous','recovery')][string]$Suite='same-bytes',
    [switch]$Baseline,
    [switch]$Observe
)
$ErrorActionPreference='Stop'
$taskRepo=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../..')).Path
$taskManifest=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'candidate.json') -Raw | ConvertFrom-Json
foreach($entry in $taskManifest.files){
    if((Get-FileHash -LiteralPath $entry.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256){throw "Artifact changed: $($entry.role)"}
}
if((Get-Volume -DriveLetter D).SizeRemaining -lt 15GB){throw 'Insufficient space; existing databases and evidence will not be deleted.'}
if(Get-Process -Name novovm-node,native-tests,cargo,rustc,clippy-driver -ErrorAction SilentlyContinue){throw 'Run alone, after builds and other workloads finish.'}
$environment=Get-Content -LiteralPath (Join-Path $taskRepo 'artifacts/audit/native-controller-throughput-20261031/candidate-v1.json') -Raw | ConvertFrom-Json
foreach($entry in $environment.environment.PSObject.Properties){[Environment]::SetEnvironmentVariable($entry.Name,[string]$entry.Value,'Process')}
$taskOld=($taskManifest.files | Where-Object role -eq 'baseline_node').path
$taskNew=($taskManifest.files | Where-Object role -eq 'candidate_node').path
$taskTester=($taskManifest.files | Where-Object role -eq 'tester').path
$env:NOVOVM_NATIVE_UCA_BASELINE_NODE_BINARY=$taskOld
$env:NOVOVM_RESIDENT_NODE_BINARY=$(if($Baseline){$taskOld}else{$taskNew})
$taskTest=switch($Suite){
    'same-bytes' {'native_pipeline::consensus::tests::native_product_rpc::native_load::uca::native_uca_same_corpus_bulk_authority_before_after'}
    'continuous' {'native_pipeline::consensus::tests::native_product_rpc::native_load::uca::rolling::native_asset_continuous_65536_signature_cpu_gpu_cpu'}
    'recovery' {'native_pipeline::consensus::tests::native_product_rpc::application::queries::uca_recovery::native_uca_process_late_validator_interrupted_catchup_respend_and_cold_recovery'}
}
if($Suite -eq 'same-bytes' -and $Baseline){throw 'Same-byte comparison must retain different before/after paths.'}
$taskLabel="$Suite-$(if($Baseline){'baseline'}else{'candidate'})-$(if($Observe){'observed'}else{'unobserved'})"
$taskOut=Join-Path $taskRepo "target/native-aad-session-reuse-20261101/$(Get-Date -Format yyyyMMdd-HHmmss)-$PID-$taskLabel"
New-Item -ItemType Directory -Path $taskOut | Out-Null
$taskArgs=@($taskTest,'--exact','--ignored','--nocapture','--test-threads=1')
$taskBegin=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$taskDriver=Start-Process -FilePath $taskTester -ArgumentList $taskArgs -WorkingDirectory $taskRepo -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $taskOut 'stdout.log') -RedirectStandardError (Join-Path $taskOut 'stderr.log')
$taskObservation=$null
try{
    if($Observe){
        $taskObservation=& (Join-Path $taskRepo 'scripts/novovm-native-pipeline-memory-profiler.ps1') -Action series -TargetPid $taskDriver.Id -NodeBinary @($taskOld,$taskNew) -Label $taskLabel -ArtifactDir $taskOut -DurationSeconds 900 -SampleIntervalMilliseconds 1000
    }
}finally{
    $taskDriver.WaitForExit()
    [ordered]@{
        schema='novovm-aad-session-load/v1';suite=$Suite;baseline_only=[bool]$Baseline;observed=[bool]$Observe
        scope='Original frozen harness and native owners; no ETW; no signing/quorum/economic/timeout changes; databases/journals retained. Observer runs separately from final unobserved TPS acceptance.'
        begin_unix_ms=$taskBegin;end_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        driver_pid=$taskDriver.Id;exit_code=$taskDriver.ExitCode;test=$taskTest;observer=$taskObservation;files=$taskManifest.files
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $taskOut 'run.json') -Encoding utf8NoBOM
}
Write-Output $taskOut
if($taskDriver.ExitCode -ne 0){throw "Original harness failed: $($taskDriver.ExitCode). Logs retained at $taskOut"}
