#Requires -Version 7.0
#Requires -RunAsAdministrator
param()
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$root='D:\WEB3_AI\SUPERVM\target\c-first-fault-20261113\ttd-tools'
$out=Join-Path $root 'control-02'
if(Test-Path -LiteralPath $out){throw 'Evidence exists; no rerun or overwrite'}
New-Item -ItemType Directory -Path $out | Out-Null
$started=[DateTime]::UtcNow.ToString('o')
$results=[Collections.Generic.List[object]]::new()
try {
    $ttd=Join-Path $root 'x64\TTD.exe'
    $fixture=Join-Path $root 'ttd-pointer-control-aligned.exe'
    if((Get-AuthenticodeSignature -LiteralPath $ttd).Status -ne 'Valid'){throw 'Recorder signature invalid'}
    foreach($mode in @('clean','bad')) {
        $args=@('-accepteula','-noUI','-maxFile','128','-passThroughExit','-out',('"'+(Join-Path $out ($mode+'.run'))+'"'),'-launch',('"'+$fixture+'"'),$mode)
        $p=Start-Process -FilePath $ttd -ArgumentList $args -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $out ($mode+'-stdout.txt')) -RedirectStandardError (Join-Path $out ($mode+'-stderr.txt'))
        if(!$p.WaitForExit(120000)) {
            $children=@(Get-CimInstance Win32_Process -Filter "Name='ttd-pointer-control-aligned.exe'" | Where-Object { $_.ExecutablePath -ieq $fixture -and $_.ParentProcessId -eq $p.Id })
            foreach($child in $children){Stop-Process -Id $child.ProcessId -Force}
            if(!$p.HasExited){$p.Kill();$p.WaitForExit()}
            throw "Isolated $mode timed out; not retried"
        }
        $p.Refresh()
        $code=$p.ExitCode
        $results.Add(@{mode=$mode;recorder_pid=$p.Id;exit_code=$code;utc=[DateTime]::UtcNow.ToString('o');trace_exists=(Test-Path -LiteralPath (Join-Path $out ($mode+'.run')))})
        $expected=if($mode -eq 'clean'){0}else{-1073741819}
        if($code -ne $expected){throw "Unexpected isolated $mode exit $code; expected $expected"}
    }
    @{start_utc=$started;end_utc=[DateTime]::UtcNow.ToString('o');results=@($results.ToArray());recording_control_completed=$true;replay_verified=$false;node_started=$false}|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $out 'result-public.json')
    exit 0
} catch {
    @{start_utc=$started;end_utc=[DateTime]::UtcNow.ToString('o');results=@($results.ToArray());error=$_.Exception.Message;node_started=$false}|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $out 'error-public.json')
    exit 1
}
