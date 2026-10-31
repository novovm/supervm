#requires -Version 7.0
[CmdletBinding()]
param([switch]$ValidateOnly)

$ErrorActionPreference = 'Stop'
$taskRepo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../../..')).Path
$taskFiles = @(
    @{ role='node'; path=(Join-Path $taskRepo 'target/native-controller-throughput-20261031/candidate-v4/novovm-node.exe'); sha='6ed28e989a0d11a78a4943e57e437de112bd7261b1ea57b7c87c282b53200a74' },
    @{ role='driver'; path=(Join-Path $taskRepo 'target/native-controller-throughput-20261031/stability-observer/native-tests.exe'); sha='647a058adb28dbc1b6a134bbaace33f8a942ea6c2fb5b38abb4e69b335025ecb' },
    @{ role='aoem'; path='D:/WorksArea/AOEM/artifacts/mldsa-native-paired-20261027/aoem_ffi.dll'; sha='8779d014f08dae0069830ade207aad083173cf963734fe9689b55b8a72455671' },
    @{ role='node_symbols'; path=(Join-Path $taskRepo 'target/runtime-rebuild/release/novovm_node.pdb'); sha='76832311a5279cab395bad164d446f2bd97c4bb47a5a864795b4d8671ae18dfe' },
    @{ role='driver_symbols'; path=(Join-Path $taskRepo 'target/runtime-rebuild/release/deps/novovm_node-9c742166f7d138ac.pdb'); sha='c38757c548dad6179f4c431ef4bc279087716d0081b8d496000c5196417186a8' },
    @{ role='aoem_symbols'; path='D:/WorksArea/AOEM/cargo-target-ci-local/release/aoem_ffi.pdb'; sha='cd59bc926dfa27632d22e948f46155582277f1a3579549f078bdeaeb36e05d78' }
)
foreach ($file in $taskFiles) {
    $actual = (Get-FileHash -LiteralPath $file.path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $file.sha) { throw "Frozen artifact changed: $($file.role)" }
}
$taskProfile = Join-Path $PSScriptRoot 'ingress-minimal.wprp'
$taskXsd = 'C:/Program Files (x86)/Windows Kits/10/Windows Performance Toolkit/WPRControlProfiles.xsd'
$taskXml = [xml](Get-Content -LiteralPath $taskProfile -Raw)
$taskXml.Schemas.Add($null, $taskXsd) | Out-Null
$taskXml.Validate($null)
$taskXperf = 'C:/Program Files (x86)/Windows Kits/10/Windows Performance Toolkit/xperf.exe'
if (-not (Test-Path -LiteralPath $taskXperf)) { throw 'Installed xperf is required for the trace-loss gate.' }

function Read-IngressTraceHeader {
    param([string]$TracePath, [string]$OutputPath, [string]$XperfPath)
    # -tle reads HEADER COUNTERS only. It never waives nonzero or unknown loss.
    $header = @(& $XperfPath -i $TracePath -tle -a tracestats -timespan 2>&1 | ForEach-Object { "$_" })
    $readExit = $LASTEXITCODE
    $header | Set-Content -LiteralPath $OutputPath -Encoding UTF8
    $events = [regex]::Match(($header -join "`n"), 'Total # Lost Events\s*:\s*(\d+)')
    $buffers = [regex]::Match(($header -join "`n"), 'Total # Lost Buffers\s*:\s*(\d+)')
    $result = [ordered]@{ read_exit=$readExit; lost_events=$null; lost_buffers=$null
        trace_start_unix_ms=$null; trace_end_unix_ms=$null; integrity='UNKNOWN' }
    foreach ($edge in @('Start','End')) {
        $time = [regex]::Match(($header -join "`n"), "(?m)^$edge time \(UTC\)\s*:\s*(\d{4}/\d{2}/\d{2}:\d{2}:\d{2}:\d{2}\.\d{7})")
        if ($time.Success) {
            $utc = [DateTimeOffset]::ParseExact($time.Groups[1].Value, 'yyyy/MM/dd:HH:mm:ss.fffffff',
                [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AssumeUniversal)
            $result["trace_$($edge.ToLowerInvariant())_unix_ms"] = $utc.ToUnixTimeMilliseconds()
        }
    }
    if ($readExit -eq 0 -and $events.Success -and $buffers.Success) {
        $result.lost_events = [long]$events.Groups[1].Value
        $result.lost_buffers = [long]$buffers.Groups[1].Value
        $result.integrity = if ($result.lost_events -eq 0 -and $result.lost_buffers -eq 0 -and
            $null -ne $result.trace_start_unix_ms -and $null -ne $result.trace_end_unix_ms) {
            'HEADER_ZERO_LOSS_NEEDS_TARGET_WINDOW_AND_SYMBOL_VALIDATION'
        } elseif ($result.lost_events -ne 0 -or $result.lost_buffers -ne 0) {
            'REJECTED_EVENT_LOSS'
        } else { 'UNKNOWN' }
    }
    return $result
}

function Measure-IngressLoadCoverage {
    param($Window, $RoundReport)
    $answer = [ordered]@{ overlap_seconds=$null; minimum_seconds=$(if ($Window.profile -eq 'NativeSchedule') { 3 } else { 4 }); coverage='UNKNOWN' }
    $boundary = $RoundReport.process_cpu_accounting.external_sample_alignment
    if ($null -ne $Window.trace_start_unix_ms -and $null -ne $Window.trace_end_unix_ms -and
        $null -ne $boundary.before_read_completed_unix_ms -and $null -ne $boundary.after_read_started_unix_ms) {
        $begin = [Math]::Max([long]$Window.trace_start_unix_ms, [long]$boundary.before_read_completed_unix_ms)
        $end = [Math]::Min([long]$Window.trace_end_unix_ms, [long]$boundary.after_read_started_unix_ms)
        $answer.overlap_seconds = [Math]::Max(0, $end - $begin) / 1000.0
        $answer.coverage = if ($answer.overlap_seconds -ge $answer.minimum_seconds) {
            'LOAD_OVERLAP_NEEDS_THREAD_SYMBOL_AND_FREQUENCY_VALIDATION'
        } else { 'REJECTED_INSUFFICIENT_LOAD_OVERLAP' }
    }
    return $answer
}

function Wait-IngressRoundSpawn {
    param($Driver, [string]$StderrPath, [int]$Round)
    $waiting = [System.Diagnostics.Stopwatch]::StartNew()
    while ($waiting.Elapsed.TotalSeconds -le 180) {
        $Driver.Refresh()
        if ($Driver.HasExited) { throw "Load exited before round $Round spawn was observed." }
        if (Test-Path -LiteralPath $StderrPath) {
            $text = Get-Content -LiteralPath $StderrPath -Raw
            $directories = @()
            if ($text) {
                $directories = @([regex]::Matches($text, '(?m)^NUAS native load artifacts=(.+)') | ForEach-Object { $_.Groups[1].Value.Trim() })
            }
            if ($directories.Count -gt $Round) {
                # The original spawn owner creates these before Command.spawn.
                # Do NOT wait for Length/LastWriteTime: the old trigger fired
                # after drive completion in every recorded window.
                $logs = @(Get-ChildItem -LiteralPath $directories[$Round] -Filter 'create-*.stdout.log' -File)
                if ($logs.Count -eq 4) {
                    return [ordered]@{ directory=$directories[$Round]; files=$logs
                        latest_creation_utc=($logs | Sort-Object CreationTimeUtc -Descending | Select-Object -First 1).CreationTimeUtc
                        observed_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() }
                }
            }
        }
        Start-Sleep -Milliseconds 250
    }
    throw "No four-node spawn window for round $Round; original load is not cancelled."
}

if ($ValidateOnly) {
    Write-Output 'PASS: six artifact hashes and minimal WPRP XSD; no recording or workload started.'
    return
}

$taskIdentity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$taskPrincipal = [System.Security.Principal.WindowsPrincipal]::new($taskIdentity)
if (-not $taskPrincipal.IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this bounded capture in an Administrator PowerShell. This script never elevates itself.'
}
if (Get-Process -Name 'novovm-node','native-tests','cargo','rustc' -ErrorAction SilentlyContinue) {
    throw 'Another node, load test or Rust build is active; do not mix it with this trace.'
}
$taskDrive = (Get-Item -LiteralPath $taskRepo).PSDrive.Name
if ((Get-PSDrive -Name $taskDrive).Free -lt 25GB) { throw 'At least 25 GiB free is required; nothing is cleaned automatically.' }

$taskStamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$taskDirectory = Join-Path $taskRepo "target/native-ingress-hot-thread-20261101/$taskStamp-$PID"
New-Item -ItemType Directory -Path $taskDirectory | Out-Null
foreach ($file in $taskFiles | Where-Object { $_.role -like '*_symbols' }) {
    $symbolDirectory = Join-Path $taskDirectory $file.role
    New-Item -ItemType Directory -Path $symbolDirectory | Out-Null
    Copy-Item -LiteralPath $file.path -Destination $symbolDirectory
}
$taskEnvironment = (Get-Content -LiteralPath (Join-Path $taskRepo 'artifacts/audit/native-controller-throughput-20261031/candidate-v1.json') -Raw | ConvertFrom-Json).environment
foreach ($field in $taskEnvironment.PSObject.Properties) {
    [Environment]::SetEnvironmentVariable($field.Name, [string]$field.Value, 'Process')
}
$env:NOVOVM_RESIDENT_NODE_BINARY = $taskFiles[0].path
$taskRunner = $taskFiles[1].path
$taskWindows = [System.Collections.Generic.List[object]]::new()
$taskDriver = $null
$taskLoadExit = $null
$taskFailure = $null
$taskBegin = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()

# Separate sparse CPU stacks from high-rate scheduler events. Never stackwalk
# CSwitch/ReadyThread on every system thread as the superseded capture did.
function Record-IngressWindow {
    param([string]$Tag, [string]$ProfileName, [int]$Seconds)
    $session = "NOVOVM_Ingress_$taskStamp`_$PID`_$Tag"
    $trace = Join-Path $taskDirectory "$Tag.etl"
    $started = $false
    $record = [ordered]@{ tag=$Tag; profile=$ProfileName; session=$session; etl=$trace
        requested_seconds=$Seconds; begin_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds(); end_unix_ms=$null
        start_exit=$null; stop_exit=$null; failure=$null; header_exit=$null
        lost_events=$null; lost_buffers=$null; integrity='UNKNOWN' }
    $taskWindows.Add($record)
    try {
        & wpr.exe -start "$taskProfile!$ProfileName" -filemode -recordtempto $taskDirectory -instancename $session *> (Join-Path $taskDirectory "$Tag-start.log")
        $record.start_exit = $LASTEXITCODE
        if ($record.start_exit -ne 0) { throw "WPR start failed; see $Tag-start.log. No trace-loss gate is bypassed." }
        $started = $true
        if ($Seconds -gt 0) { Start-Sleep -Seconds $Seconds }
    } catch {
        $record.failure = $_.Exception.Message
        throw
    } finally {
        if ($started) {
            try {
                & wpr.exe -stop $trace -skipPdbGen -instancename $session *> (Join-Path $taskDirectory "$Tag-stop.log")
                $record.stop_exit = $LASTEXITCODE
            } catch { $record.failure = "WPR stop: $($_.Exception.Message)" }
        }
        $record.end_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    }
    if ($record.stop_exit -ne 0 -or $record.failure) { throw "WPR stop failed for $session; check this named session manually." }
    $quality = Read-IngressTraceHeader -TracePath $trace -OutputPath (Join-Path $taskDirectory "$Tag-header.txt") -XperfPath $taskXperf
    $record.header_exit = $quality.read_exit
    $record.lost_events = $quality.lost_events
    $record.lost_buffers = $quality.lost_buffers
    $record['trace_start_unix_ms'] = $quality.trace_start_unix_ms
    $record['trace_end_unix_ms'] = $quality.trace_end_unix_ms
    $record.integrity = $quality.integrity
    if ($record.integrity -ne 'HEADER_ZERO_LOSS_NEEDS_TARGET_WINDOW_AND_SYMBOL_VALIDATION') {
        throw "Trace quality failed for $Tag; no further windows will be recorded."
    }
}

try {
    Record-IngressWindow -Tag 'preflight-cpu' -ProfileName 'NativeCpuStacks' -Seconds 0
    Record-IngressWindow -Tag 'preflight-schedule' -ProfileName 'NativeSchedule' -Seconds 0
    $stderr = Join-Path $taskDirectory 'load.stderr.log'
    $taskDriver = Start-Process -FilePath $taskRunner -WorkingDirectory $taskRepo -WindowStyle Hidden -PassThru `
        -ArgumentList 'native_asset_same_corpus_five_round_stability','--ignored','--test-threads=1','--nocapture' `
        -RedirectStandardOutput (Join-Path $taskDirectory 'load.stdout.log') -RedirectStandardError $stderr
    $plan = @(
        @{ profile='NativeCpuStacks'; seconds=8; backend='cpu' },
        @{ profile='NativeCpuStacks'; seconds=8; backend='gpu_required' },
        @{ profile='NativeSchedule'; seconds=5; backend='cpu' },
        @{ profile='NativeSchedule'; seconds=5; backend='gpu_required' },
        @{ profile='NativeCpuStacks'; seconds=8; backend='cpu' }
    )
    for ($round = 0; $round -lt $plan.Count; $round++) {
        $spawn = Wait-IngressRoundSpawn -Driver $taskDriver -StderrPath $stderr -Round $round
        # A scheduling aid, NOT a trusted load-start marker. Offline analysis must
        # intersect each ETL with the original drive UTC boundaries and reject
        # insufficient overlap. No node/client timer or validity is changed.
        $delay = ($spawn.latest_creation_utc.AddSeconds(8) - [DateTime]::UtcNow).TotalMilliseconds
        if ($delay -gt 0) { Start-Sleep -Milliseconds ([int]$delay) }
        Record-IngressWindow -Tag "round-$round" -ProfileName $plan[$round].profile -Seconds $plan[$round].seconds
        $taskWindows[$taskWindows.Count - 1]['round'] = $round
        $taskWindows[$taskWindows.Count - 1]['backend'] = $plan[$round].backend
        $taskWindows[$taskWindows.Count - 1]['round_directory'] = $spawn.directory
    }
} catch {
    $taskFailure = $_.Exception.Message
} finally {
    if ($taskDriver) {
        Write-Output 'Waiting for the unchanged original load and cold recovery. Check any failed WPR stop by its recorded session name.'
        $taskDriver.WaitForExit()
        $taskLoadExit = $taskDriver.ExitCode
    }
    if ($taskDriver -and $taskLoadExit -eq 0) {
        try {
            $paired = @(Select-String -LiteralPath $stderr -Pattern '^NUAS paired report=(.+)$')
            if ($paired.Count -ne 1) { throw 'Original paired report is missing; load coverage cannot qualify.' }
            $reports = Get-Content -LiteralPath ($paired[0].Matches[0].Groups[1].Value.Trim()) -Raw | ConvertFrom-Json
            foreach ($window in $taskWindows | Where-Object { $_.tag -match '^round-\d+$' }) {
                $index = [int]($window.tag -replace '^round-', '')
                $coverage = Measure-IngressLoadCoverage -Window $window -RoundReport $reports.rounds[$index]
                $window['load_coverage'] = $coverage
                if ($coverage.coverage -ne 'LOAD_OVERLAP_NEEDS_THREAD_SYMBOL_AND_FREQUENCY_VALIDATION') {
                    $taskFailure = "$taskFailure Round $index has insufficient or unknown load overlap."
                }
            }
        } catch { $taskFailure = "$taskFailure Coverage validation: $($_.Exception.Message)" }
    }
    [ordered]@{
        schema='novovm-ingress-bounded-capture/v3'
        scope='Frozen five-round same-byte CPU/GPU/CPU/GPU/CPU diagnostic. Only short selected ETW windows; not new TPS acceptance. No CSwitch/ReadyThread stacks. Scheduling does NOT provide wait callsites. All raw system traces remain local.'
        begin_unix_ms=$taskBegin; end_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        driver_pid=$(if ($taskDriver) { $taskDriver.Id } else { $null }); load_exit=$taskLoadExit; failure=$taskFailure
        windows=$taskWindows.ToArray(); profile_sha256=(Get-FileHash -LiteralPath $taskProfile -Algorithm SHA256).Hash.ToLowerInvariant()
        files=$taskFiles; environment=$taskEnvironment
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $taskDirectory 'capture.json') -Encoding UTF8
}
Write-Output "Capture directory: $taskDirectory"
if ($taskFailure) { throw $taskFailure }
if ($taskLoadExit -ne 0) { throw 'Original load failed; retain all directories and logs for analysis.' }
