<# Read-only local interface preflight. No DNS, endpoint probes or configuration changes. #>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$InterfaceAlias,
    [string]$ReportPath
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
try {
    Import-Module (Join-Path $PSScriptRoot 'NovovmNodeNetwork.psm1') -Force
    $value = [ordered]@{ok = $true; collectedAtUtc = [DateTime]::UtcNow.ToString('o'); network = (Get-NovovmNodeNetwork $InterfaceAlias)}
} catch {
    $value = [ordered]@{ok = $false; error = $_.Exception.Message}
}
$json = $value | ConvertTo-Json -Depth 7
if ($ReportPath) {
    if (-not [IO.Path]::IsPathRooted($ReportPath)) { throw 'ReportPath must be absolute.' }
    [IO.File]::WriteAllText($ReportPath, $json, (New-Object Text.UTF8Encoding($false)))
}
$json
if (-not $value.ok) { exit 1 }
