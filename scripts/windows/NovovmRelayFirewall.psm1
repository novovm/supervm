Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:RuleName = 'NOVOVM-Relay-WSS-TCP-443-v1'
$script:RuleGroup = 'NOVOVM Relay managed installer v1'

function Assert-NovovmRelayRuleOwner($Rule) {
    if ($Rule -and $Rule.Group -ne $script:RuleGroup) {
        throw 'The NOVOVM rule name is occupied by an unmanaged rule; no rule was changed.'
    }
}

function Get-NovovmRelayRule([string]$Store) {
    try {
        $rules = @(Get-NetFirewallRule -PolicyStore $Store -Name $script:RuleName -ErrorAction Stop)
    } catch {
        if ($_.CategoryInfo.Category -eq 'ObjectNotFound' -and
            $_.FullyQualifiedErrorId -like 'CmdletizationQuery_NotFound_*,Get-NetFirewallRule') {
            return $null
        }
        # Unreadable policy is not evidence that no rule exists.
        throw
    }
    if ($rules.Count -gt 1) { throw 'Ambiguous NOVOVM firewall rule.' }
    if ($rules.Count -eq 1) { return $rules[0] }
    return $null
}

function Test-NovovmRelayRule($Rule, [string]$ProgramPath) {
    if (-not $Rule) { return $false }
    Assert-NovovmRelayRuleOwner $Rule
    $app = $Rule | Get-NetFirewallApplicationFilter
    $port = $Rule | Get-NetFirewallPortFilter
    $address = $Rule | Get-NetFirewallAddressFilter
    return (
        [string]$Rule.Enabled -eq 'True' -and [string]$Rule.Direction -eq 'Inbound' -and
        [string]$Rule.Action -eq 'Allow' -and [string]$Rule.Profile -eq 'Any' -and
        [string]$Rule.EdgeTraversalPolicy -eq 'Block' -and
        [string]::Equals($app.Program, $ProgramPath, [StringComparison]::OrdinalIgnoreCase) -and
        [string]$port.Protocol -in @('TCP', '6') -and
        (@($port.LocalPort) -join ',') -eq '443' -and
        (@($port.RemotePort) -join ',') -eq 'Any' -and
        (@($address.LocalAddress) -join ',') -eq 'Any' -and
        (@($address.RemoteAddress) -join ',') -eq 'Any'
    )
}

function Get-NovovmRelayFirewallState {
    param([Parameter(Mandatory = $true)][string]$ProgramPath)
    $persistent = Get-NovovmRelayRule 'PersistentStore'
    $active = Get-NovovmRelayRule 'ActiveStore'
    [pscustomobject]@{
        ruleName = $script:RuleName
        program = $ProgramPath
        protocol = 'TCP'
        localPort = 443
        remoteAddress = 'Any'
        profiles = 'Any'
        persistentPresent = [bool]$persistent
        persistentMatches = (Test-NovovmRelayRule $persistent $ProgramPath)
        activeMatches = (Test-NovovmRelayRule $active $ProgramPath)
    }
}

function Set-NovovmRelayFirewall {
    param([Parameter(Mandatory = $true)][string]$ProgramPath)
    if (-not [IO.Path]::IsPathRooted($ProgramPath) -or
        [IO.Path]::GetFileName($ProgramPath) -ne 'supervm-relay.exe' -or
        -not (Test-Path -LiteralPath $ProgramPath -PathType Leaf)) {
        throw 'An installed, absolute supervm-relay.exe path is required.'
    }
    $existing = Get-NovovmRelayRule 'PersistentStore'
    Assert-NovovmRelayRuleOwner $existing
    $settings = @{
        PolicyStore = 'PersistentStore'; Name = $script:RuleName
        Enabled = 'True'; Direction = 'Inbound'; Action = 'Allow'; Profile = 'Any'
        Protocol = 'TCP'; LocalPort = 443; RemotePort = 'Any'
        LocalAddress = 'Any'; RemoteAddress = 'Any'; Program = $ProgramPath
        EdgeTraversalPolicy = 'Block'
    }
    if ($existing) {
        Set-NetFirewallRule @settings | Out-Null
    } else {
        New-NetFirewallRule @settings -DisplayName 'NOVOVM relay WSS (TCP 443)' -Group $script:RuleGroup -Description 'Managed node installation: inbound TCP 443 only for the installed NOVOVM relay.' | Out-Null
    }
    $state = Get-NovovmRelayFirewallState $ProgramPath
    if (-not $state.persistentMatches -or -not $state.activeMatches) {
        throw 'NOVOVM firewall configuration did not match the persistent and active policy. Check Windows policy; no global firewall settings were changed.'
    }
    $state
}

function Remove-NovovmRelayFirewall {
    $existing = Get-NovovmRelayRule 'PersistentStore'
    Assert-NovovmRelayRuleOwner $existing
    if ($existing) {
        Remove-NetFirewallRule -PolicyStore PersistentStore -Name $script:RuleName
    }
    if ((Get-NovovmRelayRule 'PersistentStore') -or (Get-NovovmRelayRule 'ActiveStore')) {
        throw 'NOVOVM firewall rule removal did not complete in persistent and active policy.'
    }
}

Export-ModuleMember -Function Get-NovovmRelayFirewallState, Set-NovovmRelayFirewall, Remove-NovovmRelayFirewall
