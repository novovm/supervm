param([Parameter(Mandatory)][string]$NodeBinary, [Parameter(Mandatory)][string]$Label)
$ErrorActionPreference='Stop'
Set-Location (Resolve-Path "$PSScriptRoot/../../..").Path
if($Label -notmatch '^[a-z0-9-]+$'){throw 'Invalid diagnostic label'}
$config=(Resolve-Path 'target/native-product-rpc/native-only-18696-1792820414906582400/node-0.json').Path
$parameters=Get-Content -LiteralPath $config -Raw | ConvertFrom-Json -AsHashtable
$env:NOVOVM_NODE_MODE=$parameters.profile
$env:NOVOVM_NATIVE_EXECUTION_CONFIG=$config
$env:NOVOVM_NATIVE_EXECUTION_START='existing'
$env:NOVOVM_NATIVE_EXECUTION_TICK_CHAIN_ID=[string]$parameters.genesis.chain_id
$env:NOVOVM_NATIVE_EXECUTION_TICK_MAX_TICKS='0'
$env:NOVOVM_NATIVE_PIPELINE_TRANSPORT='novorudp'
$env:NOVOVM_NATIVE_EXECUTION_PIPELINE_UDP_ENABLED='1'
$env:NOVOVM_PROFILE_STAGES='1'
$env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_INTERVAL_MS='250'
$report=Join-Path $PSScriptRoot "$Label.progress.json"
$stdout=Join-Path $PSScriptRoot "$Label.stdout.log"
$stderr=Join-Path $PSScriptRoot "$Label.stderr.log"
foreach($path in @($report,$stdout,$stderr)){if(Test-Path -LiteralPath $path){throw 'Preserve existing diagnostic evidence'}}
$env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH=$report
$timer=[System.Diagnostics.Stopwatch]::StartNew()
$child=Start-Process -FilePath (Resolve-Path -LiteralPath $NodeBinary).Path -WorkingDirectory (Get-Location).Path -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
try {
    while($timer.Elapsed.TotalSeconds -lt 20){
        $child.Refresh()
        [pscustomobject]@{label=$Label;seconds=$timer.Elapsed.TotalSeconds;pid=$child.Id;exited=$child.HasExited;cpu_seconds=if($child.HasExited){$null}else{$child.TotalProcessorTime.TotalSeconds};progress_exists=(Test-Path -LiteralPath $report)} | ConvertTo-Json -Compress
        if($child.HasExited -or (Test-Path -LiteralPath $report)){break}
        Start-Sleep -Milliseconds 1000
    }
} finally {
    $child.Refresh()
    if(-not $child.HasExited){Stop-Process -Id $child.Id; $child.WaitForExit()}
}
Get-Content -LiteralPath $stderr -Tail 10
