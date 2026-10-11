# Explicit fixtures only. No adapter changes or network traffic.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$module = Import-Module (Join-Path $PSScriptRoot '..\windows\NovovmNodeNetwork.psm1') -Force -PassThru
$script:checks = 0
function Check($Actual, $Expected) {
    if ($Actual -cne $Expected) { throw "Expected $Expected, got $Actual" }
    $script:checks++
}
try {
    foreach ($case in @(
        @('10.1.2.3','private'), @('172.16.0.1','private'), @('172.31.255.255','private'),
        @('172.32.0.1','otherIpv4Unverified'), @('192.168.1.1','private'),
        @('100.63.255.255','otherIpv4Unverified'), @('100.64.0.0','shared'),
        @('100.127.255.255','shared'), @('100.128.0.0','otherIpv4Unverified'),
        @('169.254.1.2','linkLocal'), @('0.0.0.0','unspecified'), @('127.0.0.1','loopback'),
        @('::1','loopback'), @('::','unspecified'), @('fe80::1%17','linkLocal'),
        @('fd12::1','uniqueLocal'), @('::ffff:192.168.1.1','specialIpv6'),
        @('ff02::1','multicast'), @('2001:db8::1','specialIpv6'), @('3fff:fff::1','specialIpv6'),
        @('2001:2::1','specialIpv6'), @('2002::1','specialIpv6'),
        @('240e:1234::1','globalUnicastCandidate')
    )) { Check (Get-NovovmAddressScope $case[0]) $case[1] }
    $snapshot = [pscustomobject]@{
        interfaceAlias='fixture';interfaceIndex=17;status='Up';ipv6Enabled=$false
        addresses=@([pscustomobject]@{address='192.168.1.28';state='Preferred';skipAsSource=$false});routes=@()
    }
    Check (ConvertTo-NovovmNodeNetworkReport $snapshot).diagnosis 'ipv6BindingDisabled'
    $snapshot.routes = @([pscustomobject]@{destination='0.0.0.0/0';state='Alive'})
    $weakNode = ConvertTo-NovovmNodeNetworkReport $snapshot
    Check $weakNode.inboundRequiredForParticipation $false
    Check $weakNode.participation 'outboundCandidateRequiresAuthenticatedPath'
    Check $weakNode.externalReachability 'notTested'
    $snapshot.ipv6Enabled = $true
    Check (ConvertTo-NovovmNodeNetworkReport $snapshot).diagnosis 'noPreferredGlobalIpv6Candidate'
    $snapshot.addresses += [pscustomobject]@{address='240e:1234::1';state='Tentative';skipAsSource=$false}
    Check (ConvertTo-NovovmNodeNetworkReport $snapshot).diagnosis 'noPreferredGlobalIpv6Candidate'
    $snapshot.addresses[1].state = 'Preferred'
    Check (ConvertTo-NovovmNodeNetworkReport $snapshot).diagnosis 'noIpv6DefaultRoute'
    $snapshot.routes = @([pscustomobject]@{destination='::/0';state='Dead'})
    Check (ConvertTo-NovovmNodeNetworkReport $snapshot).diagnosis 'noIpv6DefaultRoute'
    $snapshot.routes[0].state = 'Alive'
    $report = ConvertTo-NovovmNodeNetworkReport $snapshot
    Check $report.diagnosis 'ipv6CandidateRequiresExternalProbe'
    Check $report.ipv6Candidates.Count 1
    Check $report.externalReachability 'notTested'
    Check $report.natType 'notMeasured'
    Check $report.routerWanAddress 'notObserved'
    $snapshot.addresses[1].skipAsSource = $true
    Check (ConvertTo-NovovmNodeNetworkReport $snapshot).ipv6Candidates.Count 0
    $snapshot.status = 'Disconnected'
    Check (ConvertTo-NovovmNodeNetworkReport $snapshot).diagnosis 'adapterNotUp'
    # The adapter name cannot silently expand into a sweep of all interfaces.
    $rejected = $false
    try { Get-NovovmNodeNetwork '*' } catch { $rejected = $_.Exception.Message -eq 'An exact interface alias is required.' }
    Check $rejected $true
    Write-Output "$script:checks network preflight assertions passed (fixtures only)."
} finally { Remove-Module $module -Force }
