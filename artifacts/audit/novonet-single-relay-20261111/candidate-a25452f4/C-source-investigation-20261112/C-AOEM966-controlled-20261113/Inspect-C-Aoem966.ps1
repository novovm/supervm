#Requires -Version 7.0
#Requires -RunAsAdministrator
param([Parameter(Mandatory=$true)][string]$OutFile)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
trap {
  [IO.File]::WriteAllText($OutFile+'.error.txt',$_.Exception.ToString(),[Text.UTF8Encoding]::new($false))
  exit 1
}
$task='NOVOVM-C-a254-AOEM966-Existing-20261113'
$work='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\a254-aoem966-existing-20261113'
$logs=Join-Path $work 'logs'
$configPath=Join-Path $work 'run.json'
$nodePath='C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64\bin\novovm-node.exe'
$dllPath='C:\ProgramData\NOVOVM\releases\aoem-9662470d-c-20261113\aoem\windows\core\bin\aoem_ffi.dll'
function GetSha([string]$p){if(Test-Path -LiteralPath $p -PathType Leaf){(Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant()}else{$null}}
$t=Get-ScheduledTask -TaskName $task -ErrorAction Stop
$ti=Get-ScheduledTaskInfo -TaskName $task -ErrorAction Stop
$nodes=@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'"|Select-Object ProcessId,ExecutablePath,CreationDate,CommandLine)
$ports=@(Get-NetUDPEndpoint|Where-Object LocalPort -in 41052,42052|Select-Object LocalAddress,LocalPort,OwningProcess)
$modules=@()
foreach($node in $nodes){
  try {$modules+=@(Get-Process -Id $node.ProcessId -Module -ErrorAction Stop|Where-Object ModuleName -eq 'aoem_ffi.dll'|ForEach-Object {[ordered]@{pid=$node.ProcessId;path=$_.FileName;sha256=GetSha $_.FileName}})}catch{}
}
$progress=$null
$p=Join-Path $logs 'progress.json'
if(Test-Path -LiteralPath $p -PathType Leaf){
  try {
    $raw=Get-Content -LiteralPath $p -Raw|ConvertFrom-Json
    $progress=[ordered]@{entry=$raw.entry;chain_id=$raw.chain_id;current_height=$raw.current_height;head=$raw.head;execution_failures=$raw.execution_failures;recovery_in_progress=$raw.recovery_in_progress;bytes=(Get-Item -LiteralPath $p).Length;sha256=GetSha $p}
  } catch {$progress=[ordered]@{parse_error=$_.Exception.Message;bytes=(Get-Item -LiteralPath $p).Length}}
}
$exit=$null;$e=Join-Path $logs 'exit.json';if(Test-Path -LiteralPath $e -PathType Leaf){$exit=Get-Content -LiteralPath $e -Raw|ConvertFrom-Json}
$ownerError=$null;$oe=Join-Path $logs 'owner-error.json';if(Test-Path -LiteralPath $oe -PathType Leaf){$ownerError=Get-Content -LiteralPath $oe -Raw|ConvertFrom-Json}
$stderr=Join-Path $logs 'node.stderr';$stderrLength=if(Test-Path -LiteralPath $stderr -PathType Leaf){(Get-Item -LiteralPath $stderr).Length}else{$null}
$cfg=Get-Content -LiteralPath $configPath -Raw|ConvertFrom-Json
$oldProgress=$null
$oldProgressPath='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-observation-a25452f4-20261112\logs\progress.json'
if(Test-Path -LiteralPath $oldProgressPath -PathType Leaf){
  try {$old=Get-Content -LiteralPath $oldProgressPath -Raw|ConvertFrom-Json;$oldProgress=[ordered]@{chain_id=$old.chain_id;current_height=$old.current_height;head=$old.head;execution_failures=$old.execution_failures;recovery_in_progress=$old.recovery_in_progress;sha256=GetSha $oldProgressPath}}catch{$oldProgress=[ordered]@{parse_error=$_.Exception.Message}}
}
$report=[ordered]@{host='C';utc=[DateTime]::UtcNow.ToString('o');task_state=$t.State.ToString();task_result=[long]$ti.LastTaskResult;task_user=$t.Principal.UserId;nodes=@($nodes|ForEach-Object {[ordered]@{pid=$_.ProcessId;path=$_.ExecutablePath;created=$_.CreationDate;sha256=GetSha $_.ExecutablePath}});ports=$ports;modules=$modules;configured_library=$cfg.library;configured_library_sha256=GetSha $cfg.library;original_database_exists=(Test-Path -LiteralPath $cfg.database -PathType Container);progress=$progress;old_progress=$oldProgress;exit=$exit;owner_error=$ownerError;stderr_bytes=$stderrLength}
if(Test-Path -LiteralPath $OutFile){throw 'Refusing to overwrite evidence'}
[IO.File]::WriteAllText($OutFile,($report|ConvertTo-Json -Depth 16)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
$report|ConvertTo-Json -Depth 16
