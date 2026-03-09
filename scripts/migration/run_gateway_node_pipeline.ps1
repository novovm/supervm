param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$SpoolDir = "artifacts/ingress/spool",
    [ValidateRange(10, 60000)]
    [int]$PollMs = 200,
    [ValidateRange(0, 86400)]
    [int]$IdleExitSeconds = 0,
    [ValidateRange(0, 4294967295)]
    [uint32]$GatewayMaxRequests = 0,
    [switch]$SkipBuild,
    [string]$GatewayBinaryPath = "",
    [string]$NodeBinaryPath = "",
    [switch]$SkipGatewayStart,
    [switch]$RunOnce
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
    }
    return (Resolve-Path $Root).Path
}

function Resolve-FullPath {
    param(
        [string]$Root,
        [string]$Value
    )
    if ([System.IO.Path]::IsPathRooted($Value)) {
        return [System.IO.Path]::GetFullPath($Value)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $Root $Value))
}

function New-Dir {
    param([string]$PathValue)
    New-Item -ItemType Directory -Force -Path $PathValue | Out-Null
}

function Invoke-ProcessAndCapture {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$WorkingDirectory,
        [hashtable]$Environment
    )

    $stdoutFile = [System.IO.Path]::GetTempFileName()
    $stderrFile = [System.IO.Path]::GetTempFileName()
    try {
        $envState = Push-ProcessEnv -Environment $Environment
        try {
            $startArgs = @{
                FilePath               = $FilePath
                WorkingDirectory       = $WorkingDirectory
                RedirectStandardOutput = $stdoutFile
                RedirectStandardError  = $stderrFile
                PassThru               = $true
                Wait                   = $true
                NoNewWindow            = $true
                ErrorAction            = "Stop"
            }
            if ($null -ne $Arguments -and $Arguments.Count -gt 0) {
                $startArgs["ArgumentList"] = $Arguments
            }
            $proc = Start-Process @startArgs
            if ($null -eq $proc) {
                throw "start process returned null: $FilePath"
            }
        } finally {
            Pop-ProcessEnv -State $envState
        }

        $stdout = ""
        $stderr = ""
        if (Test-Path $stdoutFile) {
            $stdout = (Get-Content -Path $stdoutFile -Raw)
        }
        if (Test-Path $stderrFile) {
            $stderr = (Get-Content -Path $stderrFile -Raw)
        }
        $stdoutText = Normalize-Text -Value $stdout
        $stderrText = Normalize-Text -Value $stderr
        $combinedText = Normalize-Text -Value ($stdoutText + "`n" + $stderrText)
        return [ordered]@{
            exit_code = [int]$proc.ExitCode
            stdout = $stdoutText
            stderr = $stderrText
            output = $combinedText
        }
    } finally {
        if (Test-Path $stdoutFile) {
            Remove-Item -Force $stdoutFile
        }
        if (Test-Path $stderrFile) {
            Remove-Item -Force $stderrFile
        }
    }
}

function Push-ProcessEnv {
    param([hashtable]$Environment)
    $state = @{}
    if ($null -eq $Environment) {
        return $state
    }
    foreach ($key in $Environment.Keys) {
        $envPath = "Env:$key"
        $exists = Test-Path $envPath
        $oldValue = $null
        if ($exists) {
            $oldValue = (Get-Item -Path $envPath).Value
        }
        $state[$key] = [pscustomobject]@{
            exists = $exists
            value = $oldValue
        }
        Set-Item -Path $envPath -Value ([string]$Environment[$key])
    }
    return $state
}

function Pop-ProcessEnv {
    param([hashtable]$State)
    if ($null -eq $State) {
        return
    }
    foreach ($key in $State.Keys) {
        $entry = $State[$key]
        $envPath = "Env:$key"
        if ($entry.exists) {
            Set-Item -Path $envPath -Value ([string]$entry.value)
        } else {
            Remove-Item -Path $envPath -ErrorAction SilentlyContinue
        }
    }
}

function Normalize-Text {
    param($Value)
    if ($null -eq $Value) {
        return ""
    }
    return $Value.ToString().Trim()
}

function Resolve-BinaryPath {
    param(
        [string]$DefaultTargetRoot,
        [string]$ExplicitPath,
        [string]$BinaryBaseName
    )
    if ($ExplicitPath) {
        return (Resolve-FullPath -Root $DefaultTargetRoot -Value $ExplicitPath)
    }
    $isWindowsOs = $env:OS -eq "Windows_NT"
    $exeName = if ($isWindowsOs) { "$BinaryBaseName.exe" } else { $BinaryBaseName }
    return (Join-Path $DefaultTargetRoot "debug\$exeName")
}

function Get-OpsWireFiles {
    param([string]$DirPath)
    if (-not (Test-Path $DirPath)) {
        return @()
    }
    return @(Get-ChildItem -Path $DirPath -File -Filter *.opsw1 | Sort-Object Name)
}

function Requeue-ProcessingOpsWire {
    param(
        [string]$ProcessingDir,
        [string]$SpoolDir
    )
    if (-not (Test-Path $ProcessingDir)) {
        return 0
    }
    $moved = 0
    $batches = @(Get-ChildItem -Path $ProcessingDir -Directory -ErrorAction SilentlyContinue)
    foreach ($batch in $batches) {
        $files = @(Get-ChildItem -Path $batch.FullName -File -Filter *.opsw1 -ErrorAction SilentlyContinue)
        foreach ($file in $files) {
            $dest = Join-Path $SpoolDir $file.Name
            if (Test-Path $dest) {
                $suffix = [guid]::NewGuid().ToString("N").Substring(0, 6)
                $dest = Join-Path $SpoolDir ("{0}.{1}.opsw1" -f $file.BaseName, $suffix)
            }
            Move-Item -Path $file.FullName -Destination $dest
            $moved = $moved + 1
        }
        $remaining = @(Get-ChildItem -Path $batch.FullName -Force -ErrorAction SilentlyContinue)
        if ($remaining.Count -eq 0) {
            Remove-Item -Path $batch.FullName -Force -ErrorAction SilentlyContinue
        }
    }
    return $moved
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$SpoolDir = Resolve-FullPath -Root $RepoRoot -Value $SpoolDir
$ProcessingRoot = Resolve-FullPath -Root $RepoRoot -Value "artifacts/ingress/processing"
$DoneRoot = Resolve-FullPath -Root $RepoRoot -Value "artifacts/ingress/done"
$FailedRoot = Resolve-FullPath -Root $RepoRoot -Value "artifacts/ingress/failed"
$LogRoot = Resolve-FullPath -Root $RepoRoot -Value "artifacts/ingress/logs"

New-Dir -PathValue $SpoolDir
New-Dir -PathValue $ProcessingRoot
New-Dir -PathValue $DoneRoot
New-Dir -PathValue $FailedRoot
New-Dir -PathValue $LogRoot
$requeued = Requeue-ProcessingOpsWire -ProcessingDir $ProcessingRoot -SpoolDir $SpoolDir

$CargoTargetRoot = if ($env:CARGO_TARGET_DIR) {
    [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
} else {
    Join-Path $RepoRoot "target"
}

if (-not $SkipBuild) {
    $build = Invoke-ProcessAndCapture `
        -FilePath "cargo" `
        -Arguments @("build", "-p", "novovm-edge-gateway", "-p", "novovm-node") `
        -WorkingDirectory $RepoRoot `
        -Environment @{}
    if ($build.exit_code -ne 0) {
        throw "build failed: $($build.output)"
    }
}

$GatewayExe = Resolve-BinaryPath -DefaultTargetRoot $CargoTargetRoot -ExplicitPath $GatewayBinaryPath -BinaryBaseName "novovm-edge-gateway"
$NodeExe = Resolve-BinaryPath -DefaultTargetRoot $CargoTargetRoot -ExplicitPath $NodeBinaryPath -BinaryBaseName "novovm-node"
if (-not (Test-Path $GatewayExe)) {
    throw "gateway binary not found: $GatewayExe"
}
if (-not (Test-Path $NodeExe)) {
    throw "node binary not found: $NodeExe"
}

$gatewayProc = $null
if (-not $SkipGatewayStart) {
    $gatewayOut = Join-Path $LogRoot "edge-gateway.stdout.log"
    $gatewayErr = Join-Path $LogRoot "edge-gateway.stderr.log"
    $gatewayEnv = @{
        "NOVOVM_GATEWAY_BIND" = $GatewayBind
        "NOVOVM_GATEWAY_SPOOL_DIR" = $SpoolDir
        "NOVOVM_GATEWAY_MAX_REQUESTS" = "$GatewayMaxRequests"
    }
    $gatewayEnvState = Push-ProcessEnv -Environment $gatewayEnv
    try {
        $gatewayProc = Start-Process `
            -FilePath $GatewayExe `
            -WorkingDirectory $RepoRoot `
            -RedirectStandardOutput $gatewayOut `
            -RedirectStandardError $gatewayErr `
            -PassThru `
            -NoNewWindow `
            -ErrorAction Stop
        if ($null -eq $gatewayProc) {
            throw "start gateway returned null: $GatewayExe"
        }
    } finally {
        Pop-ProcessEnv -State $gatewayEnvState
    }
    Start-Sleep -Milliseconds 400
    if ($gatewayProc.HasExited) {
        throw "edge gateway exited unexpectedly. check logs: $gatewayOut ; $gatewayErr"
    }
}

Write-Host "pipeline_in: spool_dir=$SpoolDir poll_ms=$PollMs idle_exit_seconds=$IdleExitSeconds run_once=$RunOnce gateway_started=$(-not $SkipGatewayStart) requeued_from_processing=$requeued"

$processedBatches = 0
$processedFiles = 0
$lastActive = Get-Date

try {
    while ($true) {
        if ($gatewayProc -ne $null -and $gatewayProc.HasExited) {
            throw "edge gateway exited unexpectedly during pipeline run"
        }
        $files = Get-OpsWireFiles -DirPath $SpoolDir
        if (@($files).Count -eq 0) {
            if ($RunOnce) {
                break
            }
            if ($IdleExitSeconds -gt 0) {
                $idleSec = ((Get-Date) - $lastActive).TotalSeconds
                if ($idleSec -ge $IdleExitSeconds) {
                    break
                }
            }
            Start-Sleep -Milliseconds $PollMs
            continue
        }

        $lastActive = Get-Date
        $batchId = "batch-{0:yyyyMMddHHmmssfff}" -f (Get-Date)
        $batchDir = Join-Path $ProcessingRoot $batchId
        New-Dir -PathValue $batchDir

        foreach ($file in $files) {
            Move-Item -Path $file.FullName -Destination (Join-Path $batchDir $file.Name)
        }

        $nodeEnv = @{
            "NOVOVM_NODE_MODE" = "full"
            "NOVOVM_EXEC_PATH" = "ffi_v2"
            "NOVOVM_D1_INGRESS_MODE" = "ops_wire_v1"
            "NOVOVM_OPS_WIRE_DIR" = $batchDir
            "NOVOVM_TX_REPEAT_COUNT" = "1"
        }
        $result = Invoke-ProcessAndCapture `
            -FilePath $NodeExe `
            -Arguments @() `
            -WorkingDirectory $RepoRoot `
            -Environment $nodeEnv

        if ($result.exit_code -ne 0) {
            $failedDir = Join-Path $FailedRoot $batchId
            Move-Item -Path $batchDir -Destination $failedDir
            throw "novovm-node failed on ${batchId}: $($result.output)"
        }

        $doneDir = Join-Path $DoneRoot $batchId
        Move-Item -Path $batchDir -Destination $doneDir
        $processedBatches = $processedBatches + 1
        $processedFiles = $processedFiles + @($files).Count
        Write-Host "pipeline_batch_ok: id=$batchId files=$(@($files).Count) done_dir=$doneDir"

        if ($RunOnce) {
            break
        }
    }
} finally {
    if ($gatewayProc -ne $null -and -not $gatewayProc.HasExited) {
        Stop-Process -Id $gatewayProc.Id -Force
    }
}

Write-Host "pipeline_out: processed_batches=$processedBatches processed_files=$processedFiles spool_dir=$SpoolDir done_root=$DoneRoot failed_root=$FailedRoot"
