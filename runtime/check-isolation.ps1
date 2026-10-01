$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location $repoRoot
try {
    $metadataJson = cargo metadata --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $metadata = $metadataJson | ConvertFrom-Json
    $activePrefix = [IO.Path]::GetFullPath((Join-Path $repoRoot 'runtime')) + [IO.Path]::DirectorySeparatorChar
    $legacyPrefix = [IO.Path]::GetFullPath((Join-Path $repoRoot 'legacy')) + [IO.Path]::DirectorySeparatorChar
    $comparison = if ($IsWindows) { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }
    $members = @($metadata.workspace_members)
    if ($members.Count -eq 0) { throw 'Replacement workspace has no members' }
    foreach ($package in $metadata.packages) {
        $manifest = [IO.Path]::GetFullPath($package.manifest_path)
        if ($manifest.StartsWith($legacyPrefix, $comparison)) {
            throw "Active dependency resolves to legacy: $($package.name)"
        }
        if ($members -contains $package.id) {
            if (-not $manifest.StartsWith($activePrefix, $comparison)) {
                throw "Active member is outside runtime: $($package.name)"
            }
        }
    }
    Write-Output "PASS: $($members.Count) active runtime member(s), no legacy package dependency. This is not blockchain acceptance."
} finally {
    Pop-Location
}
