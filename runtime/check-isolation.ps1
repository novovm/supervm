$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$comparison = if ($IsWindows) { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }
$legacyPrefix = [IO.Path]::GetFullPath((Join-Path $repoRoot 'legacy')) + [IO.Path]::DirectorySeparatorChar

# Twenty explicit original product members, plus the existing EVM core path
# dependency which Cargo automatically includes as a workspace member.
$productMembers = @(
    'crates/aoem-bindings',
    'crates/novovm-adapter-api',
    'crates/novovm-adapter-novovm',
    'crates/gateways/evm-gateway',
    'crates/novovm-adapter-sample-plugin',
    'crates/plugins/evm/plugin',
    'crates/plugins/evm/core',
    'crates/novovm-bench',
    'crates/novovm-consensus',
    'crates/novovm-coordinator',
    'crates/novovm-exec',
    'crates/novovm-governance-observability',
    'crates/novovm-network',
    'crates/novovm-relay',
    'crates/novovm-node',
    'crates/novovm-rollout-policy',
    'crates/novovm-udp-batch',
    'crates/novovmctl',
    'crates/novovm-protocol',
    'crates/novovm-prover',
    'vendor/web30-core'
)
$componentMembers = @('runtime/novovm-host', 'runtime/novovm-aoem', 'runtime/novovm-network')

function Test-WorkspaceBoundary {
    param([string]$Manifest, [string[]]$ExpectedMembers, [string]$Label)
    $expectedManifests = @($ExpectedMembers | ForEach-Object {
        [IO.Path]::GetFullPath((Join-Path $repoRoot "$_/Cargo.toml"))
    })
    $metadataJson = cargo metadata --manifest-path $Manifest --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw "$Label Cargo metadata failed" }
    $metadata = $metadataJson | ConvertFrom-Json
    $members = @($metadata.workspace_members)
    $seen = @()
    foreach ($package in $metadata.packages) {
        $path = [IO.Path]::GetFullPath($package.manifest_path)
        if ($path.StartsWith($legacyPrefix, $comparison)) {
            throw "$Label dependency resolves to legacy: $($package.name)"
        }
        # source=null identifies path packages, including transitive local
        # dependencies. Do not allow runtime/archive code to leak into product
        # assembly, or product code into the preserved component test workspace.
        if ($null -eq $package.source -and $expectedManifests -notcontains $path) {
            throw "$Label has an unapproved local dependency: $path"
        }
        if ($members -contains $package.id) {
            if ($expectedManifests -notcontains $path) {
                throw "$Label has an unapproved workspace member: $path"
            }
            $seen += $path
        }
    }
    foreach ($path in $expectedManifests) {
        if ($seen -notcontains $path) { throw "$Label is missing an approved member: $path" }
    }
    if ($seen.Count -ne $expectedManifests.Count) { throw "$Label workspace membership differs" }
    Write-Output "PASS: $Label has $($seen.Count) approved members and no legacy dependency."
}

Push-Location $repoRoot
try {
    Test-WorkspaceBoundary 'Cargo.toml' $productMembers 'Product'
    Test-WorkspaceBoundary 'runtime/Cargo.toml' $componentMembers 'Preserved components'
    Write-Output 'Assembly boundaries only; not execution ownership or mainnet acceptance.'
} finally {
    Pop-Location
}
