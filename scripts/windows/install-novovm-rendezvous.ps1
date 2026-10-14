<# Installs only the NOVOVM communications owner. No ledger/AOEM, router or NIC
   changes. Run Install/Uninstall elevated; Status is read-only. State and keys
   survive uninstall. Existing WSS relay installation/firewall are untouched. #>
[CmdletBinding()]
param(
    [ValidateSet('Install','Status','Uninstall')][string]$Action = 'Status',
    [string]$SourceBinary,
    [string]$ExpectedSha256,
    [string]$Configuration,
    [switch]$UserSession
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ($UserSession) {
    & (Join-Path $PSScriptRoot 'rendezvous-user-install.ps1') -Action $Action -SourceBinary $SourceBinary -ExpectedSha256 $ExpectedSha256 -Configuration $Configuration
    return
}
$serviceName = 'NOVOVMRendezvous'
$account = 'NT SERVICE\NOVOVMRendezvous'
$programRoot = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'NOVOVM\Rendezvous'
$dataRoot = Join-Path ([Environment]::GetFolderPath('CommonApplicationData')) 'NOVOVM\Rendezvous'
$stateRoot = Join-Path $dataRoot 'state'
$program = Join-Path $programRoot 'novovm-rendezvous-service.exe'
$configPath = Join-Path $dataRoot 'config.json'
$manifestPath = Join-Path $programRoot 'install.json'
$commandLine = '"' + $program + '" --service "' + $configPath + '"'
$ruleName = 'NOVOVM-Rendezvous-UDP-v1'
$ruleGroup = 'NOVOVM Rendezvous managed installer v1'

function Assert-PlainPath([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            if (((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw 'Managed paths must not contain reparse points.'
            }
        }
        $cursor = [IO.Path]::GetDirectoryName($cursor)
    }
}
function Set-PrivateDirectory([string]$Path, [string]$ServiceRights) {
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($sid in @('S-1-5-18','S-1-5-32-544')) {
        $id = New-Object Security.Principal.SecurityIdentifier($sid)
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($id,'FullControl','ContainerInherit,ObjectInherit','None','Allow')))
    }
    $acl.SetOwner((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')))
    if ($ServiceRights) {
        $id = (New-Object Security.Principal.NTAccount($account)).Translate([Security.Principal.SecurityIdentifier])
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($id,$ServiceRights,'ContainerInherit,ObjectInherit','None','Allow')))
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}
foreach ($path in @($programRoot,$dataRoot,$stateRoot,$configPath,$manifestPath,$program)) { Assert-PlainPath $path }
$service = Get-CimInstance Win32_Service -Filter "Name='$serviceName'"
$manifest = if (Test-Path -LiteralPath $manifestPath) { Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json } else { $null }
if ($manifest -and ($manifest.version -ne 1 -or $manifest.program -ne $program -or $manifest.dataRoot -ne $dataRoot)) {
    throw 'Unrecognized service installation ownership.'
}
if ($service -and (-not $manifest -or $service.PathName -ne $commandLine -or $service.StartName -ne $account)) {
    throw 'Existing service has different ownership/configuration; preserved.'
}
$rule = $null
try {
    $rules = @(Get-NetFirewallRule -PolicyStore PersistentStore -Name $ruleName -ErrorAction Stop)
    if ($rules.Count -ne 1) { throw 'Ambiguous service firewall rules.' }
    $rule = $rules[0]
} catch {
    if ($_.CategoryInfo.Category -ne 'ObjectNotFound' -or
        $_.FullyQualifiedErrorId -notlike 'CmdletizationQuery_NotFound_*,Get-NetFirewallRule') { throw }
}
if ($rule) {
    $app = $rule | Get-NetFirewallApplicationFilter
    $ports = $rule | Get-NetFirewallPortFilter
    if ($app.Program -ne $program -or $ports.Protocol -ne 'UDP' -or $rule.Direction -ne 'Inbound' -or
        $rule.Action -ne 'Allow' -or $rule.Group -ne $ruleGroup) {
        throw 'Existing firewall rule has different ownership; preserved.'
    }
}
if ($Action -eq 'Status') {
    [ordered]@{installed=[bool]$manifest; serviceState=$(if($service){$service.State}else{'Absent'});
        account=$(if($service){$service.StartName}else{$null}); program=$program;
        custodyPresent=(Test-Path -LiteralPath (Join-Path $stateRoot 'keys.dpapi'));
        firewallPresent=[bool]$rule} | ConvertTo-Json
    return
}
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'Windows administrator consent is required.' }
if ($Action -eq 'Uninstall') {
    if ($service) {
        Stop-Service -Name $serviceName
        & sc.exe delete $serviceName | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Service removal failed.' }
    }
    if ($rule) { Remove-NetFirewallRule -Name $ruleName }
    if ($manifest) {
        if (Test-Path -LiteralPath $program) { Remove-Item -LiteralPath $program }
        # Keep the ownership manifest for explicit reinstall and protected data.
    }
    Write-Output 'Service removed; keys, authorization history and ownership retained.'
    return
}
if ($ExpectedSha256 -notmatch '^[a-fA-F0-9]{64}$' -or -not (Test-Path -LiteralPath $SourceBinary -PathType Leaf)) {
    throw 'Source binary and independently checked SHA-256 are required.'
}
if ((Get-FileHash -LiteralPath $SourceBinary -Algorithm SHA256).Hash -ne $ExpectedSha256) { throw 'Source binary hash differs.' }
$config = Get-Content -LiteralPath $Configuration -Raw | ConvertFrom-Json
if ($config.version -ne 1 -or $config.epoch.engineering_direct_allowed -ne $true) { throw 'Explicit engineering configuration required.' }
$config.epoch.journal_path = Join-Path $stateRoot 'service.journal'
$config.epoch.source_path = Join-Path $stateRoot 'authority.json'
$encoded = $config | ConvertTo-Json -Depth 30
if ((Test-Path -LiteralPath $configPath) -and (Get-Content -LiteralPath $configPath -Raw).Trim() -ne $encoded.Trim()) {
    throw 'Existing trust/configuration differs; automatic policy replacement refused.'
}
if (-not $manifest -and ((Test-Path -LiteralPath $programRoot) -or (Test-Path -LiteralPath $dataRoot))) {
    throw 'Existing unmanaged directories are preserved.'
}
if ($service -and $service.State -ne 'Stopped') { Stop-Service -Name $serviceName }
foreach ($path in @($programRoot,$dataRoot,$stateRoot)) {
    New-Item -ItemType Directory -Path $path -Force | Out-Null
    Set-PrivateDirectory $path ''
}
$staged = Join-Path $programRoot 'install.pending'
Copy-Item -LiteralPath $SourceBinary -Destination $staged -Force
if ((Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash -ne $ExpectedSha256) { throw 'Staged binary hash differs.' }
Move-Item -LiteralPath $staged -Destination $program -Force
[IO.File]::WriteAllText($configPath,$encoded,(New-Object Text.UTF8Encoding($false)))
& $program --check $configPath
if ($LASTEXITCODE -ne 0) { throw 'Service configuration validation failed; service was not started.' }
[ordered]@{version=1;program=$program;dataRoot=$dataRoot;sha256=$ExpectedSha256.ToLowerInvariant()} |
    ConvertTo-Json | Set-Content -LiteralPath $manifestPath -Encoding UTF8
if (-not $service) {
    & sc.exe create $serviceName binPath= $commandLine start= auto obj= $account | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Virtual-account service creation failed.' }
}
Set-PrivateDirectory $programRoot 'ReadAndExecute'
Set-PrivateDirectory $dataRoot 'ReadAndExecute'
Set-PrivateDirectory $stateRoot 'Modify'
$marker = Join-Path $stateRoot 'enrollment.started'
$request = Join-Path $stateRoot 'initialize.request'
if (-not (Test-Path -LiteralPath $marker)) {
    # Request only on an empty new install, never repair lost keys/history.
    if (@(Get-ChildItem -LiteralPath $stateRoot -Force | Where-Object Name -ne 'initialize.request').Count -ne 0) {
        throw 'Partial owner enrollment requires explicit recovery.'
    }
    [IO.File]::WriteAllText($request,'Explicit installer enrollment',(New-Object Text.UTF8Encoding($false)))
}
& $program --configure-recovery
if ($LASTEXITCODE -ne 0) { throw 'Bounded crash recovery configuration failed.' }
if (-not $rule) {
    New-NetFirewallRule -PolicyStore PersistentStore -Name $ruleName -Group $ruleGroup -DisplayName 'NOVOVM Rendezvous communications' -Direction Inbound -Action Allow -Protocol UDP -Program $program -Profile Any | Out-Null
}
Start-Service -Name $serviceName
Write-Output 'Communications owner started. SCM Running does not imply publisher authorization, public reachability or anonymity.'
