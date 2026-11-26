#Requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Client,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ClientSha256,
    [Parameter(Mandatory)][string]$Profile,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ProfileSha256,
    [Parameter(Mandatory)][string]$IsolatedLocalAppData,
    [Parameter(Mandatory)][string]$EvidenceDir,
    [ValidateRange(30,900)][int]$Seconds = 240
)
$ErrorActionPreference = 'Stop'
$clientPath = (Get-Item -LiteralPath $Client).FullName
$profilePath = (Get-Item -LiteralPath $Profile).FullName
if ((Get-FileHash -LiteralPath $clientPath).Hash -ine $ClientSha256 -or
    (Get-FileHash -LiteralPath $profilePath).Hash -ine $ProfileSha256) {
    throw 'Executable or profile digest mismatch'
}
$data = Get-Item -LiteralPath $IsolatedLocalAppData
if (!$data.PSIsContainer -or ($data.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
    (Split-Path $data.FullName -Parent) -ine [IO.Path]::GetFullPath($env:LOCALAPPDATA) -or
    $data.Name -notlike 'NOVOVM-Roaming-*') {
    throw 'Use a dedicated NOVOVM-Roaming-* directory directly under LocalAppData'
}
if (Test-Path -LiteralPath $EvidenceDir) { throw 'Existing evidence directory preserved' }
$out = New-Item -ItemType Directory -Path $EvidenceDir
$p = $null
$start = $null
$stopped = $false
$reason = 'launch_failed'
try {
    $p = Start-Process -FilePath $clientPath -ArgumentList @(
        ('"{0}"' -f $profilePath), 'follow-state', $ProfileSha256
    ) -Environment @{ LOCALAPPDATA = $data.FullName } -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput (Join-Path $out.FullName 'stdout.jsonl') `
        -RedirectStandardError (Join-Path $out.FullName 'stderr.log')
    $start = $p.StartTime.ToUniversalTime().ToString('o')
    @{ pid = $p.Id; utc = $start; client_sha256 = $ClientSha256; profile_sha256 = $ProfileSha256 } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $out.FullName 'start.json')
    Write-Output "Capture running: PID=$($p.Id), evidence=$($out.FullName)"
    $elapsed = [Diagnostics.Stopwatch]::StartNew()
    while ($elapsed.Elapsed.TotalSeconds -lt $Seconds -and !$p.HasExited -and
        !(Test-Path -LiteralPath (Join-Path $out.FullName 'stop.request'))) {
        Start-Sleep -Milliseconds 250
    }
    $reason = if ($p.HasExited) { 'client_exited' }
        elseif ($elapsed.Elapsed.TotalSeconds -ge $Seconds) { 'deadline' }
        else { 'operator_stop' }
    if ($reason -eq 'client_exited') { throw "Client exited before capture stop: $($p.ExitCode)" }
} finally {
    if ($p -and !$p.HasExited) {
        $p.Kill()
        $stopped = $p.WaitForExit(5000)
    }
    @{ pid = if ($p) { $p.Id }; started_utc = $start; ended_utc = [datetime]::UtcNow.ToString('o')
        reason = $reason; stopped_owned_process = $stopped } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $out.FullName 'end.json')
}
