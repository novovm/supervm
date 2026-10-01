param(
    [ValidateSet("prepare", "snapshot", "wpr-start", "wpr-stop")]
    [string]$Action = "snapshot",
    [int]$TargetPid = 0,
    [string]$Label = "sample",
    [string]$ArtifactDir = "artifacts/native-pipeline/memory-profiler"
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
