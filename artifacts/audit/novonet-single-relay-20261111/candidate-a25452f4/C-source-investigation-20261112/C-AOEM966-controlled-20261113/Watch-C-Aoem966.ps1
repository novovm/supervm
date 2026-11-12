#Requires -Version 7.0
#Requires -RunAsAdministrator
param([Parameter(Mandatory=$true)][string]$EvidenceRoot,[ValidateRange(300,10800)][int]$MaximumWaitSeconds=7200)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$exe='C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64\bin\novovm-node.exe'
$core='C:\ProgramData\NOVOVM\releases\aoem-9662470d-c-20261113\aoem\windows\core\bin\aoem_ffi.dll'
$task='NOVOVM-C-a254-AOEM966-Existing-20261113'
$progress='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\a254-aoem966-existing-20261113\logs\progress.json'
$expectedNode='1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d'
$expectedCore='5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(!( $out.StartsWith('D:\WEB3_AI\SUPERVM\target\c-aoem-node-test-20261113\',[StringComparison]::OrdinalIgnoreCase))){throw 'EvidenceRoot outside intended workspace'}
if(Test-Path -LiteralPath $out){throw 'Refusing to overwrite evidence'}
function GetSha([string]$p){(Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant()}
function WriteJson([string]$p,$v){[IO.File]::WriteAllText($p,($v|ConvertTo-Json -Depth 8)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))}
if((GetSha $exe) -cne $expectedNode -or (GetSha $core) -cne $expectedCore){throw 'Program/runtime changed'}
$nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Where-Object ExecutablePath -EQ $exe)
if($nodes.Count -ne 1){throw 'Expected one C node'}
$nodePid=[int]$nodes[0].ProcessId
$loaded=@(Get-Process -Id $nodePid -Module|Where-Object ModuleName -eq 'aoem_ffi.dll')
if($loaded.Count -ne 1 -or $loaded[0].FileName -cne $core){throw 'Expected new AOEM loaded'}
New-Item -ItemType Directory -Path $out|Out-Null
WriteJson (Join-Path $out 'ready.json') ([ordered]@{status='C_AOEM966_OBSERVATION_RUNNING';host='C';utc=[DateTime]::UtcNow.ToString('o');observer_pid=$PID;node_pid=$nodePid;node_sha256=$expectedNode;loaded_aoem_sha256=$expectedCore;maximum_wait_seconds=$MaximumWaitSeconds;read_only=$true})
$timer=[Diagnostics.Stopwatch]::StartNew();$samples=0;$last=$null;$failure=$null
try {
  while($timer.Elapsed.TotalSeconds -lt $MaximumWaitSeconds){
    $process=Get-Process -Id $nodePid -ErrorAction SilentlyContinue
    $t=Get-ScheduledTask -TaskName $task -ErrorAction Stop
    $ti=Get-ScheduledTaskInfo -TaskName $task -ErrorAction Stop
    $ports=@(Get-NetUDPEndpoint|Where-Object {$_.OwningProcess -eq $nodePid -and $_.LocalPort -in 41052,42052})
    $p=$null
    if(Test-Path -LiteralPath $progress -PathType Leaf){
      try {$p=Get-Content -LiteralPath $progress -Raw|ConvertFrom-Json}catch{$p=$null}
    }
    $last=[ordered]@{utc=[DateTime]::UtcNow.ToString('o');elapsed_seconds=[math]::Round($timer.Elapsed.TotalSeconds,1);node_alive=($null -ne $process);task_state=$t.State.ToString();task_result=[long]$ti.LastTaskResult;port_count=$ports.Count;chain_id=$p.chain_id;height=$p.current_height;head=$p.head;execution_failures=$p.execution_failures;recovery_in_progress=$p.recovery_in_progress}
    WriteJson (Join-Path $out 'liveness.json') $last
    $samples++
    if($null -eq $process -or $t.State -ne 'Running' -or $ports.Count -ne 2 -or $p.chain_id -ne 91129655606029 -or $p.execution_failures -ne 0 -or $p.recovery_in_progress -ne $false){$failure='health_invariant_failed';break}
    Start-Sleep -Seconds 30
  }
} catch {$failure=$_.Exception.Message}
WriteJson (Join-Path $out 'result.json') ([ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');status=$(if($failure){'FAILED'}else{'BOUNDED_WINDOW_PASSED'});failure=$failure;elapsed_seconds=[math]::Round($timer.Elapsed.TotalSeconds,1);samples=$samples;last=$last;root_cause_proven=$false;node_mutated=$false})
if($failure){exit 1}
