# Isolated policy fixtures. No real firewall command, installation or elevation.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$modulePath = Join-Path $PSScriptRoot '..\windows\NovovmRelayFirewall.psm1'
$module = Import-Module $modulePath -Force -PassThru
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('novovm-firewall-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null
$program = Join-Path $testRoot 'supervm-relay.exe'
[IO.File]::WriteAllText($program, 'policy fixture only; never executed')
$script:checks = 0
function Check([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
    $script:checks++
}
function MustReject([scriptblock]$Operation, [string]$Pattern) {
    try { & $Operation | Out-Null } catch {
        Check ($_.Exception.Message -like $Pattern) ('Wrong rejection: ' + $_.Exception.Message)
        return
    }
    throw 'Expected rejection was not raised.'
}
try {
    & $module {
        $script:fixture = $null
        $script:creates = 0
        $script:updates = 0
        $script:deletes = 0
        $script:denyActive = $false
        $script:readFailure = $false
        function script:Get-NetFirewallRule {
            param($PolicyStore, $Name, $ErrorAction)
            if ($script:readFailure) { throw 'Policy read failed' }
            if ($PolicyStore -eq 'ActiveStore' -and $script:denyActive) { return }
            if ($script:fixture -and $script:fixture.Name -eq $Name) { $script:fixture }
        }
        function script:New-NetFirewallRule {
            param($PolicyStore, $Name, $Enabled, $Direction, $Action, $Profile, $Protocol,
                $LocalPort, $RemotePort, $LocalAddress, $RemoteAddress, $Program,
                $EdgeTraversalPolicy, $DisplayName, $Group, $Description)
            $script:creates++
            $script:fixture = [pscustomobject]@{} | Select-Object *
            $copy = @{}
            foreach ($key in $PSBoundParameters.Keys) { $copy[$key] = $PSBoundParameters[$key] }
            $script:fixture = [pscustomobject]$copy
        }
        function script:Set-NetFirewallRule {
            param($PolicyStore, $Name, $Enabled, $Direction, $Action, $Profile, $Protocol,
                $LocalPort, $RemotePort, $LocalAddress, $RemoteAddress, $Program, $EdgeTraversalPolicy)
            $script:updates++
            foreach ($key in $PSBoundParameters.Keys) { $script:fixture.$key = $PSBoundParameters[$key] }
        }
        function script:Remove-NetFirewallRule {
            param($PolicyStore, $Name)
            $script:deletes++
            if ($script:fixture.Name -ne $Name) { throw 'Attempt to remove unrelated rule' }
            $script:fixture = $null
        }
        function script:Get-NetFirewallApplicationFilter {
            param([Parameter(ValueFromPipeline = $true)]$Rule)
            process { [pscustomobject]@{Program = $Rule.Program} }
        }
        function script:Get-NetFirewallPortFilter {
            param([Parameter(ValueFromPipeline = $true)]$Rule)
            process { [pscustomobject]@{Protocol = $Rule.Protocol; LocalPort = $Rule.LocalPort; RemotePort = $Rule.RemotePort} }
        }
        function script:Get-NetFirewallAddressFilter {
            param([Parameter(ValueFromPipeline = $true)]$Rule)
            process { [pscustomobject]@{LocalAddress = $Rule.LocalAddress; RemoteAddress = $Rule.RemoteAddress} }
        }
    }
    $before = Get-NovovmRelayFirewallState $program
    Check (-not $before.persistentPresent -and -not $before.activeMatches) 'Missing rule must not pass'
    & $module { $script:readFailure = $true }
    MustReject { Get-NovovmRelayFirewallState $program } '*Policy read failed*'
    MustReject { Set-NovovmRelayFirewall $program } '*Policy read failed*'
    MustReject { Remove-NovovmRelayFirewall } '*Policy read failed*'
    Check ((& $module { $script:creates + $script:updates + $script:deletes }) -eq 0) 'Unreadable policy caused mutation'
    & $module { $script:readFailure = $false }
    $installed = Set-NovovmRelayFirewall $program
    Check ($installed.persistentMatches -and $installed.activeMatches) 'Exact installed policy must pass'
    Check ((& $module { $script:fixture.Program }) -eq $program) 'Rule lost program binding'
    Check ((& $module { $script:fixture.LocalPort }) -eq 443) 'Rule broadened port'
    Check ((& $module { $script:fixture.Protocol }) -eq 'TCP') 'Rule broadened protocol'
    $null = Set-NovovmRelayFirewall $program
    Check ((& $module { $script:creates }) -eq 1 -and (& $module { $script:updates }) -eq 1) 'Reinstall created a duplicate'
    & $module { $script:fixture.LocalPort = 'Any' }
    Check (-not (Get-NovovmRelayFirewallState $program).persistentMatches) 'Broad port rule was accepted'
    $null = Set-NovovmRelayFirewall $program
    & $module { $script:denyActive = $true }
    MustReject { Set-NovovmRelayFirewall $program } '*active policy*'
    & $module { $script:denyActive = $false; $script:fixture.Group = 'Unrelated vendor' }
    $updates = & $module { $script:updates }
    MustReject { Set-NovovmRelayFirewall $program } '*unmanaged rule*'
    Check ((& $module { $script:updates }) -eq $updates) 'Unmanaged rule was changed'
    MustReject { Remove-NovovmRelayFirewall } '*unmanaged rule*'
    Check ((& $module { $script:deletes }) -eq 0) 'Unmanaged rule was removed'
    & $module { $script:fixture.Group = $script:RuleGroup }
    Remove-NovovmRelayFirewall
    Remove-NovovmRelayFirewall
    Check ((& $module { $script:deletes }) -eq 1) 'Uninstall is not idempotent'
    Check (-not (Get-NovovmRelayFirewallState $program).persistentPresent) 'Uninstall left owned rule'
    MustReject { Set-NovovmRelayFirewall (Join-Path $testRoot 'missing.exe') } '*installed, absolute*'
    # Execute ONLY the original configuration guards, never installer OS calls.
    foreach ($installer in @('rendezvous-user-install.ps1', 'install-novovm-rendezvous.ps1')) {
        $sourcePath = Join-Path (Join-Path $PSScriptRoot '../windows') $installer
        $tokens = $null; $parseErrors = $null
        $sourceAst = [System.Management.Automation.Language.Parser]::ParseFile(
            (Resolve-Path $sourcePath).Path, [ref]$tokens, [ref]$parseErrors)
        if ($parseErrors.Count) { throw $parseErrors }
        $guards = @($sourceAst.FindAll({ param($node)
            $node -is [System.Management.Automation.Language.IfStatementAst] -and
            ($node.Extent.Text.Contains('Existing protected ML-DSA-65 self-owned v3 configuration required') -or
             $node.Extent.Text.Contains('Explicit protected ML-DSA-65 epoch owner v2/daemon v3 configuration required') -or
             $node.Extent.Text.Contains('Explicit chain, qualified library pin and absolute protected transport identity are required.'))
        }, $true) | ForEach-Object { [scriptblock]::Create($_.Extent.Text) })
        Check ($guards.Count -eq 2) 'Original installer must retain version and qualified-purpose configuration guards'
        $selfOwned = $installer -eq 'rendezvous-user-install.ps1'
        $config = if ($selfOwned) {
            [pscustomobject]@{ version=3;root=$testRoot;chain_id=47;library_sha256=('a'*64);
                library=(Join-Path $testRoot 'aoem_ffi.dll');transport_identity_path=(Join-Path $testRoot 'transport.sealed') }
        } else {
            [pscustomobject]@{version=2;epoch=[pscustomobject]@{version=3;engineering_direct_allowed=$true;
                chain_id=47;library_sha256=('a'*64);library=(Join-Path $testRoot 'aoem_ffi.dll');
                relay_identity_key_path=(Join-Path $testRoot 'transport.sealed') }}
        }
        foreach ($guard in $guards) { & $guard }
        $script:checks++
        $valid = $config | ConvertTo-Json -Depth 4
        foreach ($failure in @('old-version','zero-chain','missing-pin','relative-library','relative-key')) {
            $config = $valid | ConvertFrom-Json
            $identity = if ($selfOwned) { $config } else { $config.epoch }
            switch ($failure) {
                'old-version' { $config.version = if ($selfOwned) { 2 } else { 1 } }
                'zero-chain' { $identity.chain_id = 0 }
                'missing-pin' { $identity.library_sha256 = '' }
                'relative-library' { $identity.library = 'aoem_ffi.dll' }
                'relative-key' {
                    if ($selfOwned) { $identity.transport_identity_path = 'transport.sealed' }
                    else { $identity.relay_identity_key_path = 'transport.sealed' }
                }
            }
            MustReject { foreach ($guard in $guards) { & $guard } } '*required*'
        }
        Check ($sourceAst.Extent.Text.Contains('--check')) 'Original executable must validate its current typed configuration before installation'
    }
    Write-Output "$script:checks policy assertions passed (fixtures only, no OS mutation)."
} finally {
    Remove-Module $module -Force
    Remove-Item -LiteralPath $program
    Remove-Item -LiteralPath $testRoot
}
exit 0
