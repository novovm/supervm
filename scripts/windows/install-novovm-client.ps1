[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ProfilePath,
    [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ProfileSha256,
    [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ReleaseManifestSha256,
    [ValidateSet('Install','Plan')][string]$Action = 'Install',
    [string]$PackageRoot = (Join-Path $PSScriptRoot '..\..\..')
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path -LiteralPath $PackageRoot).ProviderPath
$profile = (Resolve-Path -LiteralPath $ProfilePath).ProviderPath
$manifestPath = Join-Path $root 'release-manifest.json'
$program = Join-Path $root 'bin\novovm-native-client.exe'
$manifestBytes = [IO.File]::ReadAllBytes($manifestPath)
$sha = [Security.Cryptography.SHA256]::Create()
try { $manifestHash = [BitConverter]::ToString($sha.ComputeHash($manifestBytes)).Replace('-','') }
finally { $sha.Dispose() }
if ($manifestHash -ne $ReleaseManifestSha256 -or
    (Get-FileHash -LiteralPath $profile).Hash -ne $ProfileSha256) {
    throw 'Release manifest or discovery profile differs from its pinned hash.'
}
$manifest = [Text.Encoding]::UTF8.GetString($manifestBytes).TrimStart([char]0xfeff) | ConvertFrom-Json
$artifact = @($manifest.artifacts | Where-Object path -ceq 'bin/novovm-native-client.exe')
if ($manifest.target -cne 'x86_64-pc-windows-msvc' -or $artifact.Count -ne 1 -or
    (Get-FileHash -LiteralPath $program).Hash -ne $artifact[0].sha256) {
    throw 'Native Client differs from the pinned Windows release.'
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$taskName = 'NOVOVM-Native-Client-' + $identity.User.Value
$arguments = '"' + $profile + '" follow-state ' + $ProfileSha256.ToLowerInvariant()
$directory = Split-Path -Parent $program
$plan = [ordered]@{ task = $taskName; startup = 'current-user-logon'; program = $program;
    arguments = $arguments; working_directory = $directory; principal_sid = $identity.User.Value }
if ($Action -eq 'Plan') { $plan | ConvertTo-Json; return }

$existing = Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue
if ($existing) {
    $user = $existing.Principal.UserId
    $sid = if ($user -match '^S-1-') { $user } else {
        [Security.Principal.NTAccount]::new($user).Translate([Security.Principal.SecurityIdentifier]).Value
    }
    if ($sid -ne $identity.User.Value -or [int]$existing.Principal.LogonType -ne 3 -or
        [int]$existing.Principal.RunLevel -ne 0 -or @($existing.Actions).Count -ne 1 -or
        $existing.Actions[0].Execute -ne $program -or $existing.Actions[0].Arguments -cne $arguments -or
        $existing.Actions[0].WorkingDirectory -ne $directory -or @($existing.Triggers).Count -ne 1 -or
        $existing.Triggers[0].CimClass.CimClassName -ne 'MSFT_TaskLogonTrigger') {
        throw 'Existing task uses different startup settings. Preserve and review it before replacing it.'
    }
} else {
    $nativeAction = New-ScheduledTaskAction -Execute $program -Argument $arguments -WorkingDirectory $directory
    $principal = New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Limited
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity.Name
    $settings = New-ScheduledTaskSettingsSet -Hidden -StartWhenAvailable -AllowStartIfOnBatteries `
        -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew `
        -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
    Register-ScheduledTask -TaskName $taskName -TaskPath '\' -Action $nativeAction -Trigger $trigger `
        -Principal $principal -Settings $settings -Description 'NOVOVM installed-client verified state synchronization' | Out-Null
}
if (-not $existing -or $existing.State -ne 'Running') { Start-ScheduledTask -TaskName $taskName -TaskPath '\' }
$plan | ConvertTo-Json
