#Requires -Version 7.5
[CmdletBinding(DefaultParameterSetName='Run')]
param(
    [Parameter(Mandatory,ParameterSetName='Run')][Parameter(Mandatory,ParameterSetName='Replay')][string]$ClientEvidence,
    [Parameter(Mandatory,ParameterSetName='Run')][Parameter(Mandatory,ParameterSetName='Replay')][string]$CaptureEvidence,
    [Parameter(Mandatory,ParameterSetName='Run')][Parameter(Mandatory,ParameterSetName='Replay')][string]$Manifest,
    [Parameter(Mandatory,ParameterSetName='Run')][Parameter(Mandatory,ParameterSetName='Replay')][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ManifestSha256,
    [Parameter(Mandatory,ParameterSetName='Run')][ValidateRange(1,9000000000000000000)][long]$MinLocatorSequence,
    [Parameter(Mandatory,ParameterSetName='Run')][string]$Client,
    [Parameter(Mandatory,ParameterSetName='Run')][Parameter(Mandatory,ParameterSetName='Replay')][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ClientSha256,
    [Parameter(Mandatory,ParameterSetName='Run')][string]$PeerWitness,
    [Parameter(ParameterSetName='Run')][uri]$PeerWitnessUri,
    [Parameter(ParameterSetName='Run')][string]$PeerCaCertificate,
    [Parameter(ParameterSetName='Run')][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$PeerCaSha256,
    [Parameter(Mandatory,ParameterSetName='Run')][string]$HotspotPreflight,
    [Parameter(ParameterSetName='Run')][switch]$ProbeHotspotIfMissing,
    [Parameter(Mandatory,ParameterSetName='Run')][Parameter(Mandatory,ParameterSetName='Replay')][int]$WiredIndex,
    [Parameter(Mandatory,ParameterSetName='Run')][Parameter(Mandatory,ParameterSetName='Replay')][int]$HotspotIndex,
    [Parameter(ParameterSetName='Run')][ValidateRange(60,300)][int]$MinSwitchSeconds = 90,
    [Parameter(Mandatory,ParameterSetName='Replay')][switch]$ReplayStartup,
    [Parameter(ParameterSetName='Replay')][string]$OriginalCaptureDirectory,
    [Parameter(Mandatory,ParameterSetName='Offline')][switch]$OfflineCheck
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Read-Json($Path) {
    Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json -AsHashtable -DateKind String
}

function Test-Sha256($Left,$Right) {
    $Left -is [string] -and $Right -is [string] -and
        $Left -cmatch '\A[0-9a-fA-F]{64}\z' -and
        [string]::Equals($Left,$Right,[StringComparison]::OrdinalIgnoreCase)
}

function Test-StateRoot($Left,$Right) {
    $Left -is [string] -and $Right -is [string] -and $Left.StartsWith('0x') -and $Right.StartsWith('0x') -and
        (Test-Sha256 $Left.Substring(2) $Right.Substring(2))
}

function Read-Events($Directory) {
    $file = Join-Path $Directory 'stdout.jsonl'
    if (!(Test-Path -LiteralPath $file -PathType Leaf)) { return }
    $f = [IO.File]::Open($file,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
    try {
        if ($f.Length -gt 1048576) { throw 'Unexpected JSONL size' }
        $r = [IO.StreamReader]::new($f,[Text.UTF8Encoding]::new($false,$true))
        try { $raw = $r.ReadToEnd() } finally { $r.Dispose() }
    } finally { $f.Dispose() }
    $lines = @($raw.Split("`n"))
    for ($i = 0; $i -lt $lines.Count - 1; $i++) {
        $row = ConvertFrom-Json -InputObject $lines[$i] -AsHashtable -DateKind String
        if (!(Test-Sha256 $row.transport_identity_sha256 $localPin)) { throw 'Unexpected Client identity in JSONL' }
        if (($i -eq 0 -and ($row.event -cne 'starting' -or $row.reopened_height -ne 0)) -or
            ($i -gt 0 -and $row.event -ceq 'starting')) { throw 'Not one cold Client run' }
        $row
    }
}

function Find-Baseline($Rows, $Native, [long]$StartedMs) {
    for ($i = 1; $i -lt $Rows.Count; $i++) {
        $r = $Rows[$i]
        if ($r.event -ceq 'verified_state' -and $r.transport -ceq 'native_novorudp' -and
            $r.native_peer_address -ceq $Native -and (Test-Sha256 $r.peer_transport_sha256 $peerPin) -and
            $r.height -eq $m.height -and (Test-StateRoot $r.state_root $stateRoot) -and $r.state_version -eq $m.state_version -and
            $r.verified_nodes -gt 0 -and $r.verified_leaves -gt 0 -and $r.downloaded_nodes -eq $r.verified_nodes -and
            $r.utc_unix_ms -ge $StartedMs) { return @{ row = $r; index = $i } }
    }
}

function Find-Recovery($Rows, $Base, [long]$DownMs, [long]$MinSequence) {
    for ($i = $Base.index + 1; $i -lt $Rows.Count; $i++) {
        $r = $Rows[$i]
        if ($r.event -cne 'verified_state' -or $r.transport -cne 'iroh' -or
            !(Test-Sha256 $r.peer_transport_sha256 $peerPin) -or $r.height -ne $m.height -or !(Test-StateRoot $r.state_root $stateRoot) -or
            $r.state_version -ne $m.state_version -or $r.verified_nodes -ne $Base.row.verified_nodes -or $r.verified_leaves -ne $Base.row.verified_leaves -or
            $r.locator_sequence -lt $MinSequence -or
            ($null -ne $Base.row.locator_sequence -and $r.locator_sequence -le $Base.row.locator_sequence)) { continue }
        $matches = @($Rows[($Base.index+1)..$i] | Where-Object {
            $_.event -ceq 'verified_locator_accepted' -and (Test-Sha256 $_.peer_transport_sha256 $peerPin) -and
                $_.locator_sequence -eq $r.locator_sequence
        })
        if ($matches.Count -ne 1) { continue }
        $a = $matches[0]
        if ($a.locator_issued_at_ms -gt $DownMs -and $a.utc_unix_ms -ge $a.locator_issued_at_ms -and
            $r.utc_unix_ms -ge $a.utc_unix_ms -and $r.utc_unix_ms -lt $a.locator_expires_at_ms) {
            return @{ row = $r; index = $i; accepted = $a }
        }
    }
}

function Test-WiredRoute($Record, $Start) {
    $Record.phase -ceq 'before' -and $Record.pid -eq $Start.pid -and $Record.started_utc -ceq $Start.utc -and
        $Record.route_status -ceq 'selected' -and $Record.physical_adapter -eq $true -and
        $Record.client_running_at_end -eq $true -and $Record.interface_index -eq $WiredIndex -and
        $Record.peer_ip -ceq ([Net.IPEndPoint]::Parse($m.native_gateway).Address.ToString()) -and
        @($Record.owned_sockets | Where-Object { $_.transport -ceq 'udp' -and $_.local_port -gt 0 }).Count -gt 0
}

function Test-ComponentMap($Capture) {
    $w = @($Capture.components | Where-Object if_index -EQ $WiredIndex)
    $s = @($Capture.components | Where-Object if_index -EQ $HotspotIndex)
    $Capture.armed -eq $true -and $w.Count -eq 1 -and $s.Count -eq 1 -and
        $w[0].component_id -gt 0 -and $s[0].component_id -gt 0 -and $w[0].component_id -ne $s[0].component_id
}

function Get-MediaStatus($Nic) {
    if ($Nic.InterfaceAdminStatus -ne 1) { throw 'Physical adapter administratively disabled' }
    switch ("$($Nic.Status)/$($Nic.MediaConnectionState)") {
        'Up/Connected' { 'Up' }
        'Disconnected/Disconnected' { 'Down' }
        default { throw 'Physical adapter media state unknown' }
    }
}

function Read-PhysicalLinks {
    $begin = [datetimeoffset]::UtcNow
    $rows = @(Get-NetAdapter -Physical -ErrorAction Stop | Where-Object ifIndex -in $WiredIndex,$HotspotIndex)
    $states = @{}
    foreach ($i in $WiredIndex,$HotspotIndex) {
        $nic = @($rows | Where-Object ifIndex -eq $i)
        if ($nic.Count -ne 1 -or [guid]$nic[0].InterfaceGuid -ne $guid[$i] -or
            $nic[0].InterfaceAdminStatus -ne 1) { throw "Physical adapter missing, replaced or disabled: $i" }
        $states[$i] = Get-MediaStatus $nic[0]
    }
    @{ Begin=$begin; UTC=[datetimeoffset]::UtcNow; Wired=$states[$WiredIndex]; Hotspot=$states[$HotspotIndex] }
}

function Get-LinkEdge($Previous,$Current) {
    if ($Previous.Begin -gt $Previous.UTC -or $Previous.UTC -gt $Current.Begin -or $Current.Begin -gt $Current.UTC) {
        throw 'Observation clock regressed'
    }
    if ($Previous.Wired -eq $Current.Wired) { return }
    @{ previous=$Previous.Wired; current=$Current.Wired; lower_utc=$Previous.Begin.ToString('o');
        upper_utc=$Current.UTC.ToString('o'); upper_unix_ms=$Current.UTC.ToUnixTimeMilliseconds() }
}

function Test-PeerWitness($Witness,$Route,$Start,$Baseline) {
    $sender = [Net.IPEndPoint]::Parse($Witness.source)
    $src = $sender.Address.ToString()
    $ports = @($Route.owned_sockets | Where-Object { $_.transport -ceq 'udp' -and
        $_.local_port -eq $sender.Port -and $_.local_ip -in @($src,'0.0.0.0','::') })
    $Witness.event -ceq 'first_scoped_wired_datagram' -and (Test-Sha256 $Witness.manifest_sha256 $ManifestSha256) -and
        $Witness.destination -ceq $m.native_gateway -and $src -ceq $Route.source_ip -and $ports.Count -gt 0 -and
        $Witness.observed_utc_ms -ge ([datetimeoffset]::Parse($Start.utc).ToUnixTimeMilliseconds()) -and
        $Witness.observed_utc_ms -le $Baseline.row.utc_unix_ms
}

function Test-HotspotPreflight($Probe,$Source,$Gateway,$Now) {
    $age = $Now.ToUnixTimeMilliseconds() - [long]$Probe.observed_utc_ms
    $names = @($m.approved_relays | ForEach-Object { ([uri]$_).DnsSafeHost })
    $Probe.relay_host -in $names -and (Test-Sha256 $Probe.manifest_sha256 $ManifestSha256) -and $Probe.if_index -eq $HotspotIndex -and
        [guid]$Probe.interface_guid -eq $guid[$HotspotIndex] -and
        $Probe.source_ip -ceq $Source -and $Probe.gateway -ceq $Gateway -and
        $Probe.relay_ip -in $cap.approved_relay_ips -and $Probe.curl_exit -eq 0 -and
        $Probe.raw -cmatch '^http=([1-5][0-9]{2}) local=(\S+) remote=(\S+)(?: connect=[0-9]+(?:\.[0-9]+)?s)? tls=([0-9]+(?:\.[0-9]+)?)s?$' -and
        $Matches[2] -ceq $Source -and $Matches[3] -ceq $Probe.relay_ip -and
        [double]::Parse($Matches[4],[cultureinfo]::InvariantCulture) -gt 0 -and $age -ge 0 -and $age -le 900000
}

# Only the explicit opted-in future preflight sends one source-bound HTTPS HEAD.
function Receive-PeerWitness {
    if (!$PeerWitnessUri -or (Test-Path -LiteralPath $PeerWitness) -or [datetimeoffset]::UtcNow -lt $script:nextPeerPoll) { return }
    $script:nextPeerPoll = [datetimeoffset]::UtcNow.AddSeconds(1)
    $tmp = "$PeerWitness.$PID.pending"
    if (Test-Path -LiteralPath $tmp) { throw 'Previous witness transfer preserved' }
    try {
        $http = & "$env:SystemRoot/System32/curl.exe" --disable --noproxy '*' --proto '=https' --silent --show-error --connect-timeout 2 --max-time 3 --max-filesize 4096 --cacert $PeerCaCertificate --output $tmp --write-out '%{http_code}' $PeerWitnessUri.AbsoluteUri
        if ($LASTEXITCODE -ne 0) { throw 'Private witness transfer failed' }
        if ($http -eq '503') { return }
        if ($http -ne '200') { throw "Unexpected witness HTTP status $http" }
        [IO.File]::Move($tmp,$PeerWitness)
    } finally { if (Test-Path -LiteralPath $tmp) { [IO.File]::Delete($tmp) } }
}

function Save-HotspotPreflight($Configuration) {
    $source = @($Configuration.IPv4Address.IPAddress)
    $gateway = @($Configuration.IPv4DefaultGateway.NextHop)
    if ($source.Count -ne 1 -or $gateway.Count -ne 1) { throw 'Hotspot IPv4 path is ambiguous' }
    $relay = $null
    foreach ($candidate in $m.approved_relays) {
        $uri = [uri]$candidate
        if ($uri.Scheme -ne 'https' -or $uri.Port -ne 443) { continue }
        $addresses = @(Resolve-DnsName -DnsOnly -Type A -Name $uri.DnsSafeHost | Where-Object { $_.IPAddress -in $cap.approved_relay_ips })
        if ($addresses.Count) { $relay = $uri.DnsSafeHost; $ip = $addresses[0].IPAddress; break }
    }
    if (!$relay) { throw 'No approved relay matches the scoped capture addresses' }
    $routes = @(Find-NetRoute -RemoteIPAddress $ip -LocalIPAddress $source[0])
    if (!($routes | Where-Object { $_.InterfaceIndex -eq $HotspotIndex -and $_.CimClass.CimClassName -eq 'MSFT_NetRoute' })) {
        throw 'No route from the hotspot source to the approved relay'
    }
    $raw = & "$env:SystemRoot/System32/curl.exe" --disable --ipv4 --noproxy '*' --head --silent --show-error --connect-timeout 4 --max-time 7 --interface $source[0] --resolve "${relay}:443:${ip}" --output NUL --write-out 'http=%{http_code} local=%{local_ip} remote=%{remote_ip} tls=%{time_appconnect}' "https://$relay/"
    $exitCode = $LASTEXITCODE
    $now = [datetimeoffset]::UtcNow
    $probe = @{manifest_sha256=$ManifestSha256;if_index=$HotspotIndex;interface_guid=$guid[$HotspotIndex];
        source_ip=$source[0];gateway=$gateway[0];relay_host=$relay;relay_ip=$ip;curl_exit=$exitCode;raw=[string]$raw;observed_utc_ms=$now.ToUnixTimeMilliseconds()}
    $file = [IO.File]::Open($HotspotPreflight,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
    try { $file.Write([Text.Encoding]::UTF8.GetBytes(($probe | ConvertTo-Json))) } finally { $file.Dispose() }
}

function Assert-StartupEvidence($Cap,$Start,$ManifestData,$ManifestFileDigest,$ExpectedCaptureDirectory) {
    if (!(Test-Sha256 $ManifestFileDigest $ManifestSha256)) { throw 'Manifest file SHA-256 differs' }
    if (!(Test-Sha256 $Cap.manifest_sha256 $ManifestSha256)) { throw 'Capture manifest SHA-256 differs' }
    if (!(Test-Sha256 $Start.client_sha256 $ClientSha256)) { throw 'Client start executable SHA-256 differs' }
    if (!(Test-Sha256 $Start.profile_sha256 $ManifestData.profile_sha256)) { throw 'Client start profile SHA-256 differs from the pinned manifest' }
    if ($WiredIndex -eq $HotspotIndex -or !(Test-ComponentMap $Cap)) { throw 'Distinct physical capture components required' }
    if (!(Test-Sha256 $ManifestData.client_transport_sha256 $ManifestData.client_transport_sha256) -or
        !(Test-Sha256 $ManifestData.server_transport_sha256 $ManifestData.server_transport_sha256)) { throw 'Invalid manifest transport fingerprint' }
    if ($ManifestData.height -le 0 -or !(Test-StateRoot $ManifestData.state_root $ManifestData.state_root)) { throw 'Invalid manifest height or state root' }
    if ($Cap.etl -ine (Join-Path $ExpectedCaptureDirectory 'physical.etl')) { throw 'Unexpected capture ETL path' }
    if ([datetimeoffset]::Parse($Cap.ready_utc) -gt [datetimeoffset]::Parse($Start.utc)) { throw 'Client started before capture readiness' }
}

if ($ReplayStartup) {
    $captureDirectory = if ($OriginalCaptureDirectory) { $OriginalCaptureDirectory } else { $CaptureEvidence }
    Assert-StartupEvidence (Read-Json (Join-Path $CaptureEvidence 'ready.json')) (Read-Json (Join-Path $ClientEvidence 'start.json')) (Read-Json $Manifest) (Get-FileHash -LiteralPath $Manifest).Hash $captureDirectory
    'Archived capture, Client startup and pinned manifest bindings passed (offline).'
    return
}

if ($OfflineCheck) { . (Join-Path $PSScriptRoot "native-roaming-link-tests.ps1"); return }

function Assert-LiveRun {
    if ([datetimeoffset]::UtcNow -ge $deadline.AddSeconds(-8)) { throw 'Local test deadline' }
    if ([DRoamingConsole]::WTSGetActiveConsoleSessionId() -ne $session -or
        (Get-Process -Id $cap.admin_pid -ErrorAction Stop).StartTime.ToUniversalTime() -ne $captureStart -or
        (Test-Path -LiteralPath (Join-Path $CaptureEvidence 'result.json'))) {
        throw 'Console control or capture continuity is unconfirmed'
    }
    $p = Get-Process -Id $start.pid -ErrorAction Stop
    if ($p.Path -ine $client -or $p.StartTime.ToUniversalTime().ToString('o') -cne $start.utc -or
        (Test-Path -LiteralPath (Join-Path $ClientEvidence 'end.json'))) {
        throw 'Client continuity failed'
    }
}

function Request-Marker($Name) {
    $p = Join-Path $ClientEvidence "$Name.request"
    if (!(Test-Path -LiteralPath $p)) { New-Item -ItemType File -Path $p | Out-Null }
}

function Journal($Kind,$Data) {
    [ordered]@{ event=$Kind; observed_utc=[datetime]::UtcNow.ToString('o'); data=$Data } |
        ConvertTo-Json -Depth 5 -Compress | Add-Content -LiteralPath $journal
}

$journal = $null
$prompted = $false
$lastDown = $null
$previous = $null
$phase = "visible_console"
$diagnostic = Join-Path $ClientEvidence ("observer-startup-{0}.jsonl" -f $PID)
$diagnosticFile = [IO.File]::Open($diagnostic,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
$diagnosticFile.Dispose()
function Write-Diagnostic($Event,$Data) {
    [ordered]@{event=$Event;phase=$phase;observed_utc=[datetime]::UtcNow.ToString('o');data=$Data} | ConvertTo-Json -Depth 4 -Compress | Add-Content -LiteralPath $diagnostic
}
Write-Diagnostic 'observer_started' @{pid=$PID}
try {
    if (![Environment]::UserInteractive -or [Console]::IsOutputRedirected) {
        throw 'Open this helper in a visible local interactive console'
    }
    Add-Type -TypeDefinition @'
using System.Runtime.InteropServices;
public static class DRoamingConsole {
    [DllImport("kernel32.dll")] public static extern uint WTSGetActiveConsoleSessionId();
}
'@
    $session = (Get-Process -Id $PID).SessionId
    if ($session -eq 0 -or [DRoamingConsole]::WTSGetActiveConsoleSessionId() -ne $session) {
        throw 'Not the physical console session'
    }
    $phase = 'read_capture_start_manifest'
    $cap = Read-Json (Join-Path $CaptureEvidence 'ready.json')
    $start = Read-Json (Join-Path $ClientEvidence 'start.json')
    $m = Read-Json $Manifest
    $script:nextPeerPoll = [datetimeoffset]::MinValue
    $phase = 'peer_certificate'
    if ($PeerWitnessUri) {
        if ($PeerWitnessUri.Scheme -ne 'https' -or $PeerWitnessUri.UserInfo -or $PeerWitnessUri.Query -or $PeerWitnessUri.Fragment -or
            $PeerWitnessUri.AbsolutePath -cne '/wired-source' -or $PeerWitnessUri.Host -ne ([Net.IPEndPoint]::Parse($m.native_gateway).Address.ToString()) -or !$PeerCaCertificate -or !$PeerCaSha256) { throw 'Unexpected private witness endpoint or missing certificate pin' }
        $cert = [Security.Cryptography.X509Certificates.X509CertificateLoader]::LoadCertificateFromFile((Get-Item -LiteralPath $PeerCaCertificate).FullName)
        try { if (!(Test-Sha256 $cert.GetCertHashString([Security.Cryptography.HashAlgorithmName]::SHA256) $PeerCaSha256)) { throw 'Peer certificate digest differs' } } finally { $cert.Dispose() }
    }
    $phase = 'startup_evidence'
    Assert-StartupEvidence $cap $start $m (Get-FileHash -LiteralPath $Manifest).Hash $CaptureEvidence
    $localPin = $m.client_transport_sha256.ToLowerInvariant()
    $peerPin = $m.server_transport_sha256.ToLowerInvariant()
    $stateRoot = $m.state_root
    $phase = 'local_files'
    $Client = (Get-Item -LiteralPath $Client).FullName
    $deadline = [datetimeoffset]::Parse($cap.deadline_utc)
    if (!(Test-Path -LiteralPath $cap.etl -PathType Leaf)) { throw 'Local capture ETL missing' }
    if (!(Test-Sha256 (Get-FileHash -LiteralPath $Client).Hash $ClientSha256)) { throw 'Local Client executable SHA-256 differs' }
    $phase = 'capture_owner'
    $captureStart = (Get-Process -Id $cap.admin_pid).StartTime.ToUniversalTime()
    $phase = 'physical_adapters'
    foreach ($name in 'route-after.request','route-after.json','operator.jsonl') {
        if (Test-Path -LiteralPath (Join-Path $ClientEvidence $name)) { throw "Preserve existing $name" }
    }
    $guid = @{}
    Get-NetAdapter -Physical | Where-Object ifIndex -in $WiredIndex,$HotspotIndex | ForEach-Object {
        if (($_.ifIndex -eq $WiredIndex -and $_.MediaType -ne '802.3') -or
            ($_.ifIndex -eq $HotspotIndex -and $_.MediaType -ne 'Native 802.11')) { throw 'Unexpected physical adapter medium' }
        $guid[[int]$_.ifIndex] = [guid]$_.InterfaceGuid
    }
    if ($guid.Count -ne 2) { throw 'Physical adapter mapping not present' }
    $journalPath = Join-Path $ClientEvidence 'operator.jsonl'
    New-Item -ItemType File -Path $journalPath -ErrorAction Stop | Out-Null
    $journal = $journalPath
    $phase = 'observe_client_and_links'
    Write-Diagnostic 'startup_complete' @{pid=$start.pid}
    $startedMs = [datetimeoffset]::Parse($start.utc).ToUnixTimeMilliseconds()
    Write-Host '保持有线和热点连接，正在验证基线。' -ForegroundColor Yellow
    do {
        Assert-LiveRun
        $links = Read-PhysicalLinks
        if ($links.Wired -ne 'Up' -or $links.Hotspot -ne 'Up') { throw 'Both links must remain Up before the prompt' }
        if (($deadline - $links.UTC).TotalSeconds -lt $MinSwitchSeconds) { throw 'Insufficient time to switch and roll back' }
        $baseline = Find-Baseline @(Read-Events $ClientEvidence) $m.native_gateway $startedMs
        if ($baseline -and $baseline.row.utc_unix_ms -le $links.UTC.ToUnixTimeMilliseconds()) {
            Request-Marker 'route-before'
            $marker = Join-Path $ClientEvidence 'route-before.json'
            if (Test-Path -LiteralPath $marker) {
                $route = Read-Json $marker
                if (!(Test-WiredRoute $route $start) -or [datetimeoffset]::Parse($route.observed_utc).ToUnixTimeMilliseconds() -lt $baseline.row.utc_unix_ms) { throw 'Wired route or socket marker did not match the Client' }
                Receive-PeerWitness
                if (Test-Path -LiteralPath $PeerWitness -PathType Leaf) {
                    $witness = Read-Json $PeerWitness
                    if (!(Test-PeerWitness $witness $route $start $baseline)) { throw 'Peer UDP observation differs from this Client and run' }
                    break
                }
            }
        }
        Start-Sleep -Milliseconds 200
    } while ($true)
    $wifi = Get-NetIPConfiguration -InterfaceIndex $HotspotIndex
    if (!(Test-Path -LiteralPath $HotspotPreflight -PathType Leaf) -and $ProbeHotspotIfMissing) { Save-HotspotPreflight $wifi }
    $probe = Read-Json $HotspotPreflight
    $sources = @($wifi.IPv4Address.IPAddress | Where-Object { $_ -ceq $probe.source_ip })
    $gateways = @($wifi.IPv4DefaultGateway.NextHop | Where-Object { $_ -ceq $probe.gateway })
    if ($sources.Count -ne 1 -or $gateways.Count -ne 1 -or
        !(Test-HotspotPreflight $probe $sources[0] $gateways[0] ([datetimeoffset]::UtcNow))) {
        throw 'Hotspot source-bound TLS preflight is absent, expired, or differs from the current adapter'
    }
    Assert-LiveRun
    $links = Read-PhysicalLinks
    if ($links.Wired -ne 'Up' -or $links.Hotspot -ne 'Up' -or
        ($deadline - $links.UTC).TotalSeconds -lt $MinSwitchSeconds) { throw 'Switch gate no longer valid' }
    Journal 'wired_ready' @{pid=$start.pid;started_utc=$start.utc;line=$baseline.index;height=$m.height;state_root=$stateRoot;
        wired_index=$WiredIndex;hotspot_index=$HotspotIndex;wired_guid=$guid[$WiredIndex];hotspot_guid=$guid[$HotspotIndex];component_map=$cap.components;
        peer_witness=$witness;hotspot_probe=$probe;initial_up_utc=$links.UTC.ToString('o')}
    Write-Host "现在拔线，保留热点。最迟 $($deadline.AddSeconds(-8).ToLocalTime().ToString('HH:mm:ss')) 插回。不要等聊天消息。" -ForegroundColor Cyan
    $prompted = $true
    $previous = $links
    do {
        Assert-LiveRun
        $links = Read-PhysicalLinks
        if ($links.Hotspot -ne 'Up') { throw 'Hotspot no longer Up' }
        $edge = Get-LinkEdge $previous $links
        if ($edge -and $edge.current -eq 'Down') { break }
        if ($links.Wired -ne 'Up') { throw 'Wired link state ambiguous' }
        $previous = $links
        Start-Sleep -Milliseconds 150
    } while ($true)
    $previous = $links
    $lastDown = $links.Begin
    $downMs = $links.UTC.ToUnixTimeMilliseconds()
    Journal 'wired_down_observed' @{last_up_utc=$edge.lower_utc;first_down_utc=$links.UTC.ToString('o');upper_bound_unix_ms=$downMs}
    do {
        Assert-LiveRun
        $links = Read-PhysicalLinks
        if ($links.Wired -eq 'Down') { $lastDown = $links.Begin; $previous = $links }
        if ($links.Wired -ne 'Down' -or $links.Hotspot -ne 'Up') { throw 'Physical state changed before recovery evidence completed' }
        $recovered = Find-Recovery @(Read-Events $ClientEvidence) $baseline $downMs $MinLocatorSequence
        if ($recovered) {
            Request-Marker 'route-after'
            $marker = Join-Path $ClientEvidence 'route-after.json'
            if (Test-Path -LiteralPath $marker) {
                $after = Read-Json $marker
                if ($after.pid -ne $start.pid -or $after.started_utc -cne $start.utc -or
                    $after.route_status -cne 'selected' -or $after.physical_adapter -ne $true -or
                    $after.phase -cne 'after' -or $after.source_ip -cne $probe.source_ip -or
                    $after.peer_ip -notin $cap.approved_relay_ips -or
                    [datetimeoffset]::Parse($after.observed_utc).ToUnixTimeMilliseconds() -lt $recovered.row.utc_unix_ms -or
                    $after.interface_index -ne $HotspotIndex -or $after.client_running_at_end -ne $true) {
                    throw 'Hotspot route corroboration does not match the original Client'
                }
                Journal 'recovery_observed' @{pid=$start.pid;line=$recovered.index;locator_sequence=$recovered.row.locator_sequence;
                    verification='client_events_and_route_only';packet_verification='separate'}
                break
            }
        }
        Start-Sleep -Milliseconds 150
    } while ($true)
} catch {
    Write-Diagnostic 'observer_failed' @{reason=$_.Exception.Message;line=$_.InvocationInfo.ScriptLineNumber;type=$_.Exception.GetType().Name}
    if ($journal) { Journal 'observation_failed' @{reason=$_.Exception.Message} }
    throw
} finally {
    Write-Host '现在插回有线（如已拔下），保留热点。不要等待远端消息。' -ForegroundColor Yellow
    if ($prompted) {
        Journal 'restore_prompted' @{}
        $wait = [Diagnostics.Stopwatch]::StartNew()
        $restored = $false
        $hotspotRestored = $false
        do {
            try {
                $s = Read-PhysicalLinks
                $edge = if ($previous) { Get-LinkEdge $previous $s }
                if ($edge -and $edge.current -eq 'Down') { Journal 'wired_down_during_restore' $edge }
                if ($s.Wired -eq 'Down') { $lastDown = $s.Begin }
                $previous = $s
                if ($s.Wired -eq 'Up') {
                    $event = if ($lastDown) { 'wired_up_observed' } else { 'wired_up_without_observed_down' }
                    Journal $event @{last_down_utc=if($lastDown){$lastDown.ToString('o')};first_up_utc=$s.UTC.ToString('o');hotspot=$s.Hotspot}
                    $restored = $true
                    $hotspotRestored = $s.Hotspot -eq 'Up'
                    break
                }
            } catch { Write-Verbose $_.Exception.Message }
            Start-Sleep -Milliseconds 200
        } while ($wait.Elapsed.TotalSeconds -lt 30)
        if (!$restored) {
            Journal 'wired_restore_unconfirmed' @{}
            Write-Host '未观测到有线恢复。请检查网线和本机接口。' -ForegroundColor Red
            throw 'Wired restoration not observed'
        }
        if (!$hotspotRestored) { throw 'Ethernet restored; hotspot was no longer Up' }
    }
}
