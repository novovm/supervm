#Requires -Version 7.0
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$work='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\a254-aoem966-existing-20261113'
$root='C:\ProgramData\NOVOVM\releases\aoem-9662470d-c-20261113\aoem'
$exe='C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64\bin\novovm-node.exe'
$config=Join-Path $work 'run.json'
$logs=Join-Path $work 'logs'
$core=Join-Path $root 'windows\core\bin\aoem_ffi.dll'
$manifest=Join-Path $root 'manifest\aoem-manifest.json'
function Hash([string]$p) {(Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant()}
function WriteJson([string]$p,$v){
  if(Test-Path -LiteralPath $p){throw "refusing overwrite: $p"}
  [IO.File]::WriteAllText($p,($v|ConvertTo-Json -Depth 8)+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
}
try {
  if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -cne 'S-1-5-19'){throw 'LocalService required'}
  if((Hash $exe) -cne '1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d'){throw 'Node program changed'}
  if((Hash $core) -cne '5743f5875131330f8c8fec58cdc2a85aa39dc6e9689358609a5f0a09a01c0d52'){throw 'AOEM core changed'}
  $cfg=Get-Content -LiteralPath $config -Raw|ConvertFrom-Json
  if($cfg.database -cne 'C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state' -or $cfg.library -cne $core -or !(Test-Path -LiteralPath $cfg.database -PathType Container)){throw 'Existing config/database differs'}
  if(@(Get-CimInstance Win32_Process -Filter "Name = 'novovm-node.exe'").Count -ne 0){throw 'Another node already running'}
  WriteJson (Join-Path $logs 'owner.json') ([ordered]@{host='C';sid='S-1-5-19';start='existing';node_sha256=(Hash $exe);aoem_sha256=(Hash $core);database=$cfg.database;initialize_executed=$false})
  $env:NOVOVM_AOEM_ROOT=$root
  $env:NOVOVM_AOEM_DLL=$core
  $env:NOVOVM_AOEM_MANIFEST=$manifest
  $env:AOEM_DLL_MANIFEST=$manifest
  $env:AOEM_DLL_MANIFEST_REQUIRED='1'
  $env:NOVOVM_AOEM_RUNTIME_PROFILE=Join-Path $root 'config\aoem-runtime-profile.json'
  $env:NOVOVM_AOEM_PLUGIN_DIR=Join-Path $root 'windows\core\plugins'
  $env:AOEM_SEMANTIC_GRAPH_V2_WORKERS='1'
  $env:NOVOVM_NODE_MODE='native_execution_tick'
  $env:NOVOVM_NATIVE_EXECUTION_CONFIG=$config
  $env:NOVOVM_NATIVE_EXECUTION_START='existing'
  $env:NOVOVM_NATIVE_EXECUTION_RUN_MS='86400000'
  $env:NOVOVM_PROFILE_NATIVE_DELIVERY='1'
  $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH=Join-Path $logs 'progress.json'
  & $exe 1> (Join-Path $logs 'node.stdout') 2> (Join-Path $logs 'node.stderr')
  $exitCode=$LASTEXITCODE
  WriteJson (Join-Path $logs 'exit.json') ([ordered]@{utc=[DateTime]::UtcNow.ToString('o');exit_code=$exitCode;start='existing'})
  exit $exitCode
} catch {
  WriteJson (Join-Path $logs 'owner-error.json') ([ordered]@{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message})
  throw
}
