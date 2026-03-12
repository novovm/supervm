param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$SpoolDir = "artifacts/ingress/spool-smoke-real",
    [string]$UcaId = "",
    [string]$ExternalAddress = "",
    [switch]$SkipBuild,
    [switch]$SkipPipeline,
    [string]$EthNonRawSamplePath = "scripts/migration/baselines/gateway-eth-nonraw-regression-sample-v1.json",
    [string]$EthNonRawTxObjectSamplePath = "scripts/migration/baselines/gateway-eth-nonraw-tx-object-regression-sample-v1.json",
    [string]$EthContractCallSamplePath = "scripts/migration/baselines/gateway-eth-nonraw-contract-call-regression-sample-v1.json",
    [string]$EthContractDeploySamplePath = "scripts/migration/baselines/gateway-eth-nonraw-contract-deploy-regression-sample-v1.json",
    [string]$EthNonRawArraySamplePath = "scripts/migration/baselines/gateway-eth-nonraw-array-params-regression-sample-v1.json",
    [string]$Web30NonRawSamplePath = "scripts/migration/baselines/gateway-web30-nonraw-regression-sample-v1.json",
    [string]$SummaryOut = "artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json"
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

function Resolve-BinaryPath {
    param(
        [string]$TargetRoot,
        [string]$BinaryBaseName
    )
    $isWindowsOs = $env:OS -eq "Windows_NT"
    $exeName = if ($isWindowsOs) { "$BinaryBaseName.exe" } else { $BinaryBaseName }
    return (Join-Path $TargetRoot "debug\$exeName")
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

function Resolve-TxHashFromResult {
    param(
        [Parameter(Mandatory = $true)]
        [AllowNull()]
        $Result,
        [string]$Context = "rpc result"
    )
    if ($null -eq $Result) {
        throw "$Context is null"
    }
    if ($Result -is [string]) {
        $raw = $Result.Trim()
        if (-not $raw) {
            throw "$Context tx hash string is empty"
        }
        return $raw
    }
    if ($Result -is [psobject]) {
        $txHashProp = $Result.PSObject.Properties["tx_hash"]
        if ($null -ne $txHashProp -and -not [string]::IsNullOrWhiteSpace([string]$txHashProp.Value)) {
            return ([string]$txHashProp.Value).Trim()
        }
        $hashProp = $Result.PSObject.Properties["hash"]
        if ($null -ne $hashProp -and -not [string]::IsNullOrWhiteSpace([string]$hashProp.Value)) {
            return ([string]$hashProp.Value).Trim()
        }
    }
    throw "$Context missing tx hash (expected string result, or object.tx_hash/hash)"
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$SpoolDir = Resolve-FullPath -Root $RepoRoot -Value $SpoolDir
$LogDir = Resolve-FullPath -Root $RepoRoot -Value "artifacts/ingress/logs"
$EthNonRawSamplePath = Resolve-FullPath -Root $RepoRoot -Value $EthNonRawSamplePath
$EthNonRawTxObjectSamplePath = Resolve-FullPath -Root $RepoRoot -Value $EthNonRawTxObjectSamplePath
$EthContractCallSamplePath = Resolve-FullPath -Root $RepoRoot -Value $EthContractCallSamplePath
$EthContractDeploySamplePath = Resolve-FullPath -Root $RepoRoot -Value $EthContractDeploySamplePath
$EthNonRawArraySamplePath = Resolve-FullPath -Root $RepoRoot -Value $EthNonRawArraySamplePath
$Web30NonRawSamplePath = Resolve-FullPath -Root $RepoRoot -Value $Web30NonRawSamplePath
$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$SummaryDir = Split-Path -Parent $SummaryOut
New-Item -ItemType Directory -Force -Path $SpoolDir | Out-Null
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
if ($SummaryDir) {
    New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
}

$TargetRoot = if ($env:CARGO_TARGET_DIR) {
    [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
} else {
    Join-Path $RepoRoot "target"
}
if (-not $SkipBuild) {
    & cargo build -p novovm-evm-gateway
    if ($LASTEXITCODE -ne 0) {
        throw "build failed: novovm-evm-gateway"
    }
}
$GatewayExe = Resolve-BinaryPath -TargetRoot $TargetRoot -BinaryBaseName "novovm-evm-gateway"
if (-not (Test-Path $GatewayExe)) {
    throw "gateway binary not found: $GatewayExe"
}

if (Test-Path $SpoolDir) {
    Remove-Item -Recurse -Force $SpoolDir
}
New-Item -ItemType Directory -Force -Path $SpoolDir | Out-Null

$gwOut = Join-Path $LogDir "gateway-smoke.stdout.log"
$gwErr = Join-Path $LogDir "gateway-smoke.stderr.log"
if (Test-Path $gwOut) { Remove-Item -Force $gwOut }
if (Test-Path $gwErr) { Remove-Item -Force $gwErr }

$envMap = @{
    "NOVOVM_GATEWAY_BIND" = $GatewayBind
    "NOVOVM_GATEWAY_SPOOL_DIR" = $SpoolDir
    "NOVOVM_GATEWAY_MAX_REQUESTS" = "19"
}

$envState = Push-ProcessEnv -Environment $envMap
try {
    $gatewayProc = Start-Process `
        -FilePath $GatewayExe `
        -WorkingDirectory $RepoRoot `
        -RedirectStandardOutput $gwOut `
        -RedirectStandardError $gwErr `
        -PassThru `
        -NoNewWindow
} finally {
    Pop-ProcessEnv -State $envState
}

try {
    Start-Sleep -Milliseconds 800
    if ($gatewayProc.HasExited) {
        throw "gateway exited early"
    }
    if (-not (Test-Path $EthNonRawSamplePath)) {
        throw "eth non-raw regression sample not found: $EthNonRawSamplePath"
    }
    if (-not (Test-Path $EthNonRawTxObjectSamplePath)) {
        throw "eth non-raw tx-object regression sample not found: $EthNonRawTxObjectSamplePath"
    }
    if (-not (Test-Path $EthContractCallSamplePath)) {
        throw "eth contract-call regression sample not found: $EthContractCallSamplePath"
    }
    if (-not (Test-Path $EthContractDeploySamplePath)) {
        throw "eth contract-deploy regression sample not found: $EthContractDeploySamplePath"
    }
    if (-not (Test-Path $EthNonRawArraySamplePath)) {
        throw "eth non-raw array regression sample not found: $EthNonRawArraySamplePath"
    }
    if (-not (Test-Path $Web30NonRawSamplePath)) {
        throw "web30 non-raw regression sample not found: $Web30NonRawSamplePath"
    }

    $url = "http://$GatewayBind"
    if (-not $UcaId) {
        $UcaId = "uca-smoke-{0:yyyyMMddHHmmssfff}" -f (Get-Date)
    }
    $addr = $ExternalAddress
    if (-not $addr) {
        $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
        $buf = New-Object byte[] 20
        $rng.GetBytes($buf)
        $addr = "0x" + (($buf | ForEach-Object { $_.ToString("x2") }) -join "")
    }

    $req1 = '{"jsonrpc":"2.0","id":1,"method":"ua_createUca","params":{"uca_id":"' + $UcaId + '"}}'
    $r1 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req1

    $req2 = '{"jsonrpc":"2.0","id":2,"method":"ua_bindPersona","params":{"uca_id":"' + $UcaId + '","persona_type":"evm","chain_id":1,"external_address":"' + $addr + '"}}'
    $r2 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req2

    $req3 = '{"jsonrpc":"2.0","id":3,"method":"ua_bindPersona","params":{"uca_id":"' + $UcaId + '","persona_type":"web30","chain_id":1000,"external_address":"' + $addr + '"}}'
    $r3 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req3
    $req4 = '{"jsonrpc":"2.0","id":4,"method":"eth_sendRawTransaction","params":{"uca_id":"' + $UcaId + '","chain_id":1,"nonce":0,"from":"' + $addr + '","raw_tx":"0x04c0"}}'
    $r4 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req4
    $ethSample = Get-Content -Path $EthNonRawSamplePath -Raw | ConvertFrom-Json
    $ethSampleMethodProp = $ethSample.PSObject.Properties["method"]
    if ($null -eq $ethSampleMethodProp) {
        throw "invalid eth non-raw sample: missing method"
    }
    $ethSampleMethod = [string]$ethSampleMethodProp.Value
    if ($ethSampleMethod -ne "eth_sendTransaction") {
        throw "invalid eth non-raw sample method: expected eth_sendTransaction, got $ethSampleMethod"
    }
    $ethSampleParamsProp = $ethSample.PSObject.Properties["params"]
    if ($null -eq $ethSampleParamsProp -or $null -eq $ethSampleParamsProp.Value) {
        throw "invalid eth non-raw sample: missing params"
    }
    $ethSampleParams = $ethSampleParamsProp.Value
    $ethSampleParams | Add-Member -NotePropertyName "uca_id" -NotePropertyValue $UcaId -Force
    $ethSampleParams | Add-Member -NotePropertyName "from" -NotePropertyValue $addr -Force
    $ethSampleParams | Add-Member -NotePropertyName "external_address" -NotePropertyValue $addr -Force
    $ethSampleToProp = $ethSampleParams.PSObject.Properties["to"]
    if ($null -ne $ethSampleToProp -and [string]$ethSampleToProp.Value -eq "__EXTERNAL_ADDRESS__") {
        $ethSampleParams | Add-Member -NotePropertyName "to" -NotePropertyValue $addr -Force
    }
    $req5Object = [ordered]@{
        jsonrpc = "2.0"
        id = 5
        method = $ethSampleMethod
        params = $ethSampleParams
    }
    $req5 = $req5Object | ConvertTo-Json -Compress -Depth 32
    $r5 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req5

    $ethTxObjSample = Get-Content -Path $EthNonRawTxObjectSamplePath -Raw | ConvertFrom-Json
    $ethTxObjSampleMethodProp = $ethTxObjSample.PSObject.Properties["method"]
    if ($null -eq $ethTxObjSampleMethodProp) {
        throw "invalid eth non-raw tx-object sample: missing method"
    }
    $ethTxObjSampleMethod = [string]$ethTxObjSampleMethodProp.Value
    if ($ethTxObjSampleMethod -ne "eth_sendTransaction") {
        throw "invalid eth non-raw tx-object sample method: expected eth_sendTransaction, got $ethTxObjSampleMethod"
    }
    $ethTxObjSampleParamsProp = $ethTxObjSample.PSObject.Properties["params"]
    if ($null -eq $ethTxObjSampleParamsProp -or $null -eq $ethTxObjSampleParamsProp.Value) {
        throw "invalid eth non-raw tx-object sample: missing params"
    }
    $ethTxObjSampleParams = $ethTxObjSampleParamsProp.Value
    $ethTxObjSampleParams | Add-Member -NotePropertyName "uca_id" -NotePropertyValue $UcaId -Force
    $ethTxObjObjProp = $ethTxObjSampleParams.PSObject.Properties["tx"]
    if ($null -ne $ethTxObjObjProp -and $null -ne $ethTxObjObjProp.Value) {
        $ethTxObjObj = $ethTxObjObjProp.Value
        $ethTxObjFromProp = $ethTxObjObj.PSObject.Properties["from"]
        if ($null -ne $ethTxObjFromProp -and [string]$ethTxObjFromProp.Value -eq "__EXTERNAL_ADDRESS__") {
            $ethTxObjObj | Add-Member -NotePropertyName "from" -NotePropertyValue $addr -Force
        }
        $ethTxObjToProp = $ethTxObjObj.PSObject.Properties["to"]
        if ($null -ne $ethTxObjToProp -and [string]$ethTxObjToProp.Value -eq "__EXTERNAL_ADDRESS__") {
            $ethTxObjObj | Add-Member -NotePropertyName "to" -NotePropertyValue $addr -Force
        }
    }
    $req6Object = [ordered]@{
        jsonrpc = "2.0"
        id = 6
        method = $ethTxObjSampleMethod
        params = $ethTxObjSampleParams
    }
    $req6 = $req6Object | ConvertTo-Json -Compress -Depth 32
    $r6 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req6

    $ethCallSample = Get-Content -Path $EthContractCallSamplePath -Raw | ConvertFrom-Json
    $ethCallMethodProp = $ethCallSample.PSObject.Properties["method"]
    if ($null -eq $ethCallMethodProp) {
        throw "invalid eth contract-call sample: missing method"
    }
    $ethCallMethod = [string]$ethCallMethodProp.Value
    if ($ethCallMethod -ne "eth_sendTransaction") {
        throw "invalid eth contract-call sample method: expected eth_sendTransaction, got $ethCallMethod"
    }
    $ethCallParamsProp = $ethCallSample.PSObject.Properties["params"]
    if ($null -eq $ethCallParamsProp -or $null -eq $ethCallParamsProp.Value) {
        throw "invalid eth contract-call sample: missing params"
    }
    $ethCallParams = $ethCallParamsProp.Value
    $ethCallParams | Add-Member -NotePropertyName "uca_id" -NotePropertyValue $UcaId -Force
    $ethCallParams | Add-Member -NotePropertyName "from" -NotePropertyValue $addr -Force
    $ethCallParams | Add-Member -NotePropertyName "external_address" -NotePropertyValue $addr -Force
    $ethCallToProp = $ethCallParams.PSObject.Properties["to"]
    if ($null -ne $ethCallToProp -and [string]$ethCallToProp.Value -eq "__EXTERNAL_ADDRESS__") {
        $ethCallParams | Add-Member -NotePropertyName "to" -NotePropertyValue $addr -Force
    }
    $req7Object = [ordered]@{
        jsonrpc = "2.0"
        id = 7
        method = $ethCallMethod
        params = $ethCallParams
    }
    $req7 = $req7Object | ConvertTo-Json -Compress -Depth 32
    $r7 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req7

    $ethDeploySample = Get-Content -Path $EthContractDeploySamplePath -Raw | ConvertFrom-Json
    $ethDeployMethodProp = $ethDeploySample.PSObject.Properties["method"]
    if ($null -eq $ethDeployMethodProp) {
        throw "invalid eth contract-deploy sample: missing method"
    }
    $ethDeployMethod = [string]$ethDeployMethodProp.Value
    if ($ethDeployMethod -ne "eth_sendTransaction") {
        throw "invalid eth contract-deploy sample method: expected eth_sendTransaction, got $ethDeployMethod"
    }
    $ethDeployParamsProp = $ethDeploySample.PSObject.Properties["params"]
    if ($null -eq $ethDeployParamsProp -or $null -eq $ethDeployParamsProp.Value) {
        throw "invalid eth contract-deploy sample: missing params"
    }
    $ethDeployParams = $ethDeployParamsProp.Value
    $ethDeployParams | Add-Member -NotePropertyName "uca_id" -NotePropertyValue $UcaId -Force
    $ethDeployParams | Add-Member -NotePropertyName "from" -NotePropertyValue $addr -Force
    $ethDeployParams | Add-Member -NotePropertyName "external_address" -NotePropertyValue $addr -Force
    $req8Object = [ordered]@{
        jsonrpc = "2.0"
        id = 8
        method = $ethDeployMethod
        params = $ethDeployParams
    }
    $req8 = $req8Object | ConvertTo-Json -Compress -Depth 32
    $r8 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req8

    $ethArraySample = Get-Content -Path $EthNonRawArraySamplePath -Raw | ConvertFrom-Json
    $ethArrayMethodProp = $ethArraySample.PSObject.Properties["method"]
    if ($null -eq $ethArrayMethodProp) {
        throw "invalid eth non-raw array sample: missing method"
    }
    $ethArrayMethod = [string]$ethArrayMethodProp.Value
    if ($ethArrayMethod -ne "eth_sendTransaction") {
        throw "invalid eth non-raw array sample method: expected eth_sendTransaction, got $ethArrayMethod"
    }
    $ethArrayParamsProp = $ethArraySample.PSObject.Properties["params"]
    if ($null -eq $ethArrayParamsProp -or $null -eq $ethArrayParamsProp.Value) {
        throw "invalid eth non-raw array sample: missing params"
    }
    $ethArrayParams = $ethArrayParamsProp.Value
    if ($ethArrayParams.Count -lt 1) {
        throw "invalid eth non-raw array sample: params must include tx object"
    }
    $ethArrayTx = $ethArrayParams[0]
    $ethArrayTx | Add-Member -NotePropertyName "uca_id" -NotePropertyValue $UcaId -Force
    $ethArrayTx | Add-Member -NotePropertyName "from" -NotePropertyValue $addr -Force
    $ethArrayTx | Add-Member -NotePropertyName "external_address" -NotePropertyValue $addr -Force
    $ethArrayToProp = $ethArrayTx.PSObject.Properties["to"]
    if ($null -ne $ethArrayToProp -and [string]$ethArrayToProp.Value -eq "__EXTERNAL_ADDRESS__") {
        $ethArrayTx | Add-Member -NotePropertyName "to" -NotePropertyValue $addr -Force
    }
    $req9Object = [ordered]@{
        jsonrpc = "2.0"
        id = 9
        method = $ethArrayMethod
        params = $ethArrayParams
    }
    $req9 = $req9Object | ConvertTo-Json -Compress -Depth 32
    $r9 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req9

    $req10 = '{"jsonrpc":"2.0","id":10,"method":"web30_sendRawTransaction","params":{"uca_id":"' + $UcaId + '","chain_id":1000,"nonce":0,"from":"' + $addr + '","raw_tx":"0x0102","signature_domain":"web30:mainnet"}}'
    $r10 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req10
    $web30Sample = Get-Content -Path $Web30NonRawSamplePath -Raw | ConvertFrom-Json
    $sampleMethodProp = $web30Sample.PSObject.Properties["method"]
    if ($null -eq $sampleMethodProp) {
        throw "invalid web30 non-raw sample: missing method"
    }
    $sampleMethod = [string]$sampleMethodProp.Value
    if ($sampleMethod -ne "web30_sendTransaction") {
        throw "invalid web30 non-raw sample method: expected web30_sendTransaction, got $sampleMethod"
    }
    $sampleParamsProp = $web30Sample.PSObject.Properties["params"]
    if ($null -eq $sampleParamsProp -or $null -eq $sampleParamsProp.Value) {
        throw "invalid web30 non-raw sample: missing params"
    }
    $sampleParams = $sampleParamsProp.Value
    $sampleParams | Add-Member -NotePropertyName "uca_id" -NotePropertyValue $UcaId -Force
    $sampleParams | Add-Member -NotePropertyName "from" -NotePropertyValue $addr -Force
    $sampleTxProp = $sampleParams.PSObject.Properties["tx"]
    if ($null -ne $sampleTxProp -and $null -ne $sampleTxProp.Value) {
        $sampleTx = $sampleTxProp.Value
        $sampleToProp = $sampleTx.PSObject.Properties["to"]
        if ($null -ne $sampleToProp -and [string]$sampleToProp.Value -eq "__EXTERNAL_ADDRESS__") {
            $sampleTx | Add-Member -NotePropertyName "to" -NotePropertyValue $addr -Force
        }
    }
    $req11Object = [ordered]@{
        jsonrpc = "2.0"
        id = 11
        method = $sampleMethod
        params = $sampleParams
    }
    $req11 = $req11Object | ConvertTo-Json -Compress -Depth 32
    $r11 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req11

    $ethArrayTxHash = Resolve-TxHashFromResult -Result $r9.result -Context "eth_sendTransaction(array)"
    if (-not $ethArrayTxHash) {
        throw "eth non-raw array request missing tx_hash"
    }
    $req12Object = [ordered]@{
        jsonrpc = "2.0"
        id = 12
        method = "eth_getTransactionByHash"
        params = @($ethArrayTxHash)
    }
    $req12 = $req12Object | ConvertTo-Json -Compress -Depth 32
    $r12 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req12
    if ($null -eq $r12.result) {
        throw "eth_getTransactionByHash returned null for known tx hash: $ethArrayTxHash"
    }
    if ([string]$r12.result.hash -ne $ethArrayTxHash) {
        throw "eth_getTransactionByHash hash mismatch: expected=$ethArrayTxHash got=$([string]$r12.result.hash)"
    }

    $req13Object = [ordered]@{
        jsonrpc = "2.0"
        id = 13
        method = "eth_getTransactionReceipt"
        params = [ordered]@{
            tx_hash = $ethArrayTxHash
        }
    }
    $req13 = $req13Object | ConvertTo-Json -Compress -Depth 32
    $r13 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req13
    if ($null -eq $r13.result) {
        throw "eth_getTransactionReceipt returned null for known tx hash: $ethArrayTxHash"
    }
    if ([string]$r13.result.transactionHash -ne $ethArrayTxHash) {
        throw "eth_getTransactionReceipt hash mismatch: expected=$ethArrayTxHash got=$([string]$r13.result.transactionHash)"
    }

    $req14Object = [ordered]@{
        jsonrpc = "2.0"
        id = 14
        method = "eth_chainId"
        params = @()
    }
    $req14 = $req14Object | ConvertTo-Json -Compress -Depth 32
    $r14 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req14
    $r14ResultProp = $r14.PSObject.Properties["result"]
    if ($null -eq $r14ResultProp) {
        throw "eth_chainId response missing result: $($r14 | ConvertTo-Json -Compress)"
    }
    $chainIdHex = [string]$r14ResultProp.Value
    if (-not $chainIdHex -or -not $chainIdHex.StartsWith("0x")) {
        throw "eth_chainId returned invalid result: $chainIdHex"
    }
    $chainIdDec = [Convert]::ToUInt64($chainIdHex.Substring(2), 16)

    $req15Object = [ordered]@{
        jsonrpc = "2.0"
        id = 15
        method = "net_version"
        params = @()
    }
    $req15 = $req15Object | ConvertTo-Json -Compress -Depth 32
    $r15 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req15
    $r15ResultProp = $r15.PSObject.Properties["result"]
    if ($null -eq $r15ResultProp) {
        throw "net_version response missing result: $($r15 | ConvertTo-Json -Compress)"
    }
    $netVersion = [string]$r15ResultProp.Value
    if (-not $netVersion) {
        throw "net_version returned empty result"
    }
    [UInt64]$netVersionDec = 0
    if (-not [UInt64]::TryParse($netVersion, [ref]$netVersionDec)) {
        throw "net_version returned non-decimal result: $netVersion"
    }
    if ($netVersionDec -ne $chainIdDec) {
        throw "eth_chainId/net_version mismatch: chainId=$chainIdDec net_version=$netVersionDec"
    }

    $req16Object = [ordered]@{
        jsonrpc = "2.0"
        id = 16
        method = "eth_gasPrice"
        params = @()
    }
    $req16 = $req16Object | ConvertTo-Json -Compress -Depth 32
    $r16 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req16
    $r16ResultProp = $r16.PSObject.Properties["result"]
    if ($null -eq $r16ResultProp) {
        throw "eth_gasPrice response missing result: $($r16 | ConvertTo-Json -Compress)"
    }
    $gasPriceHex = [string]$r16ResultProp.Value
    if (-not $gasPriceHex -or -not $gasPriceHex.StartsWith("0x")) {
        throw "eth_gasPrice returned invalid result: $gasPriceHex"
    }
    $null = [Convert]::ToUInt64($gasPriceHex.Substring(2), 16)

    $req17Object = [ordered]@{
        jsonrpc = "2.0"
        id = 17
        method = "eth_estimateGas"
        params = [ordered]@{
            chain_id = 1
            from = $addr
            to = $addr
            data = "0x010203"
        }
    }
    $req17 = $req17Object | ConvertTo-Json -Compress -Depth 32
    $r17 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req17
    $r17ResultProp = $r17.PSObject.Properties["result"]
    if ($null -eq $r17ResultProp) {
        throw "eth_estimateGas response missing result: $($r17 | ConvertTo-Json -Compress)"
    }
    $estimateGasHex = [string]$r17ResultProp.Value
    if (-not $estimateGasHex -or -not $estimateGasHex.StartsWith("0x")) {
        throw "eth_estimateGas returned invalid result: $estimateGasHex"
    }
    $estimateGasDec = [Convert]::ToUInt64($estimateGasHex.Substring(2), 16)
    if ($estimateGasDec -ne 21048) {
        throw "eth_estimateGas mismatch: expected=21048 got=$estimateGasDec (hex=$estimateGasHex)"
    }

    $req18Object = [ordered]@{
        jsonrpc = "2.0"
        id = 18
        method = "eth_getCode"
        params = @($addr, "latest")
    }
    $req18 = $req18Object | ConvertTo-Json -Compress -Depth 32
    $r18 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req18
    $r18ResultProp = $r18.PSObject.Properties["result"]
    if ($null -eq $r18ResultProp) {
        throw "eth_getCode response missing result: $($r18 | ConvertTo-Json -Compress)"
    }
    $codeHex = [string]$r18ResultProp.Value
    if ($codeHex -ne "0x") {
        throw "eth_getCode mismatch: expected=0x got=$codeHex"
    }

    $req19Object = [ordered]@{
        jsonrpc = "2.0"
        id = 19
        method = "eth_getStorageAt"
        params = @($addr, "0x0", "latest")
    }
    $req19 = $req19Object | ConvertTo-Json -Compress -Depth 32
    $r19 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $req19
    $r19ResultProp = $r19.PSObject.Properties["result"]
    if ($null -eq $r19ResultProp) {
        throw "eth_getStorageAt response missing result: $($r19 | ConvertTo-Json -Compress)"
    }
    $storageHex = [string]$r19ResultProp.Value
    if ($storageHex -ne "0x0000000000000000000000000000000000000000000000000000000000000000") {
        throw "eth_getStorageAt mismatch: expected=32-byte-zero got=$storageHex"
    }

    Wait-Process -Id $gatewayProc.Id -Timeout 10

    $files = @(Get-ChildItem -Path $SpoolDir -File -Filter *.opsw1)
    if ($files.Count -lt 8) {
        $diag = @(
            "r4=$($r4 | ConvertTo-Json -Compress)",
            "r5=$($r5 | ConvertTo-Json -Compress)",
            "r6=$($r6 | ConvertTo-Json -Compress)",
            "r7=$($r7 | ConvertTo-Json -Compress)",
            "r8=$($r8 | ConvertTo-Json -Compress)",
            "r9=$($r9 | ConvertTo-Json -Compress)",
            "r10=$($r10 | ConvertTo-Json -Compress)",
            "r11=$($r11 | ConvertTo-Json -Compress)",
            "r12=$($r12 | ConvertTo-Json -Compress)",
            "r13=$($r13 | ConvertTo-Json -Compress)",
            "r14=$($r14 | ConvertTo-Json -Compress)",
            "r15=$($r15 | ConvertTo-Json -Compress)",
            "r16=$($r16 | ConvertTo-Json -Compress)",
            "r17=$($r17 | ConvertTo-Json -Compress)",
            "r18=$($r18 | ConvertTo-Json -Compress)",
            "r19=$($r19 | ConvertTo-Json -Compress)"
        ) -join " | "
        throw "expected >=8 opsw1 records in smoke spool (eth raw + eth nonraw + eth nonraw tx-object + eth contract-call + eth contract-deploy + eth nonraw array + web30 raw + web30 nonraw), got $($files.Count); responses: $diag"
    }
    $pipelineExitCode = 0
    $pipelineOutput = @()
    if (-not $SkipPipeline) {
        $pipelineScript = Resolve-FullPath -Root $RepoRoot -Value "scripts/migration/run_gateway_node_pipeline.ps1"
        $pipelineOutput = @(
            & powershell -NoProfile -ExecutionPolicy Bypass -File $pipelineScript `
                -RepoRoot $RepoRoot `
                -SpoolDir $SpoolDir `
                -SkipGatewayStart `
                -RunOnce `
                -SkipBuild 2>&1
        )
        $pipelineExitCode = $LASTEXITCODE
        if ($pipelineExitCode -ne 0) {
            throw "pipeline failed (exit=$pipelineExitCode): $($pipelineOutput -join "`n")"
        }
    }
    $summary = [ordered]@{
        gateway_bind = $GatewayBind
        spool_dir = $SpoolDir
        uca_id = $UcaId
        external_address = $addr
        eth_nonraw_sample = $EthNonRawSamplePath
        eth_nonraw_tx_object_sample = $EthNonRawTxObjectSamplePath
        eth_contract_call_sample = $EthContractCallSamplePath
        eth_contract_deploy_sample = $EthContractDeploySamplePath
        eth_nonraw_array_sample = $EthNonRawArraySamplePath
        web30_nonraw_sample = $Web30NonRawSamplePath
        requests = @(
            ($r1 | ConvertTo-Json -Compress),
            ($r2 | ConvertTo-Json -Compress),
            ($r3 | ConvertTo-Json -Compress),
            ($r4 | ConvertTo-Json -Compress),
            ($r5 | ConvertTo-Json -Compress),
            ($r6 | ConvertTo-Json -Compress),
            ($r7 | ConvertTo-Json -Compress),
            ($r8 | ConvertTo-Json -Compress),
            ($r9 | ConvertTo-Json -Compress),
            ($r10 | ConvertTo-Json -Compress),
            ($r11 | ConvertTo-Json -Compress),
            ($r12 | ConvertTo-Json -Compress),
            ($r13 | ConvertTo-Json -Compress),
            ($r14 | ConvertTo-Json -Compress),
            ($r15 | ConvertTo-Json -Compress),
            ($r16 | ConvertTo-Json -Compress),
            ($r17 | ConvertTo-Json -Compress),
            ($r18 | ConvertTo-Json -Compress),
            ($r19 | ConvertTo-Json -Compress)
        )
        opsw1_count = $files.Count
        opsw1_files = @($files | ForEach-Object { $_.FullName })
        pipeline = [ordered]@{
            enabled = (-not $SkipPipeline)
            exit_code = $pipelineExitCode
            output = @($pipelineOutput | ForEach-Object { $_.ToString() })
        }
        summary_out = $SummaryOut
    }
    $summaryJson = $summary | ConvertTo-Json -Depth 16
    Set-Content -Path $SummaryOut -Value $summaryJson -Encoding UTF8
    $summaryJson
} finally {
    if ($null -ne $gatewayProc -and -not $gatewayProc.HasExited) {
        Stop-Process -Id $gatewayProc.Id -Force
    }
}
