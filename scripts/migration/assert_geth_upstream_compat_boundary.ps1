param(
    [string]$RepoRoot = "",
    [string]$ReportPath = "artifacts/migration/geth-upstream-compat-after-a484a8506.md"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
    }
    return (Resolve-Path $Root).Path
}

function Resolve-FullPath {
    param(
        [string]$Root,
        [string]$Value
    )
    if ([System.IO.Path]::IsPathRooted($Value)) {
        return [System.IO.Path]::GetFullPath($Value)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $Root $Value))
}

function Assert-Contains {
    param(
        [string]$Path,
        [string]$Needle,
        [string]$Label
    )
    $text = Get-Content -Raw -Path $Path
    if (-not $text.Contains($Needle)) {
        throw "missing required report wording for ${Label}: $Needle"
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
Push-Location $RepoRoot
try {
    $nodeCargo = Resolve-FullPath -Root $RepoRoot -Value "crates/novovm-node/Cargo.toml"
    Assert-Contains -Path $nodeCargo -Needle "autobins = false" -Label "novovm-node explicit bin layout"
    Assert-Contains -Path $nodeCargo -Needle 'path = "src/bin/novovm-node.rs"' -Label "novovm-node live bin path"

    & git diff --quiet -- "crates/novovm-node/src/main.rs"
    if ($LASTEXITCODE -ne 0) {
        throw "boundary violation: dead/historical crates/novovm-node/src/main.rs has a diff"
    }

    & rg -q "evm_baseFee|evm_base_fee" crates
    if ($LASTEXITCODE -eq 0) {
        throw "boundary violation: non-geth evm_baseFee/evm_base_fee alias found under crates"
    }
    if ($LASTEXITCODE -ne 1) {
        throw "rg failed while checking evm_baseFee aliases"
    }

    $gatewayTests = Resolve-FullPath -Root $RepoRoot -Value "crates/gateways/evm-gateway/src/main_tests.rs"
    if (-not (Select-String -Path $gatewayTests -SimpleMatch "fn eth_base_fee_matches_fee_history_next_base_fee_and_rejects_params" -List)) {
        throw "eth_baseFee explicit regression test is missing"
    }
    if (-not (Select-String -Path $gatewayTests -SimpleMatch 'gateway_error_code_for_method("eth_baseFee"' -List)) {
        throw "eth_baseFee -32602 assertion is missing"
    }

    & rg -q "ETH_NATIVE_MAX_SUPPORTED_ETH_PROTOCOL_VERSION" crates/novovm-network/src
    if ($LASTEXITCODE -ne 0) {
        throw "native eth capability guard is missing"
    }
    & rg -q "eth_rlpx_is_unsupported_eth71_bal_message_v1" crates/novovm-network/src
    if ($LASTEXITCODE -ne 0) {
        throw "native BAL unsupported-safe classifier is missing"
    }

    & rg -q "GATEWAY_ETH_PLUGIN_RLPX_MAX_SUPPORTED_ETH_PROTO" crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs
    if ($LASTEXITCODE -ne 0) {
        throw "gateway RLPx capability guard is missing"
    }
    & rg -q "gateway_eth_rlpx_is_unsupported_eth71_bal_message" crates/gateways/evm-gateway/src/rpc_gateway_exec_cfg.rs
    if ($LASTEXITCODE -ne 0) {
        throw "gateway BAL unsupported-safe classifier is missing"
    }

    $report = Resolve-FullPath -Root $RepoRoot -Value $ReportPath
    if (-not (Test-Path $report)) {
        throw "report missing: $report"
    }
    Assert-Contains -Path $report -Needle "Code-level compatibility: PASS" -Label "code-level status"
    Assert-Contains -Path $report -Needle "Merge candidate: YES" -Label "merge candidate status"
    Assert-Contains -Path $report -Needle "EVM is a plugin capability, not the host identity" -Label "EVM plugin architecture boundary"
    Assert-Contains -Path $report -Needle 'The NOVOVM host/node binary is the configured `novovm-node` bin path:' -Label "NOVOVM host entrypoint boundary"
    Assert-Contains -Path $report -Needle "crates/novovm-node/src/bin/novovm-node.rs" -Label "NOVOVM host entrypoint path"
    Assert-Contains -Path $report -Needle "The active external Ethereum RPC / bridge edge for this compatibility patch is:" -Label "active external Ethereum edge boundary"
    Assert-Contains -Path $report -Needle "crates/gateways/evm-gateway" -Label "active external Ethereum edge path"
    Assert-Contains -Path $report -Needle 'The phrase "live gateway RPC path" means the active external Ethereum RPC edge, not the NOVOVM host entrypoint and not a new EVM plugin architecture.' -Label "live gateway phrase boundary"
    Assert-Contains -Path $report -Needle "One public probing run did not observe an RLPx auth ack" -Label "public canary narrow interpretation"
    Assert-Contains -Path $report -Needle 'not as a regression in `eth_baseFee`, `balHash`, BAL unsupported-safe handling, or the EVM plugin route' -Label "runtime note attribution boundary"
    Assert-Contains -Path $report -Needle "MEV / Uniswap observation result" -Label "MEV/Uniswap not-claimed boundary"
    Assert-Contains -Path $report -Needle "Old UnifiedAccountRouter RocksDB state migration" -Label "RocksDB migration not-claimed boundary"

    Write-Host "geth upstream compat boundary guard passed"
} finally {
    Pop-Location
}
