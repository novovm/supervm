param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$parentRoot='D:\WEB3_AI\SUPERVM\target\c-pt-live-qualification-20261115'
$resolved=[IO.Path]::GetFullPath($EvidenceRoot)
if (-not $resolved.StartsWith($parentRoot+'\run-', [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected evidence root' }
if (Test-Path -LiteralPath $resolved) { throw 'Evidence root must be new' }
$isAdmin=([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) { throw 'Elevated PowerShell required' }
New-Item -ItemType Directory -Path $resolved | Out-Null
$startUtc=[DateTime]::UtcNow
$instance='C_PT_FILE_'+$startUtc.ToString('yyyyMMddTHHmmss')
$events=[Collections.Generic.List[object]]::new()
$result=[ordered]@{schemaVersion=2; mode='File'; rawEtwLimitBytes=536870912; baselineAlsoUsesFaultMode=$true; startUtc=$startUtc.ToString('o'); instanceName=$instance; originalNodeStarted=$false; productionChanged=$false; traceStartSucceeded=$false; traceStopSucceeded=$false; actualPtDecoded=$false; qualificationPassed=$false; error=$null}
$traceAttempted=$false
$traceTemp=Join-Path $resolved 'wpr-temp'
New-Item -ItemType Directory -Path $traceTemp | Out-Null
$wpr='C:\Windows\System32\wpr.exe'
function Save-Result {
  $result['endUtc']=[DateTime]::UtcNow.ToString('o')
  $result['commands']=$events.ToArray()
  $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $resolved 'result.json') -Encoding utf8
}
function Invoke-Bounded([string]$Tag,[string]$Executable,[string[]]$Arguments,[int]$TimeoutSeconds) {
  $psi=[Diagnostics.ProcessStartInfo]::new()
  $psi.FileName=$Executable; $psi.WorkingDirectory=$resolved; $psi.UseShellExecute=$false
  $psi.CreateNoWindow=$true; $psi.RedirectStandardOutput=$true; $psi.RedirectStandardError=$true
  foreach($arg in $Arguments){$psi.ArgumentList.Add($arg)}
  $p=[Diagnostics.Process]::new(); $p.StartInfo=$psi
  $began=[DateTime]::UtcNow
  if(-not $p.Start()){throw "Start failed: $Tag"}
  $out=$p.StandardOutput.ReadToEndAsync(); $err=$p.StandardError.ReadToEndAsync()
  $created=$p.StartTime.ToUniversalTime().ToString('o')
  $deadline=[DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  $didTimeout=$false; $resourceBound=$false
  $commandError=$null
  try { while(-not $p.WaitForExit(250)) {
    if([DateTime]::UtcNow -ge $deadline){$didTimeout=$true; break}
    if($result.traceStartSucceeded -and $Tag -ne 'wpr-stop' -and $Tag -ne 'wpr-after') {
      $size=0L
      foreach($part in Get-ChildItem -LiteralPath $traceTemp -File -Recurse -ErrorAction Stop){$size+=$part.Length}
      if($size -gt 512MB -or (Get-PSDrive -Name D).Free -lt 8GB){
        $result['resourceGuard']=[ordered]@{utc=[DateTime]::UtcNow.ToString('o'); bytes=$size; limitBytes=536870912; action='terminate owned qualification command then save own WPR instance'}
        $resourceBound=$true; break
      }
    }
  }} catch { $commandError=$_.ToString() }
  finally {
    if(-not $p.HasExited){$p.Kill($false); if(-not $p.WaitForExit(10000)){throw 'Owned command did not exit after bounded termination'}}
  }
  $stdout=$out.GetAwaiter().GetResult(); $stderr=$err.GetAwaiter().GetResult()
  [IO.File]::WriteAllText((Join-Path $resolved ($Tag+'.stdout.txt')),$stdout)
  [IO.File]::WriteAllText((Join-Path $resolved ($Tag+'.stderr.txt')),$stderr)
  $actualExit=$p.ExitCode
  $cpuSeconds=$null; $peakWorkingSet=$null
  try{$cpuSeconds=$p.TotalProcessorTime.TotalSeconds}catch{}
  try{$peakWorkingSet=$p.PeakWorkingSet64}catch{}
  $row=[ordered]@{tag=$Tag; executable=$Executable; arguments=$Arguments; pid=$p.Id; processCreatedUtc=$created; startUtc=$began.ToString('o'); endUtc=[DateTime]::UtcNow.ToString('o'); exitCode=$actualExit; timedOut=$didTimeout; resourceBound=$resourceBound; commandError=$commandError; cpuSeconds=$cpuSeconds; peakWorkingSetBytes=$peakWorkingSet}
  $events.Add($row); Save-Result
  if($didTimeout -or $resourceBound){throw "Bound exceeded: $Tag"}
  if($commandError){throw $commandError}
  return [pscustomobject]@{exitCode=$p.ExitCode; stdout=$stdout; stderr=$stderr}
}
try {
  $config=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'qualification-config.json') -Raw | ConvertFrom-Json
  foreach($file in @($config.exe,$config.profile)) {
    if((Get-FileHash -LiteralPath $file.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256){throw 'Input hash mismatch'}
  }
  if(@(Get-Process -Name novovm-node,cdb -ErrorAction SilentlyContinue).Count -gt 0){throw 'Unexpected node or debugger present; stop without touching it'}
  if((Get-PSDrive -Name D).Free -lt 8GB){throw 'Less than 8 GiB free'}
  $pre=Invoke-Bounded 'wpr-before' $wpr @('-status') 20
  if($pre.exitCode -ne 0 -or $pre.stdout -notmatch 'WPR is not recording'){throw 'An existing or unreadable WPR recording is present; not changing it'}
  $baseline=Invoke-Bounded 'baseline' $config.exe.path @('--fault',(Join-Path $resolved 'baseline-target')) 40
  if($baseline.exitCode -ne -532528300){throw 'Qualification baseline did not return declared exception exit'}
  $traceAttempted=$true
  $start=Invoke-Bounded 'wpr-start' $wpr @('-start',($config.profile.path+'!C_PT_File.Light'),'-filemode','-recordtempto',$traceTemp,'-instancename',$instance) 30
  if($start.exitCode -ne 0){throw ('WPR PT actual start failed: '+$start.exitCode)}
  $result.traceStartSucceeded=$true
  $status=Invoke-Bounded 'wpr-active' $wpr @('-status','collectors','-details','-instancename',$instance) 20
  if($status.exitCode -ne 0 -or $status.stdout -notmatch 'Logging mode\s*:\s*File'){throw 'Cannot confirm File trace active'}
  $target=Invoke-Bounded 'traced-fault' $config.exe.path @('--fault',(Join-Path $resolved 'traced-target')) 45
  $result['qualificationProcessExitCode']=$target.exitCode
  if($target.exitCode -ne -532528300){throw 'Traced target did not return declared exception exit'}
  $result['faultDumpPresent']=Test-Path -LiteralPath (Join-Path $resolved 'traced-target\qualification-full.dmp')
  $result['faultContextPresent']=Test-Path -LiteralPath (Join-Path $resolved 'traced-target\result.json')
  Invoke-Bounded 'wpr-counters' $wpr @('-status','collectors','-details','-instancename',$instance) 20 | Out-Null
} catch {
  $result.error=$_.ToString()
} finally {
  if($traceAttempted){
    try {
      if((Get-PSDrive -Name D).Free -lt 2GB){throw 'Cannot safely merge; retain raw temporary trace and stop own instance without merge'}
      $stop=Invoke-Bounded 'wpr-stop' $wpr @('-stop',(Join-Path $resolved 'qualification.etl'),'-skipPdbGen','-instancename',$instance) 60
      $result.traceStopSucceeded=($stop.exitCode -eq 0)
    } catch { $result['stopError']=$_.ToString() }
    if(-not $result.traceStopSucceeded) {
      try {
        # Cancel may remove WPR temporary files: preserve them first in this same evidence directory.
        $preserved=Join-Path $resolved 'raw-before-own-cancel'
        New-Item -ItemType Directory -Path $preserved | Out-Null
        foreach($raw in Get-ChildItem -LiteralPath $traceTemp -File -Recurse){
          $relative=[IO.Path]::GetRelativePath($traceTemp,$raw.FullName)
          $destination=Join-Path $preserved $relative
          [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
          [IO.File]::Copy($raw.FullName,$destination,$false)
        }
      } catch { $result['rawPreservationError']=$_.ToString() }
      try { $cancel=Invoke-Bounded 'wpr-own-cancel' $wpr @('-cancel','-instancename',$instance) 30; $result['ownCancelExit']=$cancel.exitCode }
      catch { $result['ownCancelError']=$_.ToString() }
    }
    try {
      $own=Invoke-Bounded 'wpr-own-after' $wpr @('-status','-instancename',$instance) 20
      $result['ownTraceAbsent']=($own.stdout -match 'WPR is not recording')
      if(-not $result.ownTraceAbsent){$result['cleanupUnverified']=$true}
    } catch { $result['ownStatusError']=$_.ToString() }
  }
  try { Invoke-Bounded 'wpr-after' $wpr @('-status') 20 | Out-Null } catch { $result['statusError']=$_.ToString() }
  Save-Result
}
if($result.error -or -not $result.traceStopSucceeded){exit 1}
exit 0
