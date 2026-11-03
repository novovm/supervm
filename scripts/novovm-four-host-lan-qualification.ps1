param(
    [Parameter(Mandatory)][ValidateSet('A','C','D')][string]$Role,
    [Parameter(Mandatory)][DateTimeOffset]$StartUtc,
    [Parameter(Mandatory)][string]$ConfigRoot,
    [Parameter(Mandatory)][string]$EvidenceRoot
)
$ErrorActionPreference = 'Stop'
$configPath = (Resolve-Path -LiteralPath $ConfigRoot).Path
if ($StartUtc -le [DateTimeOffset]::UtcNow.AddMinutes(5)) {
    throw 'Choose one common start at least five minutes ahead, after all READY acknowledgments.'
}
if (Test-Path -LiteralPath $EvidenceRoot) { throw 'EvidenceRoot must be new; preserve existing reports.' }
$source = @{ A='192.168.71.74'; C='192.168.71.125'; D='192.168.71.59' }[$Role]
if (-not (Get-NetIPAddress -AddressFamily IPv4 -IPAddress $source -ErrorAction SilentlyContinue)) {
    throw "Expected LAN address $source is absent; report the change instead of rewriting pins."
}
$exe = if ($Role -eq 'A') {
    Join-Path $PSScriptRoot '../artifacts/product-overlay/native65-d7ef793b-windows-x86_64/bin/novovm-product-nat.exe'
} else {
    Join-Path $env:LOCALAPPDATA 'NOVOVM/candidates/native65-d7ef793b-windows-x86_64/bin/novovm-product-nat.exe'
}
if ((Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant() -ne
    '970e89225b5daa6a1563e0d5313db3cf25ad3ba2a90a0cc2188c8bf5a58dec73') {
    throw 'Not the frozen NAT program; do not rebuild or substitute another binary.'
}
foreach ($line in Get-Content -LiteralPath "$configPath/LAN-CHECKSUMS.sha256") {
    if ($line -notmatch '^([0-9a-f]{64})  (.+)$') { throw 'Invalid config checksum line.' }
    $hash = $Matches[1]; $relative = $Matches[2]
    if ((Get-FileHash -LiteralPath (Join-Path $configPath $relative)).Hash.ToLowerInvariant() -ne $hash) {
        throw "Config changed: $relative"
    }
}
$jobs = switch ($Role) {
    A { @(
        @{ At=$StartUtc.AddSeconds(-15); Cases=@('observer-A-for-C') },
        @{ At=$StartUtc.AddMinutes(3).AddSeconds(-15); Cases=@('observer-A-for-D') }
    ) }
    C { @(
        @{ At=$StartUtc; Cases=@('probe-C-A-first','probe-C-A-second') },
        @{ At=$StartUtc.AddMinutes(6); Cases=@('probe-C-D-first','probe-C-D-second') }
    ) }
    D { @(
        @{ At=$StartUtc.AddMinutes(3); Cases=@('probe-D-A-first','probe-D-A-second') },
        @{ At=$StartUtc.AddMinutes(6).AddSeconds(-15); Cases=@('observer-D-for-C') }
    ) }
}
foreach ($job in $jobs) {
    foreach ($case in $job.Cases) {
        $configFile = Join-Path $configPath "$Role/$case.json"
        $config = Get-Content -LiteralPath $configFile -Raw | ConvertFrom-Json
        $report = Join-Path (Split-Path $configFile) $config.report_path
        if (Test-Path -LiteralPath $report) { throw "Original report already exists: $report" }
        if ((Get-FileHash -LiteralPath $config.library).Hash.ToLowerInvariant() -ne $config.library_sha256) {
            throw "Wrong actual AOEM library for $case"
        }
    }
}
New-Item -ItemType Directory -Path $EvidenceRoot | Out-Null
$evidencePath = (Resolve-Path -LiteralPath $EvidenceRoot).Path
$peers = switch ($Role) {
    A { @('192.168.71.125','192.168.71.59') }
    C { @('192.168.71.74','192.168.71.59') }
    D { @('192.168.71.74','192.168.71.125') }
}
@(foreach ($peer in $peers) { Find-NetRoute -RemoteIPAddress $peer }) |
    ConvertTo-Json -Depth 8 > "$evidencePath/routes-before.json"
Write-Output ("ARMED_LAN_$Role start=" + $StartUtc.ToString('o'))
$records = @()
foreach ($job in $jobs) {
    while ([DateTimeOffset]::UtcNow -lt $job.At) { Start-Sleep -Seconds 1 }
    if ([DateTimeOffset]::UtcNow -gt $job.At.AddSeconds(5)) {
        $records += @{ cases=$job.Cases; status='not_executed_missed_schedule' }
        continue
    }
    foreach ($case in $job.Cases) {
        $begin = [DateTimeOffset]::UtcNow
        $configFile = Join-Path $configPath "$Role/$case.json"
        & $exe $configFile > "$evidencePath/$case.stdout.txt" 2> "$evidencePath/$case.stderr.txt"
        $code = $LASTEXITCODE
        $records += @{ case=$case; started_utc=$begin.ToString('o'); ended_utc=[DateTimeOffset]::UtcNow.ToString('o'); exit_code=$code }
        $records | ConvertTo-Json -Depth 6 > "$evidencePath/execution.json"
        if ($code -ne 0) { break }
    }
}
$records | ConvertTo-Json -Depth 6 > "$evidencePath/execution.json"
Write-Output "LAN_$Role finished; retain role/reports, stdout/stderr and route evidence for paired reconciliation."
if (@($records | Where-Object { $_.status -or $_.exit_code -ne 0 }).Count) { exit 1 }
