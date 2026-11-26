[CmdletBinding()]
param(
    [string]$ProfilePath,
    [ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ProfileSha256,
    [Parameter(Mandatory)][ValidatePattern('^[a-fA-F0-9]{64}$')][string]$ReleaseManifestSha256,
    [ValidateSet('Install','Plan')][string]$Action = 'Install',
    [string]$PackageRoot = (Join-Path $PSScriptRoot '..\..\..')
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-ArtifactPin($Manifest, [string]$RelativePath) {
    $entry = @($Manifest.artifacts | Where-Object path -ceq $RelativePath)
    if ($entry.Count -ne 1 -or $entry[0].sha256 -cnotmatch '^[a-f0-9]{64}$') {
        throw "Client release manifest must identify exactly one artifact: $RelativePath"
    }
    return $entry[0].sha256
}

function Assert-ClientFile([string]$Base, [string]$RelativePath, [string]$Pin) {
    $file = Join-Path $Base $RelativePath
    if (!(Test-Path -LiteralPath $file -PathType Leaf) -or (Get-FileHash -LiteralPath $file).Hash -ne $Pin) {
        throw "Client release artifact differs from its pinned hash: $RelativePath"
    }
}

function Install-ClientFiles([string]$Source, [string]$Destination, $Files) {
    if (!(Test-Path -LiteralPath $Destination -PathType Container)) {
        $staging = $Destination + '.pending.' + [guid]::NewGuid().ToString('N')
        New-Item -ItemType Directory -Path $staging -Force | Out-Null
        try {
            foreach ($entry in $Files.GetEnumerator()) {
                $target = Join-Path $staging $entry.Key
                New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
                Copy-Item -LiteralPath (Join-Path $Source $entry.Key) -Destination $target
                Assert-ClientFile $staging $entry.Key $entry.Value
            }
            try { [IO.Directory]::Move($staging, $Destination) }
            catch [IO.IOException] {
                if (!(Test-Path -LiteralPath $Destination -PathType Container)) { throw }
            }
        } finally {
            if (Test-Path -LiteralPath $staging) {
                $resolved = Get-Item -LiteralPath $staging -Force
                if ($resolved.FullName -ne $staging -or
                    ($resolved.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                    throw 'Unexpected Client staging directory during cleanup.'
                }
                Remove-Item -LiteralPath $resolved.FullName -Recurse -Force
            }
        }
    }
    foreach ($entry in $Files.GetEnumerator()) {
        Assert-ClientFile $Destination $entry.Key $entry.Value
    }
}

$root = (Resolve-Path -LiteralPath $PackageRoot).ProviderPath
$manifestPath = Join-Path $root 'release-manifest.json'
$manifestBytes = [IO.File]::ReadAllBytes($manifestPath)
$sha = [Security.Cryptography.SHA256]::Create()
try { $manifestHash = [BitConverter]::ToString($sha.ComputeHash($manifestBytes)).Replace('-','') }
finally { $sha.Dispose() }
if ($manifestHash -ne $ReleaseManifestSha256) {
    throw 'Release manifest differs from its pinned hash.'
}
$manifest = [Text.Encoding]::UTF8.GetString($manifestBytes).TrimStart([char]0xfeff) | ConvertFrom-Json
if ($manifest.target -cne 'x86_64-pc-windows-msvc') { throw 'Windows x64 Client release required.' }
$entryPoint = 'bin/novovm-native-client.exe'
$files = @{ $entryPoint = (Get-ArtifactPin $manifest $entryPoint) }
Assert-ClientFile $root $entryPoint $files[$entryPoint]
if ([bool]$ProfilePath -ne [bool]$ProfileSha256) {
    throw 'Explicit discovery profile path and SHA-256 must be supplied together.'
}

$installedRelease = $null
$program = Join-Path $root $entryPoint
if ($ProfilePath) {
    $profile = (Resolve-Path -LiteralPath $ProfilePath).ProviderPath
    if ((Get-FileHash -LiteralPath $profile).Hash -ne $ProfileSha256) {
        throw 'Discovery profile differs from its pinned hash.'
    }
} else {
    $profileEntry = 'config/discovery/installed-discovery.json'
    $runtime = 'aoem/windows/core/bin/aoem_ffi.dll'
    $ProfileSha256 = Get-ArtifactPin $manifest $profileEntry
    Assert-ClientFile $root $profileEntry $ProfileSha256
    $document = Get-Content -Raw -LiteralPath (Join-Path $root $profileEntry) | ConvertFrom-Json
    $runtimePin = Get-ArtifactPin $manifest $runtime
    if ($document.library -cne '../../aoem/windows/core/bin/aoem_ffi.dll' -or
        $document.library_sha256 -cne $runtimePin) {
        throw 'Bundled discovery profile does not pin the packaged AOEM runtime.'
    }
    $bootstrap = @($document.seeds)
    if ($bootstrap.Count -ne 2 -or @($bootstrap | Where-Object {
        $locator = $_.PSObject.Properties['locator']
        $contact = $_.PSObject.Properties['contact']
        (!$locator -or !$locator.Value) -and (!$contact -or !$contact.Value)
    }).Count) {
        throw 'Bundled discovery profile requires two bootstrap sources with a locator or contact.'
    }
    $seeds = @($bootstrap | ForEach-Object {
        $locator = $_.PSObject.Properties['locator']
        if ($locator -and $locator.Value) { $locator.Value }
    })
    if (@($seeds | Sort-Object -Unique).Count -ne $seeds.Count -or
        @($seeds | Where-Object { $_ -notmatch '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}\.json$' -or
            $_ -ieq 'installed-discovery.json' }).Count) {
        throw 'Bundled discovery profile requires distinct portable locator filenames.'
    }
    $files[$profileEntry] = $ProfileSha256
    $files['release-manifest.json'] = $manifestHash
    $runtimeEntries = @($manifest.artifacts | Where-Object { $_.path -clike 'aoem/windows/core/bin/*' })
    foreach ($entry in $runtimeEntries) {
        if ($entry.path -cnotmatch '^aoem/windows/core/bin/[a-zA-Z0-9][a-zA-Z0-9._-]*$' -or
            $files.ContainsKey($entry.path)) { throw 'Ambiguous or invalid Client runtime manifest path.' }
        $files[$entry.path] = Get-ArtifactPin $manifest $entry.path
    }
    foreach ($seed in $seeds) {
        $locator = "config/discovery/$seed"
        $files[$locator] = Get-ArtifactPin $manifest $locator
    }
    foreach ($entry in $files.GetEnumerator()) {
        Assert-ClientFile $root $entry.Key $entry.Value
    }
    $local = [Environment]::GetFolderPath('LocalApplicationData')
    if (![IO.Path]::IsPathRooted($local)) { throw 'Current user LocalAppData directory required.' }
    $installedRelease = Join-Path $local ('NOVOVM/client-releases/' + $manifestHash.ToLowerInvariant())
    $profile = Join-Path $installedRelease $profileEntry
    $program = Join-Path $installedRelease $entryPoint
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$taskName = 'NOVOVM-Native-Client-' + $identity.User.Value
$arguments = '"' + $profile + '" follow-state ' + $ProfileSha256.ToLowerInvariant()
$directory = Split-Path -Parent $program
$plan = [ordered]@{ task = $taskName; startup = 'current-user-logon'; program = $program;
    arguments = $arguments; working_directory = $directory; principal_sid = $identity.User.Value;
    installed_release = $installedRelease }
if ($Action -eq 'Plan') { $plan | ConvertTo-Json; return }

$sourceProfile = if ($installedRelease) { Join-Path $root $profileEntry } else { $profile }
& (Join-Path $root $entryPoint) $sourceProfile check-installed-profile $ProfileSha256 | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'Native Client rejected the issued discovery profile before installation.'
}

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
}
if ($installedRelease) { Install-ClientFiles $root $installedRelease $files }
if (!$existing) {
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
