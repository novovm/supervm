#Requires -Version 7
#Requires -RunAsAdministrator
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$base=$PSScriptRoot;$out=Join-Path $base 'attach-control-01';$task='NOVOVM-C-Ttd-AttachControl-20261113-01'
$pwsh='C:\Program Files\PowerShell\7\pwsh.exe';$tools='D:\WEB3_AI\SUPERVM\target\c-first-fault-20261113\ttd-tools'
$recorder=$null;$registered=$false
function Save($name,$value){$p=Join-Path $out $name;if(Test-Path -LiteralPath $p){throw 'Evidence exists'};$value|ConvertTo-Json -Depth 8|Set-Content -LiteralPath ($p+'.tmp');Move-Item -LiteralPath ($p+'.tmp') -Destination $p}
try{
 if(Test-Path -LiteralPath $out){throw 'Evidence exists; no retry'}
 New-Item -ItemType Directory -Path $out|Out-Null
 & icacls.exe $out /inheritance:r /grant:r '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' '*S-1-5-19:(OI)(CI)M' ('*'+[Security.Principal.WindowsIdentity]::GetCurrent().User.Value+':(OI)(CI)F')|Out-Null
 if($LASTEXITCODE -ne 0){throw 'ACL failed'}
 $fixture=Join-Path $tools 'ttd-pointer-control-aligned.exe'
 if((Get-FileHash -LiteralPath $fixture).Hash -ine '9c7848f47daced72a3202ce9970b3931ba3f89318d707f0926a9b0498bf3c595'){throw 'Fixture hash mismatch'}
 $ttd=Join-Path $tools 'x64\TTD.exe'
 if((Get-FileHash -LiteralPath $ttd).Hash -ine '652868a0f5b20430c4fe9da3ef004c946af90264c5ca0e61f482a1ac48ae201b' -or (Get-AuthenticodeSignature -LiteralPath $ttd).Status -ne 'Valid'){throw 'Recorder mismatch'}
 Copy-Item -LiteralPath $fixture,(Join-Path $base 'SuspendedControlOwner.ps1') -Destination $out
 $action=New-ScheduledTaskAction -Execute $pwsh -Argument ('-NoProfile -NonInteractive -File "'+(Join-Path $out 'SuspendedControlOwner.ps1')+'" -EvidenceRoot "'+$out+'"')
 $principal=New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\LOCAL SERVICE' -LogonType ServiceAccount
 $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 10) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
 if(Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue){throw 'Task exists'}
 Register-ScheduledTask -TaskName $task -Action $action -Principal $principal -Settings $settings|Out-Null;$registered=$true
 Start-ScheduledTask -TaskName $task
 $results=@()
 foreach($mode in 'clean','bad'){
  $clock=[Diagnostics.Stopwatch]::StartNew();$childPath=Join-Path $out "$mode-child.json"
  while(!(Test-Path -LiteralPath $childPath)){if($clock.Elapsed.TotalSeconds -gt 150 -or (Test-Path -LiteralPath (Join-Path $out 'owner-error.json'))){throw 'Child unavailable'};Start-Sleep -Milliseconds 200}
  $child=Get-Content -LiteralPath $childPath -Raw|ConvertFrom-Json
  $c=Get-CimInstance Win32_Process -Filter "ProcessId=$($child.pid)"
  if(!$c -or $c.ExecutablePath -ine $child.path -or $c.CreationDate.ToUniversalTime().Ticks -ne ([DateTime]$child.created_utc).ToUniversalTime().Ticks -or (Invoke-CimMethod $c -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Attach target mismatch'}
  $stdout=Join-Path $out "$mode-recorder.stdout";$stderr=Join-Path $out "$mode-recorder.stderr"
  $recorder=Start-Process -FilePath $ttd -ArgumentList @('-accepteula','-noUI','-maxFile','128','-out',('"'+(Join-Path $out "$mode.run")+'"'),'-attach',"$($child.pid)") -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
  Save "$mode-recorder.json" @{pid=$recorder.Id;created_utc=$recorder.StartTime.ToUniversalTime().ToString('o');attached_pid=$child.pid;node_started=$false}
  $clock.Restart()
  while($true){
   $raw=Get-Content -LiteralPath $stdout -Raw -ErrorAction SilentlyContinue
   if($raw -match "Recording has started of process \(PID:$($child.pid)\)"){break}
   if($recorder.HasExited -or $clock.Elapsed.TotalSeconds -gt 75){throw 'Recorder not ready within bound'}
   Start-Sleep -Milliseconds 200
  }
  Save "$mode-resume.json" @{utc=[DateTime]::UtcNow.ToString('o');recording_started=$true;pid=$child.pid}
  if(!$recorder.WaitForExit(130000)){throw 'Recorder execution bound reached'}
  $recorder.Refresh();$rc=$recorder.ExitCode;$recorder=$null
  $exitFile=Join-Path $out "$mode-guest-exit.json";$clock.Restart()
  while(!(Test-Path -LiteralPath $exitFile)){if($clock.Elapsed.TotalSeconds -gt 15){throw 'Kernel exit evidence missing'};Start-Sleep -Milliseconds 100}
  $guest=Get-Content -LiteralPath $exitFile -Raw|ConvertFrom-Json
  $expected=if($mode -eq 'clean'){0}else{-1073741819}
  $results+=@{mode=$mode;exit_code=$guest.exit_code;recorder_exit=$rc}
  if($rc -ne 0 -or $guest.exit_code -ne $expected){throw 'Control exit mismatch'}
 }
 Save 'result-public.json' @{utc=[DateTime]::UtcNow.ToString('o');results=$results;node_started=$false;recording_mode='administrator attach to suspended LocalService child'}
 & 'D:\Anaconda3\python.exe' (Join-Path $base 'Verify-RecordedControl.py') $out *> (Join-Path $out 'replay-private.log')
 if($LASTEXITCODE -ne 0){throw 'Reverse replay control failed'}
 Save 'complete-public.json' @{utc=[DateTime]::UtcNow.ToString('o');exit_code=0;node_started=$false;root_cause_fixed=$false};exit 0
}catch{if(Test-Path -LiteralPath $out){Save 'error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;node_started=$false}};exit 1}
finally{
 if(Test-Path -LiteralPath $out){Save 'stop.json' @{utc=[DateTime]::UtcNow.ToString('o')};Start-Sleep -Seconds 2}
 if($recorder -and !$recorder.HasExited){if(!$recorder.WaitForExit(15000)){$recorder.Kill();$recorder.WaitForExit()}}
 if($registered){$t=Get-ScheduledTask -TaskName $task;Save 'task-end-public.json' @{state=$t.State.ToString();result=(Get-ScheduledTaskInfo -TaskName $task).LastTaskResult};if($t.State -ne 'Running'){Unregister-ScheduledTask -TaskName $task -Confirm:$false}}
}
