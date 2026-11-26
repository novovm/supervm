#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet('Keys','Plan','Validate','Ready','Create','Existing','Stop','Archive','Query','Submit')]
    [string] $Action,
    [string] $RunId,
    [long] $ChainId,
    [ValidateSet('A1','A2','C1','D1')][string] $Role,
    [ValidateSet('A1','C1','D1')][string] $Actor,
    [string] $Address,
    [string] $PlanFile,
    [string[]] $PublicSlots,
    [long] $StartUnix,
    [long] $EndUnix,
    [string] $Node,
    [string] $Aoem,
    [string] $Harness,
    [string] $Ctl,
    [string] $NodeSha256,
    [string] $PlanSha256,
    [string] $AoemSha256,
    [string] $HarnessSha256,
    [string] $CtlSha256,
    [long] $Number = -1,
    [ValidatePattern('^[a-zA-Z0-9][a-zA-Z0-9-]{0,63}$')][string] $Evidence
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-PinnedFile([string] $Path, [string] $Sha256) {
    if ($Sha256 -cnotmatch '^[a-f0-9]{64}$' -or -not $Path) { throw 'Explicit file and lowercase SHA-256 required' }
    $resolved = (Get-Item -LiteralPath $Path).FullName
    if ((Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant() -cne $Sha256) {
        throw "SHA-256 mismatch for $resolved"
    }
    $resolved
}

function Confirm-Directory([string] $Path) {
    $item = Get-Item -LiteralPath $Path
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Expected a real fixture directory at $Path"
    }
}

function Write-NewJson([string] $Path, $Value) {
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 30))
    $file = [IO.File]::Open([IO.Path]::GetFullPath($Path), 'CreateNew', 'Write', 'None')
    try { $file.Write($bytes) } finally { $file.Dispose() }
}

function Confirm-Domain {
    if ($RunId -cnotmatch '^[a-f0-9]{32}$' -or $ChainId -lt 7000000000 -or $ChainId -ge 8000000000) {
        throw 'Require a new run id and isolated chain id'
    }
}

function Confirm-LanBind([string] $Endpoint) {
    $ep = [Net.IPEndPoint]::Parse($Endpoint)
    $bytes = $ep.Address.GetAddressBytes()
    if ($bytes.Count -ne 4 -or $ep.Port -lt 45000 -or $ep.Port -ge 50000 -or -not (
        $bytes[0] -eq 10 -or ($bytes[0] -eq 172 -and $bytes[1] -ge 16 -and $bytes[1] -le 31) -or
        ($bytes[0] -eq 192 -and $bytes[1] -eq 168)
    )) { throw 'Expected a private IPv4 address and test UDP port (45000–49999)' }
    $socket = [Net.Sockets.UdpClient]::new($ep.AddressFamily)
    try {
        $socket.ExclusiveAddressUse = $true
        $socket.Client.Bind($ep)
    } finally {
        $socket.Dispose()
    }
}

function Invoke-PurposeKey([string] $Operation, [string] $Purpose, [string] $KeyPath) {
    $raw = & $Ctl native-identity $Operation --chain-id $ChainId --purpose $Purpose --key-file $KeyPath --library $Aoem --library-sha256 $AoemSha256
    if ($LASTEXITCODE -ne 0) { throw "$Operation failed for test purpose $Purpose" }
    $data = ($raw | ConvertFrom-Json).data
    if ($data.private_material_exported -ne $false -or $data.identity.public_key.Count -ne 1952) {
        throw 'Expected public-only ML-DSA-65 identity'
    }
    return ,$data.identity.public_key
}

function Invoke-Harness([string] $Operation, [string] $Report) {
    $saved = @{}
    foreach ($entry in (Get-ChildItem Env: | Where-Object Name -Match '^(NOVOVM|AOEM)_')) {
        $saved[$entry.Name] = $entry.Value
        [Environment]::SetEnvironmentVariable($entry.Name, $null)
    }
    $inputs = @{
        ACTION = $Operation; PLAN = $PlanFile; PLAN_SHA256 = $PlanSha256; REPORT = $Report
        ROLE = $Role; ACTOR = $Actor; AOEM = $Aoem; NODE = $Node
        NUMBER = $(if ($Number -ge 0) { "$Number" } else { '-' })
    }
    try {
        foreach ($entry in $inputs.GetEnumerator()) {
            [Environment]::SetEnvironmentVariable("NOVOVM_CATCHUP_$($entry.Key)", $entry.Value)
        }
        & $Harness native_distributed_validator_fixture --ignored --nocapture --test-threads 1
        if ($LASTEXITCODE -ne 0) { throw "Catch-up fixture action $Operation failed" }
    } finally {
        foreach ($key in $inputs.Keys) { [Environment]::SetEnvironmentVariable("NOVOVM_CATCHUP_$key", $null) }
        foreach ($entry in $saved.GetEnumerator()) { [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value) }
    }
}

$slots = @('A1','A2','C1','D1')
if (-not $IsWindows) { throw 'This runner uses Windows test packages and current-user DPAPI' }

if ($Action -eq 'Plan') {
    Confirm-Domain
    if (-not $PlanFile -or @($PublicSlots).Count -ne 4) {
        throw 'Supply four public slot files and a new PlanFile path'
    }
    $Harness = Resolve-PinnedFile $Harness $HarnessSha256
    $public = @{}
    if ($NodeSha256 -cnotmatch '^[a-f0-9]{64}$' -or $AoemSha256 -cnotmatch '^[a-f0-9]{64}$' -or
        $CtlSha256 -cnotmatch '^[a-f0-9]{64}$' -or
        ($EndUnix - $StartUnix) -notin 60..1800) {
        throw 'Supply approved binary pins and a 60–1800 second UTC window'
    }
    foreach ($file in $PublicSlots) {
        $data = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
        if ($data.run_id -cne $RunId -or $data.chain_id -ne $ChainId -or $data.role -cnotin $slots -or
            $data.aoem_sha256 -cne $AoemSha256 -or $data.ctl_sha256 -cne $CtlSha256 -or
            $public.ContainsKey($data.role)) {
            throw 'Conflicting public slot material'
        }
        $public[$data.role] = @{
            role = $data.role; address = $data.address; validator = $data.validator; transport = $data.transport
        }
    }
    $PlanFile = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($PlanFile)
    Write-NewJson $PlanFile @{
        run_id = $RunId; chain_id = $ChainId; start_unix = $StartUnix; end_unix = $EndUnix
        node_sha256 = $NodeSha256; aoem_sha256 = $AoemSha256; harness_sha256 = $HarnessSha256; ctl_sha256 = $CtlSha256
        slots = @($slots | ForEach-Object { $public[$_] })
    }
    $PlanSha256 = (Get-FileHash -LiteralPath $PlanFile).Hash.ToLowerInvariant()
    Invoke-Harness 'validate' "$PlanFile.genesis.json"
    Get-FileHash -LiteralPath $PlanFile, "$PlanFile.genesis.json"
    return
}

if ($Action -ne 'Keys') {
    $PlanFile = Resolve-PinnedFile $PlanFile $PlanSha256
    $plan = Get-Content -LiteralPath $PlanFile -Raw | ConvertFrom-Json
    $RunId = $plan.run_id
    $ChainId = $plan.chain_id
    Confirm-Domain
    $Harness = Resolve-PinnedFile $Harness $plan.harness_sha256
    $AoemSha256 = $plan.aoem_sha256
    if ($Action -eq 'Validate') {
        if (-not $Evidence) { throw 'Provide an unused Evidence label' }
        Invoke-Harness 'validate' "$PlanFile.$Evidence.genesis.json"
        return
    }
}

if (-not $Role) { throw 'Select an isolated validator slot' }
$base = Join-Path $env:LOCALAPPDATA 'NOVOVM-Isolated-Catchup'
$root = Join-Path $base $RunId
$slotPath = Join-Path $root $Role
if ($Action -eq 'Keys') {
    Confirm-Domain
    Confirm-LanBind $Address
    $Ctl = Resolve-PinnedFile $Ctl $CtlSha256
    $Aoem = Resolve-PinnedFile $Aoem $AoemSha256
    foreach ($directory in @($base, $root)) {
        if (-not (Test-Path -LiteralPath $directory)) { $null = New-Item -ItemType Directory -Path $directory }
        Confirm-Directory $directory
    }
    $null = New-Item -ItemType Directory -Path $slotPath
    Confirm-Directory $slotPath
    $validator = Invoke-PurposeKey generate validator (Join-Path $slotPath 'validator.sealed')
    $transport = Invoke-PurposeKey generate transport (Join-Path $slotPath 'transport.sealed')
    if ($Role -ne 'A2') {
        $null = Invoke-PurposeKey generate transport (Join-Path $slotPath 'operator.sealed')
    }
    $publicFile = Join-Path $slotPath 'public.json'
    Write-NewJson $publicFile @{
        run_id = $RunId; chain_id = $ChainId; role = $Role; address = $Address
        validator = $validator; transport = $transport; aoem_sha256 = $AoemSha256; ctl_sha256 = $CtlSha256
    }
    Get-FileHash -LiteralPath $publicFile
    return
}

Confirm-Directory $base
Confirm-Directory $root
if ($Action -in @('Ready','Create','Existing','Stop','Archive')) { Confirm-Directory $slotPath }
$selected = @($plan.slots | Where-Object role -CEQ $Role)
if ($selected.Count -ne 1) { throw 'Slot missing from pinned plan' }
$stopPath = Join-Path $slotPath 'stop'
if ($Action -eq 'Stop') {
    if (-not (Test-Path -LiteralPath (Join-Path $slotPath 'launch.lock'))) {
        throw 'No owned launch is registered for this slot'
    }
    Write-NewJson $stopPath @{ plan_sha256 = $PlanSha256; role = $Role }
    return
}

$Aoem = Resolve-PinnedFile $Aoem $AoemSha256
if ($Action -in @('Ready','Create','Existing')) {
    $Node = Resolve-PinnedFile $Node $plan.node_sha256
}
if ($Action -eq 'Ready') {
    $Ctl = Resolve-PinnedFile $Ctl $plan.ctl_sha256
    $slot = $selected[0]
    Confirm-LanBind $slot.address
    $validator = Invoke-PurposeKey inspect validator (Join-Path $slotPath 'validator.sealed')
    $transport = Invoke-PurposeKey inspect transport (Join-Path $slotPath 'transport.sealed')
    if (($validator -join ',') -cne ($slot.validator -join ',') -or
        ($transport -join ',') -cne ($slot.transport -join ',')) {
        throw 'DPAPI identity differs from the agreed public plan'
    }
    if ($Role -ne 'A2') {
        $operator = Invoke-PurposeKey inspect transport (Join-Path $slotPath 'operator.sealed')
        foreach ($peer in $plan.slots) {
            if (($operator -join ',') -in @(($peer.validator -join ','),($peer.transport -join ','))) {
                throw 'Operator key matches a validator identity'
            }
        }
    }
    $genesisFile = Join-Path $slotPath "ready-genesis-$PlanSha256.json"
    Invoke-Harness 'preflight' $genesisFile
    Write-NewJson (Join-Path $slotPath "ready-$PlanSha256.json") @{
        plan_sha256 = $PlanSha256; run_id = $RunId; chain_id = $ChainId; role = $Role
        address = $slot.address; genesis_sha256 = (Get-FileHash -LiteralPath $genesisFile).Hash.ToLowerInvariant()
        node_sha256 = $plan.node_sha256; aoem_sha256 = $plan.aoem_sha256; harness_sha256 = $plan.harness_sha256
        ctl_sha256 = $plan.ctl_sha256
        utc = [DateTimeOffset]::UtcNow.ToString('o')
    }
    Get-Item -LiteralPath (Join-Path $slotPath "ready-$PlanSha256.json")
    return
}
if ($Action -in @('Create','Existing','Query','Submit')) {
    $owner = if ($Action -in @('Query','Submit')) { $Actor } else { $Role }
    if (-not $owner) { throw 'Specify the local operator slot with -Actor' }
    $ownerPath = Join-Path $root $owner
    Confirm-Directory $ownerPath
    $ready = Get-Content -LiteralPath (Join-Path $ownerPath "ready-$PlanSha256.json") -Raw | ConvertFrom-Json
    if ($ready.plan_sha256 -cne $PlanSha256 -or $ready.role -cne $owner -or
        $ready.run_id -cne $RunId -or $ready.chain_id -ne $ChainId) {
        throw 'Local readiness receipt does not match the pinned plan'
    }
}
if (-not $Evidence) { throw 'Provide an unused Evidence label' }
$reportDir = Join-Path $root 'evidence'
if (-not (Test-Path -LiteralPath $reportDir)) { $null = New-Item -ItemType Directory -Force -Path $reportDir }
Confirm-Directory $reportDir
$report = Join-Path $reportDir "$Evidence.json"
if (Test-Path -LiteralPath $report) { throw 'Evidence label was already used' }
Invoke-Harness $Action.ToLowerInvariant() $report
Get-Item -LiteralPath $report
