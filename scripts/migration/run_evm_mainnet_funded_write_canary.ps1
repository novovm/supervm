param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$UpstreamRpc = "",
    [string]$PrivateKey = "",
    [string]$PrivateKeyEnvName = "NOVOVM_EVM_MAINNET_CANARY_PRIVATE_KEY",
    [string]$ToAddress = "0x000000000000000000000000000000000000dEaD",
    [string]$UcaId = "",
    [UInt64]$ChainId = 1,
    [int]$PollMaxAttempts = 45,
    [int]$PollIntervalMs = 3000,
    [UInt64]$UpstreamTimeoutMs = 20000,
    [string]$NpmWorkspace = "artifacts/tmp-mainnet-canary",
    [string]$WriteSummaryOut = "artifacts/migration/evm-mainnet-write-canary-funded-summary.json",
    [string]$SummaryOut = "artifacts/migration/evm-mainnet-funded-write-canary-summary.json",
    [switch]$AllowBroadcastFailure,
    [switch]$SkipBuild
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

function Push-ProcessEnv {
    param([hashtable]$Environment)
    $state = @{}
    if ($null -eq $Environment) {
        return $state
    }
    foreach ($key in $Environment.Keys) {
        $envPath = "Env:$key"
        $exists = Test-Path $envPath
        $oldValue = $null
        if ($exists) {
            $oldValue = (Get-Item -Path $envPath).Value
        }
        $state[$key] = [pscustomobject]@{
            exists = $exists
            value = $oldValue
        }
        Set-Item -Path $envPath -Value ([string]$Environment[$key])
    }
    return $state
}

function Pop-ProcessEnv {
    param([hashtable]$State)
    if ($null -eq $State) {
        return
    }
    foreach ($key in $State.Keys) {
        $entry = $State[$key]
        $envPath = "Env:$key"
        if ($entry.exists) {
            Set-Item -Path $envPath -Value ([string]$entry.value)
        } else {
            Remove-Item -Path $envPath -ErrorAction SilentlyContinue
        }
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$NpmWorkspace = Resolve-FullPath -Root $RepoRoot -Value $NpmWorkspace
$WriteSummaryOut = Resolve-FullPath -Root $RepoRoot -Value $WriteSummaryOut
$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
New-Item -ItemType Directory -Force -Path $NpmWorkspace | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $WriteSummaryOut) | Out-Null
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $SummaryOut) | Out-Null

if (-not $UpstreamRpc) {
    $UpstreamRpc = $env:NOVOVM_GATEWAY_ETH_UPSTREAM_RPC
}
if (-not $UpstreamRpc) {
    throw "missing UpstreamRpc: pass -UpstreamRpc or set NOVOVM_GATEWAY_ETH_UPSTREAM_RPC"
}

if (-not $PrivateKey) {
    $pkItem = Get-Item -Path ("Env:{0}" -f $PrivateKeyEnvName) -ErrorAction SilentlyContinue
    if ($null -ne $pkItem -and $null -ne $pkItem.Value) {
        $PrivateKey = [string]$pkItem.Value
    }
}
if (-not $PrivateKey) {
    throw ("missing PrivateKey: pass -PrivateKey or set env {0}" -f $PrivateKeyEnvName)
}

if (-not $UcaId) {
    $UcaId = "uca-mainnet-funded-{0:yyyyMMddHHmmssfff}" -f (Get-Date)
}

$packageJson = Join-Path $NpmWorkspace "package.json"
if (-not (Test-Path $packageJson)) {
    & npm --prefix $NpmWorkspace init -y | Out-Null
}

$needInstallEthers = $true
Push-Location $NpmWorkspace
try {
    & node -e "try{require('ethers');process.exit(0)}catch(e){process.exit(1)}"
    if ($LASTEXITCODE -eq 0) {
        $needInstallEthers = $false
    }
} finally {
    Pop-Location
}
if ($needInstallEthers) {
    & npm --prefix $NpmWorkspace install ethers@6 --no-fund --no-audit
    if ($LASTEXITCODE -ne 0) {
        throw "npm install ethers failed"
    }
}

$generatorScriptPath = Join-Path $NpmWorkspace "gen_funded_rawtx.js"
$generatorScript = @'
const { Wallet, JsonRpcProvider, parseUnits } = require('ethers');

function toBigIntOrNull(v) {
  if (v === undefined || v === null || v === '') return null;
  return BigInt(v);
}

async function main() {
  const rpc = process.env.RPC_URL;
  const privateKey = process.env.PRIVATE_KEY;
  const toAddr = process.env.TO_ADDRESS;
  const chainId = Number(process.env.CHAIN_ID || '1');

  if (!rpc) throw new Error('RPC_URL missing');
  if (!privateKey) throw new Error('PRIVATE_KEY missing');
  if (!toAddr) throw new Error('TO_ADDRESS missing');

  const provider = new JsonRpcProvider(rpc, chainId);
  const wallet = new Wallet(privateKey, provider);

  const nonce = await provider.getTransactionCount(wallet.address, 'pending');
  const fee = await provider.getFeeData();

  const oneGwei = parseUnits('1', 'gwei');
  const minMaxFee = parseUnits('30', 'gwei');
  let maxPriorityFeePerGas = fee.maxPriorityFeePerGas ?? oneGwei;
  if (maxPriorityFeePerGas < oneGwei) {
    maxPriorityFeePerGas = oneGwei;
  }
  let maxFeePerGas = fee.maxFeePerGas ?? ((fee.gasPrice ?? parseUnits('20', 'gwei')) * 2n);
  if (maxFeePerGas < maxPriorityFeePerGas + oneGwei) {
    maxFeePerGas = maxPriorityFeePerGas + oneGwei;
  }
  if (maxFeePerGas < minMaxFee) {
    maxFeePerGas = minMaxFee;
  }

  const tx = {
    type: 2,
    chainId,
    nonce,
    to: toAddr,
    value: 0n,
    gasLimit: 21000n,
    maxPriorityFeePerGas,
    maxFeePerGas
  };

  const rawTx = await wallet.signTransaction(tx);
  process.stdout.write(JSON.stringify({
    address: wallet.address,
    nonce,
    maxPriorityFeePerGas: maxPriorityFeePerGas.toString(),
    maxFeePerGas: maxFeePerGas.toString(),
    rawTx
  }));
}

main().catch((e) => {
  const msg = e && e.stack ? e.stack : String(e);
  process.stderr.write(msg);
  process.exit(1);
});
'@
Set-Content -Path $generatorScriptPath -Value $generatorScript -Encoding UTF8

$genEnv = @{
    "RPC_URL" = $UpstreamRpc
    "PRIVATE_KEY" = $PrivateKey
    "TO_ADDRESS" = $ToAddress
    "CHAIN_ID" = ([string]$ChainId)
}
$genEnvState = Push-ProcessEnv -Environment $genEnv
try {
    Push-Location $NpmWorkspace
    try {
        $genJson = & node $generatorScriptPath
    } finally {
        Pop-Location
    }
} finally {
    Pop-ProcessEnv -State $genEnvState
}
if ($LASTEXITCODE -ne 0) {
    throw "generate raw tx failed"
}
if (-not $genJson) {
    throw "generate raw tx returned empty output"
}

$gen = $genJson | ConvertFrom-Json
if (-not $gen.rawTx) {
    throw "generated rawTx missing"
}
if (-not $gen.address) {
    throw "generated address missing"
}

$writeScript = Join-Path $RepoRoot "scripts/migration/run_evm_mainnet_write_canary.ps1"
if (-not (Test-Path $writeScript)) {
    throw "write canary script missing: $writeScript"
}

$writeArgs = @{
    RepoRoot = $RepoRoot
    GatewayBind = $GatewayBind
    UpstreamRpc = $UpstreamRpc
    RawTx = [string]$gen.rawTx
    UcaId = $UcaId
    FromAddress = [string]$gen.address
    ChainId = [UInt64]$ChainId
    PollMaxAttempts = [int]$PollMaxAttempts
    PollIntervalMs = [int]$PollIntervalMs
    UpstreamTimeoutMs = [UInt64]$UpstreamTimeoutMs
    SummaryOut = $WriteSummaryOut
}
if ($SkipBuild) {
    $writeArgs["SkipBuild"] = $true
}

$writeSucceeded = $false
$writeError = $null
try {
    & $writeScript @writeArgs
    $writeSucceeded = $true
} catch {
    $writeError = $_.Exception.Message
}

$writeSummary = $null
if (Test-Path $WriteSummaryOut) {
    $writeSummary = Get-Content -Path $WriteSummaryOut -Raw | ConvertFrom-Json
}

$summary = [ordered]@{
    started_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    upstream_rpc = $UpstreamRpc
    chain_id = $ChainId
    uca_id = $UcaId
    signer_address = [string]$gen.address
    nonce = [int]$gen.nonce
    max_priority_fee_per_gas_wei = [string]$gen.maxPriorityFeePerGas
    max_fee_per_gas_wei = [string]$gen.maxFeePerGas
    write_succeeded = $writeSucceeded
    write_error = $writeError
    write_summary_path = $WriteSummaryOut
    write_summary = $writeSummary
}
$summary.completed_at_utc = [DateTimeOffset]::UtcNow.ToString("o")

$summary | ConvertTo-Json -Depth 30 | Set-Content -Path $SummaryOut -Encoding UTF8
Write-Host ("summary written: {0}" -f $SummaryOut)
if ($writeSucceeded) {
    Write-Host "mainnet funded write canary finished"
} else {
    Write-Host ("mainnet funded write canary failed: {0}" -f $writeError)
}
if (-not $writeSucceeded -and -not $AllowBroadcastFailure) {
    throw ("funded write canary failed: {0}" -f $writeError)
}
