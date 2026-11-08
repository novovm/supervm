# Local deployment process barriers. No node configuration or database access.
function Wait-NovonetNodeExit {
    param(
        [int]$TimeoutMilliseconds = 10000,
        [scriptblock]$ReadNodes = { @(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'") }
    )
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $emptySamples = 0
    do {
        $nodes = @(& $ReadNodes | Where-Object { $null -ne $_ })
        if ($nodes.Count -eq 0) {
            $emptySamples += 1
            if ($emptySamples -eq 2) { return }
        } else {
            $emptySamples = 0
        }
        Start-Sleep -Milliseconds 100
    } while ($timer.ElapsedMilliseconds -lt $TimeoutMilliseconds)
    $remaining = $nodes | Select-Object ProcessId, ParentProcessId, ExecutablePath, CreationDate
    throw ('Node exit barrier timed out; preserve process evidence: ' + ($remaining | ConvertTo-Json -Compress))
}

function Stop-NovonetExactProcess {
    param(
        [Parameter(Mandatory=$true)][int]$ProcessId,
        [Parameter(Mandatory=$true)][string]$ExpectedPath,
        [Parameter(Mandatory=$true)][string]$ExpectedSha256
    )
    $node = Get-CimInstance Win32_Process -Filter "ProcessId=$ProcessId"
    if ($null -eq $node -or $node.ExecutablePath -cne $ExpectedPath -or
        (Invoke-CimMethod -InputObject $node -MethodName GetOwnerSid).Sid -cne 'S-1-5-19' -or
        (Get-FileHash -LiteralPath $ExpectedPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ExpectedSha256) {
        throw 'Exact node process identity changed before termination'
    }
    $process = Get-Process -Id $ProcessId -ErrorAction Stop
    try {
        # Keep the same OS process handle across termination and exit waiting.
        $null = $process.Handle
        if ($process.Path -cne $ExpectedPath -or
            [Math]::Abs(($process.StartTime.ToUniversalTime() - $node.CreationDate.ToUniversalTime()).TotalSeconds) -gt 1) {
            throw 'Process handle differs from inspected node'
        }
        $process.Kill()
        if (-not $process.WaitForExit(10000)) { throw 'Exact node process has not exited' }
    } finally {
        $process.Dispose()
    }
    Wait-NovonetNodeExit
}
