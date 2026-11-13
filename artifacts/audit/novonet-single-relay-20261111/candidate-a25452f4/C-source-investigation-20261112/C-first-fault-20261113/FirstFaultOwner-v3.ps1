#Requires -Version 7.0
param([Parameter(Mandatory)][string]$WorkRoot,[Parameter(Mandatory)][string]$ToolRoot)
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
function Save($name,$value){$p=Join-Path $WorkRoot $name;if(Test-Path -LiteralPath $p){throw 'Evidence exists'}; $value|ConvertTo-Json -Depth 8|Set-Content -LiteralPath $p}
function AssertPrivateDump([string]$Path){
 if(!(Test-Path -LiteralPath $Path -PathType Leaf)){throw 'Full dump missing'}
 $file=Get-Item -LiteralPath $Path
 if($file.Length -lt 32){throw 'Full dump truncated'}
 $stream=[IO.File]::OpenRead($Path)
 try{ $header=[byte[]]::new(32);if($stream.Read($header,0,32) -ne 32){throw 'Dump header incomplete'} }finally{$stream.Dispose()}
 if([Text.Encoding]::ASCII.GetString($header,0,4) -cne 'MDMP'){throw 'Dump signature invalid'}
 if(([BitConverter]::ToUInt64($header,24) -band 2) -eq 0){throw 'Dump lacks full memory flag'}
 return @{bytes=$file.Length;sha256=(Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant();full_memory=$true}
}
function DebugOnce([string]$name,[string]$exe,[string]$extra,[int]$seconds){
 $commands=Join-Path $WorkRoot ($name+'.cdb')
 $dump=Join-Path $WorkRoot ($name+'-private.dmp')
 # Backslashes inside CDB event-command strings are escape characters.
 $dumpCommandPath=([IO.Path]::GetFullPath($dump)).Replace('\','/')
 if($dumpCommandPath -cnotmatch '^[A-Za-z]:/[A-Za-z0-9_./-]+$'){throw 'Unsafe CDB dump path'}
 @"
.sympath $ToolRoot
.echo C_DIAG_INITIAL_BREAK
.echo PAGEHEAP_VERIFIED_FROM_LOADER_EVENT
sxe -c ".echo C_FIRST_FAULT; .exr -1; .ecxr; r; kv; !heap -p -a @rdx; !heap -p -a @rcx; .dump /ma $dumpCommandPath; q" av
sxe -c ".echo C_FIRST_VERIFIER_STOP; .exr -1; r; kv; .dump /ma $dumpCommandPath; q" bpe
sxe -c ".echo C_TARGET_EXIT; .lastevent; q" epr
.echo C_DIAG_READY
g
"@ | Set-Content -LiteralPath $commands
 $cdb=Join-Path $ToolRoot 'cdb.exe'
 $p=Start-Process -FilePath $cdb -ArgumentList @('-cf',('"'+$commands+'"'),('"'+$exe+'"'),$extra) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $WorkRoot ($name+'-private.stdout')) -RedirectStandardError (Join-Path $WorkRoot ($name+'-private.stderr'))
 Save ($name+'-debugger.json') @{pid=$p.Id;created_utc=$p.StartTime.ToUniversalTime().ToString('o');path=$cdb;max_seconds=$seconds}
 $deadline=[DateTime]::UtcNow.AddSeconds($seconds);$reason='debugger_exited'
 while(!$p.WaitForExit(2000)){
  $children=@(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($p.Id)" | Where-Object Name -eq ([IO.Path]::GetFileName($exe)))
  $tooLarge=$false
  foreach($child in $children){$proc=Get-Process -Id $child.ProcessId -ErrorAction SilentlyContinue;if($proc -and $proc.PrivateMemorySize64 -gt 6GB){$tooLarge=$true}}
  if([DateTime]::UtcNow -gt $deadline -or $tooLarge -or (Get-PSDrive D).Free -lt 10GB){
   $reason=if($tooLarge){'memory_bound'}elseif((Get-PSDrive D).Free -lt 10GB){'disk_bound'}else{'time_bound'}
   # The debugger owns exactly this child; default kill-on-debugger-exit stops it.
   if($p.StartTime.ToUniversalTime() -ne (Get-Process -Id $p.Id).StartTime.ToUniversalTime()){throw 'Debugger PID reused'}
   Stop-Process -Id $p.Id -Force; $p.WaitForExit();break
  }
 }
 Save ($name+'-debugger-exit.json') @{utc=[DateTime]::UtcNow.ToString('o');debugger_exit=$p.ExitCode;reason=$reason;node_exit_not_inferred=$true}
 if($reason -ne 'debugger_exited'){throw "Diagnostic stopped: $reason"}
 return Get-Content -LiteralPath (Join-Path $WorkRoot ($name+'-private.stdout')) -Raw
}
try{
 if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -cne 'S-1-5-19'){throw 'LocalService required'}
 $control=Join-Path $ToolRoot 'c-pageheap-control.exe'
 $clean=DebugOnce 'control-clean' $control 'clean' 60
 if($clean -notmatch '(?m)^CONTROL_CLEAN_OK\r?$' -or $clean -notmatch '(?m)^Page heap: pid 0x[0-9a-fA-F]+: page heap enabled with flags 0x3\.' -or $clean -notmatch 'Exit process .*code 0' -or $clean -match '(?m)^C_FIRST_(FAULT|VERIFIER_STOP)\r?$'){throw 'Clean control failed'}
 $bad=DebugOnce 'control-overrun' $control 'overrun' 60
 if($bad -notmatch '(?m)^C_FIRST_FAULT\r?$' -or $bad -notmatch '(?m)^CONTROL_BEFORE_OVERRUN\r?$' -or $bad -match '(?m)^CONTROL_OVERRUN_MISSED\r?$' -or $bad -notmatch 'c0000005'){throw 'Overrun control not caught'}
 $controlDump=AssertPrivateDump (Join-Path $WorkRoot 'control-overrun-private.dmp')
 Save 'control-dump-public.json' $controlDump
 Save 'controls-public.json' @{clean='PASS';deliberate_overrun_caught='PASS';fixture_only=$true;sid='S-1-5-19';utc=[DateTime]::UtcNow.ToString('o')}
 $cfg=Get-Content (Join-Path $WorkRoot 'run.json') -Raw|ConvertFrom-Json
 $node='C:\ProgramData\NOVOVM\releases\novonet-observation-a25452f4-windows-x86_64\bin\novovm-node.exe'
 $root='C:\ProgramData\NOVOVM\releases\aoem-9662470d-c-20261113\aoem'
 if($cfg.database -cne 'C:\ProgramData\NOVOVM\selfrun\91129655606029\C\chain-state'){throw 'Wrong original database'}
 if((Get-FileHash $node).Hash -cne '1758E4A79DD34D1688D8F7BF4B60F46ED9CED518186F42FDB16BAF04C0D0C46D'){throw 'Node changed'}
 if((Get-FileHash $cfg.library).Hash -cne '5743F5875131330F8C8FEC58CDC2A85AA39DC6E9689358609A5F0A09A01C0D52'){throw 'Runtime changed'}
 if(@(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'").Count){throw 'Node already present'}
 $env:NOVOVM_AOEM_ROOT=$root;$env:NOVOVM_AOEM_DLL=$cfg.library
 $env:NOVOVM_AOEM_MANIFEST=Join-Path $root 'manifest\aoem-manifest.json';$env:AOEM_DLL_MANIFEST=$env:NOVOVM_AOEM_MANIFEST;$env:AOEM_DLL_MANIFEST_REQUIRED='1'
 $env:NOVOVM_AOEM_RUNTIME_PROFILE=Join-Path $root 'config\aoem-runtime-profile.json';$env:NOVOVM_AOEM_PLUGIN_DIR=Join-Path $root 'windows\core\plugins'
 $env:AOEM_SEMANTIC_GRAPH_V2_WORKERS='1';$env:NOVOVM_NODE_MODE='native_execution_tick'
 $env:NOVOVM_NATIVE_EXECUTION_CONFIG=Join-Path $WorkRoot 'run.json';$env:NOVOVM_NATIVE_EXECUTION_START='existing'
 $env:NOVOVM_NATIVE_EXECUTION_RUN_MS='1800000';$env:NOVOVM_PROFILE_NATIVE_DELIVERY='1';$env:RUST_BACKTRACE='full'
 $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH=Join-Path $WorkRoot 'progress-private.json'
 $text=DebugOnce 'node' $node '' 1920
 $kind=if($text -match '(?m)^C_FIRST_FAULT\r?$'){'FIRST_CHANCE_AV'}elseif($text -match '(?m)^C_FIRST_VERIFIER_STOP\r?$'){'VERIFIER_STOP'}elseif($text -match '(?m)^C_TARGET_EXIT\r?$'){'TARGET_EXIT'}else{'UNKNOWN_STOP'}
 if($kind -in 'FIRST_CHANCE_AV','VERIFIER_STOP'){Save 'node-dump-public.json' (AssertPrivateDump (Join-Path $WorkRoot 'node-private.dmp'))}
 Save 'result-public.json' @{status=$kind;utc=[DateTime]::UtcNow.ToString('o');start='existing';root_cause_fixed=$false;relay_ready=$false;no_retry=$true}
 exit 0
}catch{Save 'error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;root_cause_fixed=$false;relay_ready=$false};exit 1}
