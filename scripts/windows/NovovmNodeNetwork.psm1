Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-NovovmAddressScope {
    param([Parameter(Mandatory = $true)][string]$Address)
    $ip = $null
    if (-not [Net.IPAddress]::TryParse($Address, [ref]$ip)) { throw 'Invalid local IP address.' }
    $b = $ip.GetAddressBytes()
    if ([Net.IPAddress]::IsLoopback($ip)) { return 'loopback' }
    if ($ip.Equals([Net.IPAddress]::Any) -or $ip.Equals([Net.IPAddress]::IPv6Any)) { return 'unspecified' }
    if ($b.Length -eq 4) {
        if ($b[0] -eq 10 -or ($b[0] -eq 172 -and $b[1] -ge 16 -and $b[1] -le 31) -or
            ($b[0] -eq 192 -and $b[1] -eq 168)) { return 'private' }
        if ($b[0] -eq 100 -and $b[1] -ge 64 -and $b[1] -le 127) { return 'shared' }
        if ($b[0] -eq 169 -and $b[1] -eq 254) { return 'linkLocal' }
        if ($b[0] -ge 224 -and $b[0] -le 239) { return 'multicast' }
        # Do not turn a partial special-address classifier into a public-IP test.
        return 'otherIpv4Unverified'
    }
    if ($ip.IsIPv6LinkLocal) { return 'linkLocal' }
    if ($ip.IsIPv6Multicast) { return 'multicast' }
    if (($b[0] -band 0xfe) -eq 0xfc) { return 'uniqueLocal' }
    if ($ip.IsIPv4MappedToIPv6 -or $ip.IsIPv6SiteLocal) { return 'specialIpv6' }
    # Conservative candidates, excluding special-purpose ranges within 2000::/3.
    # Not a replacement for the IANA registry or an external connection test.
    if (($b[0] -band 0xe0) -eq 0x20) {
        if (($b[0] -eq 0x20 -and $b[1] -eq 1 -and $b[2] -le 1) -or
            ($b[0] -eq 0x20 -and $b[1] -eq 1 -and $b[2] -eq 0x0d -and $b[3] -eq 0xb8) -or
            ($b[0] -eq 0x20 -and $b[1] -eq 2) -or
            ($b[0] -eq 0x26 -and $b[1] -eq 0x20 -and $b[2] -eq 0 -and $b[3] -eq 0x4f -and $b[4] -eq 0x80 -and $b[5] -eq 0) -or
            ($b[0] -eq 0x3f -and $b[1] -eq 0xff -and ($b[2] -band 0xf0) -eq 0)) { return 'specialIpv6' }
        return 'globalUnicastCandidate'
    }
    return 'specialIpv6'
}

function ConvertTo-NovovmNodeNetworkReport {
    param([Parameter(Mandatory = $true)]$Snapshot)
    $addresses = @($Snapshot.addresses | ForEach-Object {
        [pscustomobject]@{address = $_.address; scope = (Get-NovovmAddressScope $_.address);
            state = $_.state; skipAsSource = [bool]$_.skipAsSource}
    })
    $v6 = @($addresses | Where-Object {
        $_.scope -eq 'globalUnicastCandidate' -and $_.state -eq 'Preferred' -and -not $_.skipAsSource
    })
    $v6Routes = @($Snapshot.routes | Where-Object { $_.destination -eq '::/0' -and $_.state -eq 'Alive' })
    $v4Routes = @($Snapshot.routes | Where-Object { $_.destination -eq '0.0.0.0/0' -and $_.state -eq 'Alive' })
    $status = if ($Snapshot.status -ne 'Up') { 'adapterNotUp' }
        elseif (-not $Snapshot.ipv6Enabled) { 'ipv6BindingDisabled' }
        elseif ($v6.Count -eq 0) { 'noPreferredGlobalIpv6Candidate' }
        elseif ($v6Routes.Count -eq 0) { 'noIpv6DefaultRoute' }
        else { 'ipv6CandidateRequiresExternalProbe' }
    [pscustomobject]@{
        version = 1; interfaceAlias = $Snapshot.interfaceAlias; interfaceIndex = $Snapshot.interfaceIndex
        status = $Snapshot.status; ipv6Enabled = [bool]$Snapshot.ipv6Enabled
        ipv4DefaultRoute = ($v4Routes.Count -gt 0); ipv6DefaultRoute = ($v6Routes.Count -gt 0)
        diagnosis = $status; addresses = $addresses
        ipv6Candidates = @(if ($status -eq 'ipv6CandidateRequiresExternalProbe') { $v6.address })
        externalReachability = 'notTested'; natType = 'notMeasured'; routerWanAddress = 'notObserved'
        # A local private IPv4 cannot identify the gateway WAN or prove CGNAT.
        inboundRequiredForParticipation = $false
        participation = if ($Snapshot.status -eq 'Up' -and ($v4Routes.Count -gt 0 -or $v6Routes.Count -gt 0)) {
            'outboundCandidateRequiresAuthenticatedPath'
        } else { 'noDefaultRouteObserved' }
        # This is never a deny gate for the runtime. Route/NAT measurements
        # cannot grant or withdraw identity, contact or privacy authorization.
        readOnly = $true
    }
}

function Get-NovovmNodeNetwork {
    param([Parameter(Mandatory = $true)][ValidateLength(1, 128)][string]$InterfaceAlias)
    if ([System.Management.Automation.WildcardPattern]::ContainsWildcardCharacters($InterfaceAlias)) {
        throw 'An exact interface alias is required.'
    }
    $adapters = @(Get-NetAdapter -Name $InterfaceAlias -ErrorAction Stop)
    if ($adapters.Count -ne 1) { throw 'Expected exactly one adapter.' }
    $adapter = $adapters[0]
    $binding = @(Get-NetAdapterBinding -Name $InterfaceAlias -ComponentID ms_tcpip6 -ErrorAction Stop)
    if ($binding.Count -ne 1) { throw 'IPv6 binding could not be read unambiguously.' }
    try { $ips = @(Get-NetIPAddress -InterfaceIndex $adapter.ifIndex -ErrorAction Stop) }
    catch {
        if ($_.CategoryInfo.Category -eq 'ObjectNotFound' -and $_.FullyQualifiedErrorId -like 'CmdletizationQuery_NotFound_*') { $ips = @() }
        else { throw }
    }
    try { $routes = @(Get-NetRoute -InterfaceIndex $adapter.ifIndex -ErrorAction Stop) }
    catch {
        if ($_.CategoryInfo.Category -eq 'ObjectNotFound' -and $_.FullyQualifiedErrorId -like 'CmdletizationQuery_NotFound_*') { $routes = @() }
        else { throw }
    }
    $after = Get-NetAdapter -Name $InterfaceAlias -ErrorAction Stop
    $bindingAfter = Get-NetAdapterBinding -Name $InterfaceAlias -ComponentID ms_tcpip6 -ErrorAction Stop
    if ($after.InterfaceGuid -ne $adapter.InterfaceGuid -or $after.ifIndex -ne $adapter.ifIndex -or
        $after.Status -ne $adapter.Status -or $bindingAfter.Enabled -ne $binding[0].Enabled) {
        throw 'Adapter changed during collection; run a new snapshot.'
    }
    $snapshot = [pscustomobject]@{
        interfaceAlias = $adapter.Name; interfaceIndex = $adapter.ifIndex; status = [string]$adapter.Status
        ipv6Enabled = [bool]$binding[0].Enabled
        addresses = @($ips | ForEach-Object { [pscustomobject]@{address = $_.IPAddress; state = [string]$_.AddressState; skipAsSource = $_.SkipAsSource} })
        routes = @($routes | ForEach-Object { [pscustomobject]@{destination = $_.DestinationPrefix; state = [string]$_.State} })
    }
    ConvertTo-NovovmNodeNetworkReport $snapshot
}

Export-ModuleMember -Function Get-NovovmNodeNetwork, ConvertTo-NovovmNodeNetworkReport, Get-NovovmAddressScope
