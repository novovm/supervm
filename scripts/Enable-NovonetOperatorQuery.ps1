#Requires -Version 5.1
# A-only diagnostic client reply rule. Does not initialize, stop or start nodes.
param(
    [string]$PackageRoot='D:\WorksArea\SUPERVM\artifacts\product-overlay\novonet-08236129-windows-x86_64',
    [string]$ReportDirectory='D:\WorksArea\SUPERVM\target\novonet-four-host-live-20261108\operator-firewall'
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
$program=(Resolve-Path -LiteralPath (Join-Path $PackageRoot 'bin\novovm-native-client.exe')).Path
if((Get-FileHash -LiteralPath $program -Algorithm SHA256).Hash.ToLowerInvariant() -cne '597b4de7a369bc6e5fcaf8114694ed1ef91f6248ca61431c866b070f9a5c4ca1'){
    throw 'Frozen original client hash mismatch'
}
if(-not (Get-NetIPAddress -AddressFamily IPv4 -IPAddress '192.168.71.74' -ErrorAction SilentlyContinue)){
    throw 'A LAN address changed; inspect instead of widening the rule'
}
$blockedName='UDP Query User{8CBB3BE5-FAF7-48FA-B066-F1C1AEF56EA7}D:\worksarea\supervm\artifacts\product-overlay\novonet-08236129-windows-x86_64\bin\novovm-native-client.exe'
$allowName='NOVOVM-08236129-A-OperatorQuery-41190-20261108'
if((Test-Path -LiteralPath $ReportDirectory) -or (Get-NetFirewallRule -Name $allowName -ErrorAction SilentlyContinue)){
    throw 'Existing diagnostic result/rule: inspect it; do not repeat or overwrite'
}
$blocked=Get-NetFirewallRule -Name $blockedName
$application=$blocked | Get-NetFirewallApplicationFilter
$port=$blocked | Get-NetFirewallPortFilter
if($blocked.Direction.ToString() -cne 'Inbound' -or
   $blocked.Enabled.ToString() -cne 'True' -or $blocked.Profile.ToString() -cne 'Public' -or
   $port.Protocol.ToString() -notin @('UDP','17') -or
   -not [string]::Equals($application.Program,$program,[StringComparison]::OrdinalIgnoreCase)){
    throw ('Original UDP rule shape changed; no changes made. Observed action='+$blocked.Action.ToString()+', enabled='+$blocked.Enabled.ToString()+', profile='+$blocked.Profile.ToString()+', protocol='+$port.Protocol.ToString())
}
if($blocked.Action.ToString() -ceq 'Allow'){
    Write-Output 'A_OPERATOR_EXISTING_UDP_ALLOW. Existing rule already permits this client; no firewall changes made. Continue original read-only queries; this is not network acceptance.'
    exit 0
}
if($blocked.Action.ToString() -cne 'Block'){
    throw ('Unexpected original UDP action: '+$blocked.Action.ToString()+'; no changes made')
}
if(-not ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){
    throw 'Run once in Administrator PowerShell. No changes made.'
}
New-Item -ItemType Directory -Path $ReportDirectory | Out-Null
$before=[ordered]@{scope='A_original_native_operator_reply_only';observed_utc=[DateTime]::UtcNow.ToString('o');program=$program;disabled_rule=$blockedName;original_enabled=$true;original_action='Block';local_address='192.168.71.74';local_udp_port=41190;remote_addresses=@('47.103.59.206','192.168.71.125','192.168.71.59');remote_udp_ports=@(41050,41052,41053);node_or_state_changed=$false}
$before | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'before.json') -Encoding UTF8
Disable-NetFirewallRule -Name $blockedName | Out-Null
try{
    New-NetFirewallRule -Name $allowName -DisplayName $allowName -Enabled True -Direction Inbound -Action Allow -Profile Public -Protocol UDP -Program $program -LocalAddress '192.168.71.74' -LocalPort 41190 -RemoteAddress $before.remote_addresses -RemotePort $before.remote_udp_ports | Out-Null
}catch{
    Enable-NetFirewallRule -Name $blockedName | Out-Null
    throw
}
$after=Get-NetFirewallRule -Name $allowName
if($after.Enabled.ToString() -cne 'True' -or $after.Action.ToString() -cne 'Allow'){
    throw 'Rule verification failed; preserve before.json for exact rollback'
}
[ordered]@{scope=$before.scope;completed_utc=[DateTime]::UtcNow.ToString('o');disabled_rule=$blockedName;allow_rule=$allowName;fixed_client_source_port=41190;nodes_restarted=$false;tcp_rule_changed=$false;global_firewall_changed=$false} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'result.json') -Encoding UTF8
Write-Output 'A_OPERATOR_REPLY_RULE_READY. Use only the original client bound to 192.168.71.74:41190. Network query success remains unverified.'
