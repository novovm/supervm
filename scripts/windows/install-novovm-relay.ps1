<# Windows relay package install/upgrade entry. Firewall setup is part of Install,
   not a runtime fallback. Run Install/Uninstall with Windows administrator consent.
   The existing duplex-v2 relay remains the only network runtime. #>
[CmdletBinding()]
param(
    [ValidateSet('Install', 'Status', 'Uninstall')][string]$Action = 'Install',
    [string]$SourceBinary,
    [string]$ExpectedSha256,
    [string]$ReportPath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'NovovmRelayFirewall.psm1') -Force

function Assert-NoReparsePath([string]$Path) {
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw 'Reparse points are not supported in the managed installation path.'
            }
        }
        $parent = [IO.Path]::GetDirectoryName($cursor)
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
}

function Write-InstallReport($Value) {
    $json = $Value | ConvertTo-Json -Depth 6
    if ($ReportPath) {
        if (-not [IO.Path]::IsPathRooted($ReportPath)) { throw 'ReportPath must be absolute.' }
        [IO.File]::WriteAllText($ReportPath, $json, (New-Object Text.UTF8Encoding($false)))
    }
    Write-Output $json
}

try {
    if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'Windows is required.' }
    $programFiles = [Environment]::GetFolderPath('ProgramFiles')
    $installRoot = Join-Path $programFiles 'NOVOVM\Relay'
    $program = Join-Path $installRoot 'supervm-relay.exe'
    $manifestPath = Join-Path $installRoot 'install.json'
    Assert-NoReparsePath $installRoot
    $manifest = $null
    if (Test-Path -LiteralPath $manifestPath) {
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
        if ($manifest.version -ne 1 -or $manifest.product -ne 'NOVOVM Relay' -or
            $manifest.program -ne $program) { throw 'Unrecognized installation manifest.' }
    } elseif ((Test-Path -LiteralPath $program) -or (Test-Path -LiteralPath $installRoot)) {
        throw 'Existing unmanaged installation directory is preserved; resolve ownership first.'
    }
    if ($Action -eq 'Status') {
        $state = Get-NovovmRelayFirewallState $program
        $binaryPresent = Test-Path -LiteralPath $program -PathType Leaf
        $hash = if ($binaryPresent) { (Get-FileHash -LiteralPath $program -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
        Write-InstallReport ([ordered]@{ok = $true; action = $Action; installed = [bool]$manifest; binaryPresent = $binaryPresent; sha256 = $hash; firewall = $state})
        exit 0
    }
    $principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Administrator consent is required to install or uninstall the node firewall rule.'
    }
    # Check ownership before copying files or changing any rule.
    $null = Get-NovovmRelayFirewallState $program
    if ($Action -eq 'Uninstall') {
        Remove-NovovmRelayFirewall
        if ($manifest) {
            # No recursive deletion; private node data/configuration is external.
            if (Test-Path -LiteralPath $program) { Remove-Item -LiteralPath $program }
            Remove-Item -LiteralPath $manifestPath
            if (@(Get-ChildItem -LiteralPath $installRoot -Force).Count -eq 0) {
                Remove-Item -LiteralPath $installRoot
            }
        }
        Write-InstallReport ([ordered]@{ok = $true; action = $Action; dataRemoved = $false; firewall = (Get-NovovmRelayFirewallState $program)})
        exit 0
    }
    if ($ExpectedSha256 -notmatch '^[0-9a-fA-F]{64}$' -or
        -not (Test-Path -LiteralPath $SourceBinary -PathType Leaf)) {
        throw 'Install requires an existing source binary and its independently verified SHA-256.'
    }
    $expected = $ExpectedSha256.ToLowerInvariant()
    if ((Get-FileHash -LiteralPath $SourceBinary -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
        throw 'Source binary SHA-256 mismatch.'
    }
    $existingHash = if (Test-Path -LiteralPath $program) { (Get-FileHash -LiteralPath $program -Algorithm SHA256).Hash.ToLowerInvariant() } else { '' }
    if ($existingHash -ne $expected) {
        New-Item -ItemType Directory -Force -Path $installRoot | Out-Null
        $staged = Join-Path $installRoot ('install-' + [guid]::NewGuid().ToString('N') + '.tmp')
        try {
            Copy-Item -LiteralPath $SourceBinary -Destination $staged
            if ((Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) { throw 'Copied binary SHA-256 mismatch.' }
            # Replacement fails if the existing process cannot be updated. No process is killed.
            Move-Item -LiteralPath $staged -Destination $program -Force
        } finally {
            if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged }
        }
    }
    # Record file ownership before OS policy, allowing safe retry after policy failure.
    [ordered]@{version = 1; product = 'NOVOVM Relay'; program = $program; sha256 = $expected} |
        ConvertTo-Json | Set-Content -LiteralPath $manifestPath -Encoding UTF8
    $state = Set-NovovmRelayFirewall $program
    Write-InstallReport ([ordered]@{ok = $true; action = $Action; program = $program; sha256 = $expected; firewall = $state; runtimeStarted = $false})
} catch {
    Write-InstallReport ([ordered]@{ok = $false; action = $Action; error = $_.Exception.Message})
    exit 1
}
