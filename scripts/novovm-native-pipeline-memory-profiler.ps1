param(
    [ValidateSet("prepare", "snapshot", "series", "wpr-start", "wpr-stop")]
    [string]$Action = "snapshot",
    [int]$TargetPid = 0,
    [string]$Label = "sample",
    [string]$ArtifactDir = "artifacts/native-pipeline/memory-profiler",
    [string]$NodeBinary = "",
    [ValidateRange(1, 900)]
    [int]$DurationSeconds = 900,
    [ValidateRange(500, 5000)]
    [int]$SampleIntervalMilliseconds = 1000
)

$ErrorActionPreference = "Stop"

function Ensure-ArtifactDir {
    New-Item -ItemType Directory -Force -Path $ArtifactDir | Out-Null
}

function Resolve-TargetProcess {
    param([int]$TargetPid)
    if ($TargetPid -gt 0) {
        return Get-Process -Id $TargetPid -ErrorAction Stop
    }
    $nodes = Get-Process novovm-node -ErrorAction SilentlyContinue | Sort-Object StartTime -Descending
    if (-not $nodes) {
        throw "No novovm-node process found. Pass -TargetPid <receiver pid> after starting receiver."
    }
    return $nodes[0]
}

function Write-Manifest {
    param($Process, [string]$Kind)
    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $path = Join-Path $ArtifactDir "$timestamp-$Label-$Kind.json"
    $payload = [ordered]@{
        schema = "novovm-native-pipeline-memory-profiler/v1"
        kind = $Kind
        label = $Label
        timestamp = (Get-Date).ToString("o")
        pid = $Process.Id
        process_name = $Process.ProcessName
        path = $Process.Path
        start_time = $Process.StartTime
        working_set_bytes = [int64]$Process.WorkingSet64
        private_bytes = [int64]$Process.PrivateMemorySize64
        virtual_bytes = [int64]$Process.VirtualMemorySize64
        handle_count = [int64]$Process.HandleCount
        thread_count = [int64]$Process.Threads.Count
        vmmap_guidance = "Use VMMap GUI: File -> Save As into $ArtifactDir, capture at 1000/2000/2400 tx windows."
        wpr_guidance = "Use wpr-start before sender and wpr-stop after failure/PASS when heap stack capture is needed."
        memory_plateau_signed = $false
    }
    $payload | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 -Path $path
    Write-Output $path
}

function Read-SeriesProcess {
    param($Process, [string]$Role)
    $started = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    try {
        $Process.Refresh()
        $threads = @(
            foreach ($thread in $Process.Threads) {
                $threadId = $null
                try {
                    $threadId = $thread.Id
                    $state = $thread.ThreadState
                    [ordered]@{
                        tid = $threadId
                        cpu_ticks_100ns = $thread.TotalProcessorTime.Ticks
                        state = $state.ToString()
                        wait_reason = $(if ($state -eq [System.Diagnostics.ThreadState]::Wait) {
                            $thread.WaitReason.ToString()
                        } else { $null })
                    }
                } catch {
                    [ordered]@{ tid = $threadId; error = $_.Exception.Message }
                } finally {
                    $thread.Dispose()
                }
            }
        )
        return [ordered]@{
            status = "AVAILABLE"
            role = $Role
            pid = $Process.Id
            process_start_utc = $Process.StartTime.ToUniversalTime().ToString("o")
            sample_started_unix_ms = $started
            cpu_ticks_100ns = $Process.TotalProcessorTime.Ticks
            user_ticks_100ns = $Process.UserProcessorTime.Ticks
            kernel_ticks_100ns = $Process.PrivilegedProcessorTime.Ticks
            working_set_bytes = $Process.WorkingSet64
            peak_working_set_bytes = $Process.PeakWorkingSet64
            private_bytes = $Process.PrivateMemorySize64
            handle_count = $Process.HandleCount
            thread_count = $threads.Count
            threads = $threads
            sample_completed_unix_ms = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        }
    } catch {
        return [ordered]@{ status = "UNAVAILABLE"; role = $Role; pid = $Process.Id
            sample_started_unix_ms = $started; error = $_.Exception.Message }
    }
}

Ensure-ArtifactDir

switch ($Action) {
    "prepare" {
        $readme = Join-Path $ArtifactDir "README.txt"
        @"
NOVOVM native pipeline memory profiler artifacts

1. Start B receiver.
2. Use: .\scripts\novovm-native-pipeline-memory-profiler.ps1 -Action snapshot -Label 1000tx
3. Save VMMap snapshots into this directory at 1000/2000/2400 tx.
4. If Heap/Private Data dominates, run:
   .\scripts\novovm-native-pipeline-memory-profiler.ps1 -Action wpr-start -TargetPid <pid>
   ... reproduce ...
   .\scripts\novovm-native-pipeline-memory-profiler.ps1 -Action wpr-stop -Label heap

Do not sign memory plateau from these artifacts alone.
"@ | Set-Content -Encoding UTF8 -Path $readme
        Write-Output $readme
    }
    "snapshot" {
        $process = Resolve-TargetProcess -TargetPid $TargetPid
        Write-Manifest -Process $process -Kind "process-snapshot"
    }
    "series" {
        if ($TargetPid -le 0 -or -not $NodeBinary) {
            throw "series requires the live test-driver -TargetPid and frozen -NodeBinary."
        }
        $binary = (Resolve-Path -LiteralPath $NodeBinary).ProviderPath
        $driver = Get-Process -Id $TargetPid -ErrorAction Stop
        $driverStart = $driver.StartTime
        $observer = Get-Process -Id $PID
        $observerCpu = $observer.TotalProcessorTime.Ticks
        $clock = [System.Diagnostics.Stopwatch]::StartNew()
        $samples = [System.Collections.Generic.List[object]]::new()
        $stopReason = "duration_bound"
        try {
        while ($clock.Elapsed.TotalSeconds -lt $DurationSeconds) {
            try {
                $driver.Refresh()
                if ($driver.HasExited) { $stopReason = "driver_exited"; break }
                if ($driver.StartTime -ne $driverStart) { throw "Driver process identity changed." }
            } catch {
                $stopReason = "driver_unavailable: $($_.Exception.Message)"
                break
            }
            $sampleClock = [System.Diagnostics.Stopwatch]::StartNew()
            $processes = [System.Collections.Generic.List[object]]::new()
            $processes.Add((Read-SeriesProcess -Process $driver -Role "driver"))
            foreach ($node in @(Get-Process -Name "novovm-node" -ErrorAction SilentlyContinue)) {
                try {
                    if ([string]::Equals($node.Path, $binary, [StringComparison]::OrdinalIgnoreCase)) {
                        $processes.Add((Read-SeriesProcess -Process $node -Role "node"))
                    }
                } catch {
                    $processes.Add([ordered]@{ status = "UNAVAILABLE"; role = "node"
                        pid = $node.Id; error = $_.Exception.Message })
                } finally { $node.Dispose() }
            }
            $samples.Add([ordered]@{
                elapsed_ms = $clock.Elapsed.TotalMilliseconds
                sample_cost_ms = $sampleClock.Elapsed.TotalMilliseconds
                processes = $processes.ToArray()
            })
            $remaining = $SampleIntervalMilliseconds - $sampleClock.Elapsed.TotalMilliseconds
            if ($remaining -gt 0) { Start-Sleep -Milliseconds ([int]$remaining) }
        }
        } catch { $stopReason = "observer_error: $($_.Exception.Message)" }
        $observer.Refresh()
        $timestamp = Get-Date -Format "yyyyMMdd-HHmmss-fff"
        $path = Join-Path $ArtifactDir "$timestamp-$PID-$Label-process-series.json"
        if (Test-Path -LiteralPath $path) { throw "Refusing to overwrite $path" }
        [ordered]@{
            schema = "novovm-native-pipeline-process-series/v1"
            target_driver_pid = $TargetPid
            node_binary = $binary
            logical_processors = [Environment]::ProcessorCount
            elapsed_ms = $clock.Elapsed.TotalMilliseconds
            interval_ms = $SampleIntervalMilliseconds
            stop_reason = $stopReason
            coverage = $(if ($stopReason -eq "driver_exited") { "sampled until driver exit" } else { "PARTIAL: observer stopped before confirmed driver exit" })
            observer_sampling_cpu_ticks_100ns = $observer.TotalProcessorTime.Ticks - $observerCpu
            observer_peak_working_set_before_output_bytes = $observer.PeakWorkingSet64
            scope = "External sampled process/thread counters, buffered until stop. Includes startup and cold recovery; align by harness node PIDs and CPU boundary markers before comparing load. Thread states are instantaneous, not wait durations. Working/private bytes are not allocation/free counts; sampled private maxima are lower bounds. OS peak working set spans process lifetime. No persistence/disk latency or GPU device time is measured. Observer sampling overhead is retained, not deducted from TPS; final JSON serialization/write cost is outside these observer counters."
            samples = $samples.ToArray()
        } | ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 -LiteralPath $path
        $driver.Dispose()
        $observer.Dispose()
        Write-Output $path
    }
    "wpr-start" {
        $process = Resolve-TargetProcess -TargetPid $TargetPid
        Write-Manifest -Process $process -Kind "wpr-start-target" | Out-Null
        wpr -start Heap -filemode
        Write-Output "WPR heap trace started for observation target pid=$($process.Id). Stop with -Action wpr-stop."
    }
    "wpr-stop" {
        $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
        $etl = Join-Path $ArtifactDir "$timestamp-$Label-heap.etl"
        wpr -stop $etl
        Write-Output $etl
    }
}
