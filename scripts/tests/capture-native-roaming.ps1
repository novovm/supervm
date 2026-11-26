#Requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Client,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ClientSha256,
    [Parameter(Mandatory)][string]$Profile,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$ProfileSha256,
    [Parameter(Mandatory)][string]$IsolatedLocalAppData,
    [Parameter(Mandatory)][string]$EvidenceDir,
    [ValidateRange(30,900)][int]$Seconds = 240,
    [System.Net.IPAddress]$PeerIp,
    [System.Net.IPAddress]$BeforePeerIp,
    [System.Net.IPAddress]$AfterPeerIp
)
$ErrorActionPreference = 'Stop'

function Get-RoamingRouteEvidence {
    param([System.Net.IPAddress]$Peer, [Diagnostics.Process]$Process, [string]$StartedUtc, [string]$Phase)

    $family = if ($Peer.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetwork) { 'IPv4' } else { 'IPv6' }
    $record = [ordered]@{
        phase = $Phase
        observed_utc = [datetime]::UtcNow.ToString('o')
        pid = $Process.Id
        started_utc = $StartedUtc
        peer_ip = $Peer.ToString()
        address_family = $family
        evidence_kind = 'windows_route_selection_corroboration_only'
        actual_packet_interface = 'not_observed'
        route_status = 'unavailable'
        interface_index = $null
        source_ip = $null
        destination_prefix = $null
        next_hop = $null
        default_gateways = @()
        physical_adapter = $null
        error = $null
        owned_sockets = @()
        socket_error = $null
    }
    try {
        if ($Process.HasExited) { throw 'Client was not running at the evidence marker' }
        $selection = @(Find-NetRoute -RemoteIPAddress $Peer.ToString() -ErrorAction Stop)
        $source = @($selection | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_NetIPAddress' })
        $route = @($selection | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_NetRoute' })
        if ($source.Count -ne 1 -or $route.Count -ne 1 -or
            $source[0].InterfaceIndex -ne $route[0].InterfaceIndex) {
            throw 'Windows did not return an unambiguous source and route on one interface'
        }
        $record.interface_index = $route[0].InterfaceIndex
        $record.source_ip = $source[0].IPAddress
        $record.destination_prefix = $route[0].DestinationPrefix
        $record.next_hop = $route[0].NextHop
        $physical = @(Get-NetAdapter -Physical -ErrorAction Stop |
            Where-Object { $_.ifIndex -eq $record.interface_index })
        $record.physical_adapter = $physical.Count -gt 0
        $config = Get-NetIPConfiguration -InterfaceIndex $record.interface_index -ErrorAction Stop
        $gateways = if ($family -eq 'IPv4') { $config.IPv4DefaultGateway } else { $config.IPv6DefaultGateway }
        $record.default_gateways = @($gateways | ForEach-Object { $_.NextHop } | Where-Object { $_ })
        if ($Process.HasExited) { throw 'Client exited during route sampling' }
        $record.route_status = 'selected'
    } catch {
        $record.error = $_.Exception.Message
    }
    try {
        if ($Process.HasExited) { throw 'Client was not running at the socket sample' }
        $transport = Get-NetTCPConnection -ErrorAction Stop | Where-Object {
            $_.OwningProcess -eq $Process.Id -and $_.RemoteAddress -eq $Peer.ToString() -and
            $_.State -eq 'Established'
        } | ForEach-Object {
            @{ transport = 'tcp'; local_ip = $_.LocalAddress; local_port = $_.LocalPort;
               peer_ip = $_.RemoteAddress; peer_port = $_.RemotePort }
        }
        $datagram = Get-NetUDPEndpoint -ErrorAction Stop | Group-Object LocalPort |
            Where-Object {
                # A port also owned by another PID does not identify this Client.
                !($_.Group | Where-Object OwningProcess -ne $Process.Id)
            } | ForEach-Object Group | ForEach-Object {
                @{ transport = 'udp'; local_ip = $_.LocalAddress; local_port = $_.LocalPort }
            }
        $record.owned_sockets = @($transport | Where-Object { $_ }) +
            @($datagram | Where-Object { $_ })
    } catch {
        $record.socket_error = $_.Exception.Message
    }
    $record.client_running_at_end = !$Process.HasExited
    $record.completed_utc = [datetime]::UtcNow.ToString('o')
    $record
}

function Save-RoamingRouteMarker {
    param(
        [string]$Directory, [ValidateSet('before','after')][string]$Phase,
        [System.Net.IPAddress]$Peer, [Diagnostics.Process]$Process, [string]$StartedUtc
    )
    $output = Join-Path $Directory "route-$Phase.json"
    $signal = Join-Path $Directory "route-$Phase.request"
    if ((Test-Path -LiteralPath $output) -or !(Test-Path -LiteralPath $signal -PathType Leaf)) { return }
    $result = Get-RoamingRouteEvidence -Peer $Peer -Process $Process -StartedUtc $StartedUtc -Phase $Phase |
        ConvertTo-Json
    $staging = $output + '.pending'
    [IO.File]::WriteAllText($staging, $result)
    [IO.File]::Move($staging, $output)
}

$clientPath = (Get-Item -LiteralPath $Client).FullName
$profilePath = (Get-Item -LiteralPath $Profile).FullName
$beforePeer = $BeforePeerIp ?? $PeerIp
$afterPeer = $AfterPeerIp ?? $PeerIp
if ([bool]$beforePeer -ne [bool]$afterPeer) { throw 'Provide both phase peer IPs or one common PeerIp' }
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
        if ($beforePeer) {
            # Wait for route-before.json before switching, then signal route-after.request.
            # Route selection is not a packet observation; correlate with both endpoints or capture.
            Save-RoamingRouteMarker $out.FullName 'before' $beforePeer $p $start
            if (Test-Path -LiteralPath (Join-Path $out.FullName 'route-before.json')) {
                Save-RoamingRouteMarker $out.FullName 'after' $afterPeer $p $start
            }
        }
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
