Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$checker = Join-Path $repoRoot 'runtime/check-isolation.ps1'

# Keep this fixture roster independent of the checker so dropping an approved
# member from either side cannot silently turn the positive case into a pass.
$productPaths = @(
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
    'crates/novovm-storage-budget',
    'crates/novovm-relay',
    'crates/novovm-node',
    'crates/novovm-rollout-policy',
    'crates/novovm-udp-batch',
    'crates/novovmctl',
    'crates/novovm-protocol',
    'crates/novovm-prover',
    'vendor/web30-core'
)
$componentPaths = @('runtime/novovm-host', 'runtime/novovm-aoem', 'runtime/novovm-network')

function Assert-True([bool]$Value, [string]$Message) {
    if (-not $Value) { throw $Message }
}

function New-LocalPackage([string]$Path) {
    $manifest = [IO.Path]::GetFullPath((Join-Path $repoRoot "$Path/Cargo.toml"))
    [PSCustomObject]@{
        name = Split-Path $Path -Leaf
        id = "fixture:$Path"
        source = $null
        manifest_path = $manifest
    }
}

function New-Metadata([string[]]$Paths) {
    $packages = @($Paths | ForEach-Object { New-LocalPackage $_ })
    [PSCustomObject]@{
        packages = $packages
        workspace_members = @($packages | ForEach-Object { $_.id })
    }
}

function Invoke-CheckerFixture {
    param($Product, $Components, [string]$FailedManifest = '')
    $fixtureState = @{
        Metadata = @{
            'Cargo.toml' = $Product
            'runtime/Cargo.toml' = $Components
        }
        FailedManifest = $FailedManifest
        Calls = [Collections.Generic.List[string]]::new()
    }
    $script:cargoCalls = $fixtureState.Calls
    # Shadow Cargo only in the test boundary; invoke the actual checker intact.
    function cargo {
        if ($args.Count -ne 6 -or $args[0] -ne 'metadata' -or
            $args[1] -ne '--manifest-path' -or $args[3] -ne '--locked' -or
            $args[4] -ne '--format-version' -or $args[5] -ne '1') {
            throw 'Unexpected Cargo command in isolation checker'
        }
        $manifest = [string]$args[2]
        if (-not $fixtureState.Metadata.ContainsKey($manifest)) {
            throw "Unexpected manifest: $manifest"
        }
        $fixtureState.Calls.Add($manifest)
        $global:LASTEXITCODE = if ($manifest -eq $fixtureState.FailedManifest) { 101 } else { 0 }
        $fixtureState.Metadata[$manifest] | ConvertTo-Json -Depth 8 -Compress
    }
    & $checker
}

function Assert-Rejected {
    param($Product, $Components, [string]$Expected, [string]$FailedManifest = '')
    $location = (Get-Location).Path
    $rejected = $false
    try {
        Invoke-CheckerFixture $Product $Components $FailedManifest | Out-Null
    } catch {
        if (-not $_.Exception.Message.Contains($Expected)) { throw }
        $rejected = $true
    }
    Assert-True $rejected "Expected rejection: $Expected"
    Assert-True ((Get-Location).Path -eq $location) 'Rejection changed caller location'
    Write-Output "PASS: rejects $Expected"
}

$product = New-Metadata $productPaths
$components = New-Metadata $componentPaths
$location = (Get-Location).Path
$output = @(Invoke-CheckerFixture $product $components)
Assert-True ($output -contains 'PASS: Product has 22 approved members and no legacy dependency.') 'Product roster was not accepted'
Assert-True ($output -contains 'PASS: Preserved components has 3 approved members and no legacy dependency.') 'Component roster was not accepted'
Assert-True (($script:cargoCalls -join ',') -eq 'Cargo.toml,runtime/Cargo.toml') 'Both workspaces must be checked'
Assert-True ((Get-Location).Path -eq $location) 'Success changed caller location'
Write-Output 'PASS: exact product roster including storage budget and preserved component roster'

# Real Cargo metadata can contain case-distinct feature names even when that
# dependency targets another platform. The old PSCustomObject parser rejects
# this valid JSON before either workspace boundary is evaluated.
$caseFeatures = [Collections.Generic.Dictionary[string, object]]::new([StringComparer]::Ordinal)
$caseFeatures.Add('USB', @('feature-a'))
$caseFeatures.Add('usb', @('feature-b'))
$caseProduct = New-Metadata $productPaths
$caseProduct.packages[0] | Add-Member -NotePropertyName features -NotePropertyValue $caseFeatures
$caseOutput = @(Invoke-CheckerFixture $caseProduct $components)
Assert-True ($caseOutput -contains 'PASS: Product has 22 approved members and no legacy dependency.') 'Valid case-distinct Cargo features were rejected'
Assert-True ($caseOutput -contains 'PASS: Preserved components has 3 approved members and no legacy dependency.') 'Feature parsing skipped the component boundary'
Write-Output 'PASS: valid metadata preserves case-distinct USB/usb Cargo features'

# Transitive path dependencies are not workspace members and must still fail.
foreach ($path in @('crates/unapproved-fixture', 'vendor/unapproved-fixture')) {
    $product = New-Metadata $productPaths
    $package = New-LocalPackage $path
    $product.packages += $package
    Assert-Rejected $product $components "Product has an unapproved local dependency: $($package.manifest_path)"
}

$product = New-Metadata $productPaths
$product.packages += New-LocalPackage 'legacy/supervm-20261002/unapproved-fixture'
Assert-Rejected $product $components 'Product dependency resolves to legacy: unapproved-fixture'

# Being present as a dependency is insufficient: it must remain a member too.
$product = New-Metadata $productPaths
$product.workspace_members = @($product.workspace_members | Where-Object { $_ -ne 'fixture:crates/novovm-storage-budget' })
$storageManifest = (New-LocalPackage 'crates/novovm-storage-budget').manifest_path
Assert-Rejected $product $components "Product is missing an approved member: $storageManifest"

$product = New-Metadata @($productPaths | Where-Object { $_ -ne 'crates/novovm-storage-budget' })
Assert-Rejected $product $components "Product is missing an approved member: $storageManifest"

$product = New-Metadata $productPaths
$package = New-LocalPackage 'runtime/novovm-host'
$product.packages += $package
Assert-Rejected $product $components "Product has an unapproved local dependency: $($package.manifest_path)"

$product = New-Metadata $productPaths
$components = New-Metadata $componentPaths
$package = New-LocalPackage 'crates/novovm-storage-budget'
$components.packages += $package
Assert-Rejected $product $components "Preserved components has an unapproved local dependency: $($package.manifest_path)"

$components = New-Metadata $componentPaths
$components.packages += New-LocalPackage 'legacy/supervm-20261002/unapproved-fixture'
Assert-Rejected $product $components 'Preserved components dependency resolves to legacy: unapproved-fixture'

$components = New-Metadata @($componentPaths | Where-Object { $_ -ne 'runtime/novovm-host' })
$hostManifest = (New-LocalPackage 'runtime/novovm-host').manifest_path
Assert-Rejected $product $components "Preserved components is missing an approved member: $hostManifest"

$components = New-Metadata $componentPaths
Assert-Rejected $product $components 'Product Cargo metadata failed' 'Cargo.toml'
Assert-True (($script:cargoCalls -join ',') -eq 'Cargo.toml') 'Failed product metadata must stop the checker'
Assert-Rejected $product $components 'Preserved components Cargo metadata failed' 'runtime/Cargo.toml'
Write-Output 'PASS: workspace isolation fixtures (2 accepted, 11 refused); no Cargo build invoked'
exit 0
