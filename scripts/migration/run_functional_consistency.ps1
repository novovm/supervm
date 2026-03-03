param(
    [string]$RepoRoot = "D:\WorksArea\SUPERVM",
    [string]$OutputDir = "D:\WorksArea\SUPERVM\artifacts\migration\functional",
    [int]$Rounds = 200,
    [int]$Points = 1024,
    [int]$KeySpace = 251,
    [double]$Rw = 0.5,
    [int]$Seed = 123,
    [bool]$IncludeCapabilitySnapshot = $true,
    [bool]$IncludeNetworkProcessSignal = $true,
    [ValidateSet("core", "persist", "wasm")]
    [string]$CapabilityVariant = "core",
    [string]$CapabilityJson = "",
    [string]$NetworkProcessJson = "",
    [ValidateRange(1, 1024)]
    [int]$BatchADemoTxs = 8,
    [ValidateRange(1, 256)]
    [int]$BatchABatchCount = 2,
    [ValidateRange(1, 1000000)]
    [int64]$BatchAMempoolFeeFloor = 1,
    [ValidateRange(2, 12)]
    [int]$NetworkProcessNodeCount = 2,
    [ValidateRange(1, 50)]
    [int]$NetworkProcessRounds = 1,
    [ValidateSet("auto", "native", "plugin")]
    [string]$AdapterBackend = "auto",
    [string]$AdapterPluginPath = "",
    [string]$AdapterExpectedChain = "novovm",
    [ValidateSet("auto", "native", "plugin")]
    [string]$AdapterExpectedBackend = "auto",
    [ValidateRange(1, 2147483647)]
    [int]$AdapterPluginExpectedAbi = 1,
    [string]$AdapterPluginRequiredCaps = "0x1",
    [string]$AdapterPluginRegistryPath = "",
    [bool]$AdapterPluginRegistryStrict = $false,
    [string]$AdapterPluginRegistrySha256 = "",
    [bool]$IncludeAdapterBackendCompare = $false,
    [string]$AdapterComparePluginPath = "",
    [bool]$IncludeAdapterPluginAbiNegative = $false,
    [string]$AdapterNegativePluginPath = "",
    [bool]$IncludeAdapterPluginSymbolNegative = $false,
    [string]$AdapterSymbolNegativePluginPath = "",
    [bool]$IncludeAdapterPluginRegistryNegative = $false,
    [bool]$IncludeNetworkBlockWireNegative = $false
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-Cargo {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs,
        [hashtable]$EnvVars
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    foreach ($k in $EnvVars.Keys) {
        $psi.Environment[$k] = [string]$EnvVars[$k]
    }

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()

    $text = ($stdout + $stderr).Trim()
    if ($proc.ExitCode -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed in $WorkDir`n$text"
    }
    return $text
}

function Invoke-CargoAllowFailure {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs,
        [hashtable]$EnvVars
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    foreach ($k in $EnvVars.Keys) {
        $psi.Environment[$k] = [string]$EnvVars[$k]
    }

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()

    $text = ($stdout + $stderr).Trim()
    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        output = $text
    }
}

function Parse-NodeReportLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^mode=ffi_v2 variant=" } | Select-Object -Last 1)
    if (-not $line) {
        throw "novovm-node output missing final report line"
    }
    $m = [regex]::Match(
        $line,
        "^mode=ffi_v2 variant=(?<variant>\w+) dll=(?<dll>.+?) rc=(?<rc>\d+)\((?<rc_name>[^)]+)\) submitted=(?<submitted>\d+) processed=(?<processed>\d+) success=(?<success>\d+) writes=(?<writes>\d+) elapsed_us=(?<elapsed>\d+)$"
    )
    if (-not $m.Success) {
        throw "cannot parse novovm-node report line: $line"
    }
    return [ordered]@{
        variant   = $m.Groups["variant"].Value
        dll       = $m.Groups["dll"].Value
        rc        = [int]$m.Groups["rc"].Value
        rc_name   = $m.Groups["rc_name"].Value
        submitted = [int]$m.Groups["submitted"].Value
        processed = [int]$m.Groups["processed"].Value
        success   = [int]$m.Groups["success"].Value
        writes    = [int64]$m.Groups["writes"].Value
        elapsed_us = [int64]$m.Groups["elapsed"].Value
    }
}

function Parse-TxMetaLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^tx_meta:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^tx_meta:\s+accounts=(?<accounts>\d+)\s+txs=(?<txs>\d+)\s+min_fee=(?<min_fee>\d+)\s+max_fee=(?<max_fee>\d+)\s+nonce_ok=(?<nonce_ok>true|false)\s+sig_ok=(?<sig_ok>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        accounts = [int]$m.Groups["accounts"].Value
        txs = [int]$m.Groups["txs"].Value
        min_fee = [int64]$m.Groups["min_fee"].Value
        max_fee = [int64]$m.Groups["max_fee"].Value
        nonce_ok = [bool]::Parse($m.Groups["nonce_ok"].Value)
        sig_ok = [bool]::Parse($m.Groups["sig_ok"].Value)
        raw = $line
    }
}

function Parse-AdapterOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^adapter_out:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^adapter_out:\s+backend=(?<backend>[a-z0-9_]+)\s+chain=(?<chain>[a-z0-9_]+)\s+txs=(?<txs>\d+)\s+verified=(?<verified>true|false)\s+applied=(?<applied>true|false)\s+accounts=(?<accounts>\d+)\s+state_root=(?<state_root>[0-9a-f]+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        backend = $m.Groups["backend"].Value
        chain = $m.Groups["chain"].Value
        txs = [int]$m.Groups["txs"].Value
        verified = [bool]::Parse($m.Groups["verified"].Value)
        applied = [bool]::Parse($m.Groups["applied"].Value)
        accounts = [int]$m.Groups["accounts"].Value
        state_root = $m.Groups["state_root"].Value
        raw = $line
    }
}

function Parse-AdapterPluginAbiLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^adapter_plugin_abi:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^adapter_plugin_abi:\s+enabled=(?<enabled>true|false)\s+version=(?<version>\d+)\s+expected=(?<expected>\d+)\s+caps=(?<caps>0x[0-9a-f]+)\s+required=(?<required>0x[0-9a-f]+)\s+compatible=(?<compatible>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        enabled = [bool]::Parse($m.Groups["enabled"].Value)
        version = [int]$m.Groups["version"].Value
        expected = [int]$m.Groups["expected"].Value
        caps = $m.Groups["caps"].Value.ToLowerInvariant()
        required = $m.Groups["required"].Value.ToLowerInvariant()
        compatible = [bool]::Parse($m.Groups["compatible"].Value)
        raw = $line
    }
}

function Parse-AdapterPluginRegistryLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^adapter_plugin_registry:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^adapter_plugin_registry:\s+enabled=(?<enabled>true|false)\s+strict=(?<strict>true|false)\s+matched=(?<matched>true|false)\s+chain_allowed=(?<chain_allowed>true|false)\s+entry_abi=(?<entry_abi>\d+)\s+entry_required=(?<entry_required>0x[0-9a-f]+)\s+hash_check=(?<hash_check>true|false)\s+hash_match=(?<hash_match>true|false)\s+abi_whitelist=(?<abi_whitelist>true|false)\s+abi_allowed=(?<abi_allowed>true|false)$"
    )
    if (-not $m.Success) {
        $legacy = [regex]::Match(
            $line,
            "^adapter_plugin_registry:\s+enabled=(?<enabled>true|false)\s+strict=(?<strict>true|false)\s+matched=(?<matched>true|false)\s+chain_allowed=(?<chain_allowed>true|false)\s+entry_abi=(?<entry_abi>\d+)\s+entry_required=(?<entry_required>0x[0-9a-f]+)$"
        )
        if (-not $legacy.Success) {
            return [ordered]@{
                parse_ok = $false
                raw = $line
            }
        }
        return [ordered]@{
            parse_ok = $true
            enabled = [bool]::Parse($legacy.Groups["enabled"].Value)
            strict = [bool]::Parse($legacy.Groups["strict"].Value)
            matched = [bool]::Parse($legacy.Groups["matched"].Value)
            chain_allowed = [bool]::Parse($legacy.Groups["chain_allowed"].Value)
            entry_abi = [int]$legacy.Groups["entry_abi"].Value
            entry_required = $legacy.Groups["entry_required"].Value.ToLowerInvariant()
            hash_check = $false
            hash_match = $true
            abi_whitelist = $false
            abi_allowed = $true
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        enabled = [bool]::Parse($m.Groups["enabled"].Value)
        strict = [bool]::Parse($m.Groups["strict"].Value)
        matched = [bool]::Parse($m.Groups["matched"].Value)
        chain_allowed = [bool]::Parse($m.Groups["chain_allowed"].Value)
        entry_abi = [int]$m.Groups["entry_abi"].Value
        entry_required = $m.Groups["entry_required"].Value.ToLowerInvariant()
        hash_check = [bool]::Parse($m.Groups["hash_check"].Value)
        hash_match = [bool]::Parse($m.Groups["hash_match"].Value)
        abi_whitelist = [bool]::Parse($m.Groups["abi_whitelist"].Value)
        abi_allowed = [bool]::Parse($m.Groups["abi_allowed"].Value)
        raw = $line
    }
}

function Parse-AdapterConsensusLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^adapter_consensus:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^adapter_consensus:\s+plugin_class=(?<plugin_class>[a-z_]+)\s+plugin_class_code=(?<plugin_class_code>\d+)\s+consensus_adapter_hash=(?<consensus_adapter_hash>[0-9a-f]{64})\s+backend=(?<backend>[a-z_]+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        plugin_class = $m.Groups["plugin_class"].Value
        plugin_class_code = [int]$m.Groups["plugin_class_code"].Value
        consensus_adapter_hash = $m.Groups["consensus_adapter_hash"].Value.ToLowerInvariant()
        backend = $m.Groups["backend"].Value
        raw = $line
    }
}

function Normalize-HexMask {
    param([string]$Text)
    $raw = "$Text".Trim()
    if (-not $raw) {
        throw "empty hex mask"
    }
    if ($raw -match "^(0x|0X)[0-9a-fA-F]+$") {
        $value = [Convert]::ToUInt64($raw.Substring(2), 16)
        return ("0x{0}" -f $value.ToString("x"))
    }
    if ($raw -match "^\d+$") {
        $value = [UInt64]::Parse($raw)
        return ("0x{0}" -f $value.ToString("x"))
    }
    throw "invalid mask format: $Text"
}

function Normalize-Sha256Hex {
    param([string]$Text)
    $raw = "$Text".Trim()
    if (-not $raw) {
        throw "empty sha256"
    }
    if ($raw.StartsWith("0x") -or $raw.StartsWith("0X")) {
        $raw = $raw.Substring(2)
    }
    if ($raw -notmatch "^[0-9a-fA-F]{64}$") {
        throw "invalid sha256 hex (expected 64 hex chars): $Text"
    }
    return $raw.ToLowerInvariant()
}

function Parse-TxCodecLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^tx_codec:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^tx_codec:\s+codec=(?<codec>[a-z0-9_]+)\s+encoded=(?<encoded>\d+)\s+decoded=(?<decoded>\d+)\s+bytes=(?<bytes>\d+)\s+pass=(?<pass>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        codec = $m.Groups["codec"].Value
        encoded = [int]$m.Groups["encoded"].Value
        decoded = [int]$m.Groups["decoded"].Value
        bytes = [int]$m.Groups["bytes"].Value
        pass = [bool]::Parse($m.Groups["pass"].Value)
        raw = $line
    }
}

function Parse-MempoolOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^mempool_out:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^mempool_out:\s+policy=(?<policy>[a-z_]+)\s+accepted=(?<accepted>\d+)\s+rejected=(?<rejected>\d+)\s+fee_floor=(?<fee_floor>\d+)\s+nonce_ok=(?<nonce_ok>true|false)\s+sig_ok=(?<sig_ok>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        policy = $m.Groups["policy"].Value
        accepted = [int]$m.Groups["accepted"].Value
        rejected = [int]$m.Groups["rejected"].Value
        fee_floor = [int64]$m.Groups["fee_floor"].Value
        nonce_ok = [bool]::Parse($m.Groups["nonce_ok"].Value)
        sig_ok = [bool]::Parse($m.Groups["sig_ok"].Value)
        raw = $line
    }
}

function Parse-BatchALine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^batch_a:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^batch_a:\s+epoch=(?<epoch>\d+)\s+height=(?<height>\d+)\s+committed=(?<committed>true|false)\s+txs=(?<txs>\d+)\s+state_root=(?<state_root>[0-9a-f]+)\s+proposal_hash=(?<proposal_hash>[0-9a-f]+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        epoch = [int64]$m.Groups["epoch"].Value
        height = [int64]$m.Groups["height"].Value
        committed = [bool]::Parse($m.Groups["committed"].Value)
        txs = [int64]$m.Groups["txs"].Value
        state_root = $m.Groups["state_root"].Value
        proposal_hash = $m.Groups["proposal_hash"].Value
        raw = $line
    }
}

function Parse-BlockOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^block_out:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^block_out:\s+height=(?<height>\d+)\s+epoch=(?<epoch>\d+)\s+batches=(?<batches>\d+)\s+txs=(?<txs>\d+)\s+block_hash=(?<block_hash>[0-9a-f]+)\s+state_root=(?<state_root>[0-9a-f]+)\s+proposal_hash=(?<proposal_hash>[0-9a-f]+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        height = [int64]$m.Groups["height"].Value
        epoch = [int64]$m.Groups["epoch"].Value
        batches = [int64]$m.Groups["batches"].Value
        txs = [int64]$m.Groups["txs"].Value
        block_hash = $m.Groups["block_hash"].Value
        state_root = $m.Groups["state_root"].Value
        proposal_hash = $m.Groups["proposal_hash"].Value
        raw = $line
    }
}

function Parse-BlockWireLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^block_wire:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^block_wire:\s+codec=(?<codec>[a-z0-9_]+)\s+bytes=(?<bytes>\d+)\s+pass=(?<pass>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        codec = $m.Groups["codec"].Value
        bytes = [int]$m.Groups["bytes"].Value
        pass = [bool]::Parse($m.Groups["pass"].Value)
        raw = $line
    }
}

function Parse-CommitOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^commit_out:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^commit_out:\s+store=(?<store>\w+)\s+committed=(?<committed>true|false)\s+height=(?<height>\d+)\s+total_blocks=(?<total_blocks>\d+)\s+block_hash=(?<block_hash>[0-9a-f]+)\s+state_root=(?<state_root>[0-9a-f]+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        store = $m.Groups["store"].Value
        committed = [bool]::Parse($m.Groups["committed"].Value)
        height = [int64]$m.Groups["height"].Value
        total_blocks = [int64]$m.Groups["total_blocks"].Value
        block_hash = $m.Groups["block_hash"].Value
        state_root = $m.Groups["state_root"].Value
        raw = $line
    }
}

function Parse-NetworkOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^network_out:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^network_out:\s+transport=(?<transport>\w+)\s+from=(?<from>\d+)\s+to=(?<to>\d+)\s+sent=(?<sent>\d+)\s+received=(?<received>\d+)\s+msg_kind=(?<msg_kind>[a-z_]+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        transport = $m.Groups["transport"].Value
        from = [int64]$m.Groups["from"].Value
        to = [int64]$m.Groups["to"].Value
        sent = [int64]$m.Groups["sent"].Value
        received = [int64]$m.Groups["received"].Value
        msg_kind = $m.Groups["msg_kind"].Value
        raw = $line
    }
}

function Parse-NetworkClosureLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^network_closure:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^network_closure:\s+nodes=(?<nodes>\d+)\s+discovery=(?<discovery>true|false)\s+gossip=(?<gossip>true|false)\s+sync=(?<sync>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        nodes = [int64]$m.Groups["nodes"].Value
        discovery = [bool]::Parse($m.Groups["discovery"].Value)
        gossip = [bool]::Parse($m.Groups["gossip"].Value)
        sync = [bool]::Parse($m.Groups["sync"].Value)
        raw = $line
    }
}

function Parse-ConsistencyDigestLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^consistency:" } | Select-Object -Last 1)
    if (-not $line) {
        throw "ffi_consistency_digest output missing digest line"
    }
    $m = [regex]::Match(
        $line,
        "^consistency: rounds=(?<rounds>\d+) points=(?<points>\d+) key_space=(?<key_space>\d+) rw=(?<rw>[0-9.]+) seed=(?<seed>\d+) digest=(?<digest>[0-9a-f]+) total_processed=(?<total_processed>\d+) total_success=(?<total_success>\d+) total_writes=(?<total_writes>\d+)$"
    )
    if (-not $m.Success) {
        throw "cannot parse consistency digest line: $line"
    }
    return [ordered]@{
        rounds          = [int]$m.Groups["rounds"].Value
        points          = [int]$m.Groups["points"].Value
        key_space       = [int64]$m.Groups["key_space"].Value
        rw              = [double]$m.Groups["rw"].Value
        seed            = [int64]$m.Groups["seed"].Value
        digest          = $m.Groups["digest"].Value
        total_processed = [int64]$m.Groups["total_processed"].Value
        total_success   = [int64]$m.Groups["total_success"].Value
        total_writes    = [int64]$m.Groups["total_writes"].Value
    }
}

function Get-DllPathForVariant {
    param([string]$AoemRoot, [string]$Variant)
    switch ($Variant) {
        "core" { return Join-Path $AoemRoot "bin\aoem_ffi.dll" }
        "persist" { return Join-Path $AoemRoot "variants\persist\bin\aoem_ffi.dll" }
        "wasm" { return Join-Path $AoemRoot "variants\wasm\bin\aoem_ffi.dll" }
        default { throw "invalid variant: $Variant" }
    }
}

function Get-CapabilitySnapshot {
    param(
        [string]$RepoRoot,
        [string]$Variant,
        [string]$CapabilityJson
    )

    $sourceJson = $CapabilityJson
    if (-not $sourceJson) {
        $scriptPath = Join-Path $RepoRoot "scripts\migration\dump_capability_contract.ps1"
        if (-not (Test-Path $scriptPath)) {
            throw "missing capability dump script: $scriptPath"
        }

        $capOutputDir = Join-Path $RepoRoot "artifacts\migration\capabilities"
        New-Item -ItemType Directory -Force -Path $capOutputDir | Out-Null

        & powershell -ExecutionPolicy Bypass -File $scriptPath -RepoRoot $RepoRoot -OutputDir $capOutputDir -Variant $Variant | Out-Null
        $sourceJson = Join-Path $capOutputDir "capability-contract-$Variant.json"
    }

    if (-not (Test-Path $sourceJson)) {
        throw "capability json not found: $sourceJson"
    }

    $raw = Get-Content -Path $sourceJson -Raw | ConvertFrom-Json
    if (-not $raw.contract) {
        throw "invalid capability json (missing contract): $sourceJson"
    }

    $fallbackCodes = @()
    if ($null -ne $raw.contract.fallback_reason_codes) {
        $fallbackCodes = @($raw.contract.fallback_reason_codes)
    }

    return [ordered]@{
        source_json = $sourceJson
        generated_at_utc = [string]$raw.generated_at_utc
        variant = [string]$raw.variant
        execute_ops_v2 = [bool]$raw.contract.execute_ops_v2
        zkvm_prove = [bool]$raw.contract.zkvm_prove
        zkvm_verify = [bool]$raw.contract.zkvm_verify
        msm_accel = [bool]$raw.contract.msm_accel
        msm_backend = [string]$raw.contract.msm_backend
        fallback_reason_codes = $fallbackCodes
        inferred_from_legacy_fields = [bool]$raw.contract.inferred_from_legacy_fields
    }
}

function Get-NetworkProcessSignal {
    param(
        [string]$RepoRoot,
        [string]$NetworkProcessJson,
        [int]$NodeCount,
        [int]$ProbeRounds
    )

    $sourceJson = $NetworkProcessJson
    if (-not $sourceJson) {
        $scriptPath = Join-Path $RepoRoot "scripts\migration\run_network_two_process.ps1"
        if (-not (Test-Path $scriptPath)) {
        return [ordered]@{
            available = $false
            pass = $false
            source_json = $null
            rounds = $ProbeRounds
            rounds_passed = 0
            round_pass_ratio = 0.0
            block_wire_available = $false
            block_wire_pass = $false
            block_wire_rounds_passed = 0
            block_wire_pass_ratio = 0.0
            block_wire_verified = 0
            block_wire_total = 0
            block_wire_verified_ratio = 0.0
            reason = "missing script: $scriptPath"
        }
    }

        $probeDir = Join-Path $RepoRoot "artifacts\migration\network-two-process"
        New-Item -ItemType Directory -Force -Path $probeDir | Out-Null
        & powershell -ExecutionPolicy Bypass -File $scriptPath -RepoRoot $RepoRoot -OutputDir $probeDir -NodeCount $NodeCount -Rounds $ProbeRounds | Out-Null
        $sourceJson = Join-Path $probeDir "network-two-process.json"
    }

    if (-not (Test-Path $sourceJson)) {
        return [ordered]@{
            available = $false
            pass = $false
            source_json = $sourceJson
            rounds = $ProbeRounds
            rounds_passed = 0
            round_pass_ratio = 0.0
            block_wire_available = $false
            block_wire_pass = $false
            block_wire_rounds_passed = 0
            block_wire_pass_ratio = 0.0
            block_wire_verified = 0
            block_wire_total = 0
            block_wire_verified_ratio = 0.0
            reason = "network two-process json not found"
        }
    }

    $raw = Get-Content -Path $sourceJson -Raw | ConvertFrom-Json
    return [ordered]@{
        available = $true
        pass = [bool]$raw.pass
        source_json = $sourceJson
        mode = [string]$raw.mode
        rounds = if ($null -ne $raw.rounds) { [int]$raw.rounds } else { 1 }
        rounds_passed = if ($null -ne $raw.rounds_passed) { [int]$raw.rounds_passed } else { if ([bool]$raw.pass) { 1 } else { 0 } }
        round_pass_ratio = if ($null -ne $raw.round_pass_ratio) { [double]$raw.round_pass_ratio } else { if ([bool]$raw.pass) { 1.0 } else { 0.0 } }
        node_count = [int]$raw.node_count
        total_pairs = [int]$raw.total_pairs
        passed_pairs = [int]$raw.passed_pairs
        pair_pass_ratio = [double]$raw.pair_pass_ratio
        directed_edges_total = if ($null -ne $raw.directed_edges_total) { [int]$raw.directed_edges_total } else { 0 }
        directed_edges_up = if ($null -ne $raw.directed_edges_up) { [int]$raw.directed_edges_up } else { 0 }
        directed_edge_ratio = if ($null -ne $raw.directed_edge_ratio) { [double]$raw.directed_edge_ratio } else { 0.0 }
        block_wire_available = if ($null -ne $raw.block_wire_available) { [bool]$raw.block_wire_available } else { $false }
        block_wire_pass = if ($null -ne $raw.block_wire_pass) { [bool]$raw.block_wire_pass } else { $false }
        block_wire_rounds_passed = if ($null -ne $raw.block_wire_rounds_passed) { [int]$raw.block_wire_rounds_passed } else { 0 }
        block_wire_pass_ratio = if ($null -ne $raw.block_wire_pass_ratio) { [double]$raw.block_wire_pass_ratio } else { 0.0 }
        block_wire_verified = if ($null -ne $raw.block_wire_verified) { [int]$raw.block_wire_verified } else { 0 }
        block_wire_total = if ($null -ne $raw.block_wire_total) { [int]$raw.block_wire_total } else { 0 }
        block_wire_verified_ratio = if ($null -ne $raw.block_wire_verified_ratio) { [double]$raw.block_wire_verified_ratio } else { 0.0 }
        node_a_exit_code = if ($raw.node_a -and $null -ne $raw.node_a.exit_code) { [int]$raw.node_a.exit_code } else { $null }
        node_b_exit_code = if ($raw.node_b -and $null -ne $raw.node_b.exit_code) { [int]$raw.node_b.exit_code } else { $null }
        reason = $null
    }
}

function Get-NetworkBlockWireNegativeSignal {
    param(
        [string]$RepoRoot
    )

    $scriptPath = Join-Path $RepoRoot "scripts\migration\run_network_two_process.ps1"
    if (-not (Test-Path $scriptPath)) {
        return [ordered]@{
            enabled = $true
            available = $false
            pass = $false
            source_json = $null
            tamper_mode = "hash_mismatch"
            expected_fail = $false
            reason_match = $false
            block_wire_pass = $null
            block_wire_verified = 0
            block_wire_total = 0
            reason = "missing script: $scriptPath"
        }
    }

    $probeDir = Join-Path $RepoRoot "artifacts\migration\network-two-process-negative-block-wire"
    New-Item -ItemType Directory -Force -Path $probeDir | Out-Null
    & powershell -ExecutionPolicy Bypass -File $scriptPath `
        -RepoRoot $RepoRoot `
        -OutputDir $probeDir `
        -NodeCount 2 `
        -Rounds 1 `
        -ProbeMode "mesh" `
        -TamperBlockWireMode "hash_mismatch" | Out-Null

    $sourceJson = Join-Path $probeDir "network-two-process.json"
    if (-not (Test-Path $sourceJson)) {
        return [ordered]@{
            enabled = $true
            available = $false
            pass = $false
            source_json = $sourceJson
            tamper_mode = "hash_mismatch"
            expected_fail = $false
            reason_match = $false
            block_wire_pass = $null
            block_wire_verified = 0
            block_wire_total = 0
            reason = "negative network probe json not found"
        }
    }

    $raw = Get-Content -Path $sourceJson -Raw | ConvertFrom-Json
    $expectedFail = -not [bool]$raw.pass
    $blockWirePass = if ($null -ne $raw.block_wire_pass) { [bool]$raw.block_wire_pass } else { $false }
    $blockWireVerified = if ($null -ne $raw.block_wire_verified) { [int]$raw.block_wire_verified } else { 0 }
    $blockWireTotal = if ($null -ne $raw.block_wire_total) { [int]$raw.block_wire_total } else { 0 }
    $reasonMatch = ((-not $blockWirePass) -and $blockWireVerified -lt $blockWireTotal)

    return [ordered]@{
        enabled = $true
        available = $true
        pass = ($expectedFail -and $reasonMatch)
        source_json = $sourceJson
        tamper_mode = if ($null -ne $raw.tamper_block_wire_mode) { [string]$raw.tamper_block_wire_mode } else { "hash_mismatch" }
        expected_fail = $expectedFail
        reason_match = $reasonMatch
        block_wire_pass = $blockWirePass
        block_wire_verified = $blockWireVerified
        block_wire_total = $blockWireTotal
        reason = $null
    }
}

$aoemRoot = Join-Path $RepoRoot "aoem"
$nodeDir = Join-Path $RepoRoot "crates\novovm-node"
$bindingsDir = Join-Path $RepoRoot "crates\aoem-bindings"
$adapterPluginRequiredCapsNormalized = Normalize-HexMask -Text $AdapterPluginRequiredCaps
$adapterPluginRegistryPathResolved = ""
if ($AdapterPluginRegistryPath) {
    $adapterPluginRegistryPathResolved = $AdapterPluginRegistryPath
} else {
    $defaultRegistryPath = Join-Path $RepoRoot "config\novovm-adapter-plugin-registry.json"
    if (Test-Path $defaultRegistryPath) {
        $adapterPluginRegistryPathResolved = $defaultRegistryPath
    }
}
$adapterPluginRegistryExpectedEnabled = [bool]($adapterPluginRegistryPathResolved)
$adapterPluginRegistryStrictFlag = if ($AdapterPluginRegistryStrict) { "1" } else { "0" }
$adapterPluginRegistrySha256Normalized = ""
if ($AdapterPluginRegistrySha256) {
    $adapterPluginRegistrySha256Normalized = Normalize-Sha256Hex -Text $AdapterPluginRegistrySha256
}
$adapterPluginRegistryExpectedHashCheck = [bool]($adapterPluginRegistrySha256Normalized)

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$nodeFfiText = Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run") -EnvVars @{
    NOVOVM_EXEC_PATH = "ffi_v2"
    NOVOVM_AOEM_VARIANT = "core"
    NOVOVM_DEMO_TXS = "$BatchADemoTxs"
    NOVOVM_BATCH_A_BATCHES = "$BatchABatchCount"
    NOVOVM_MEMPOOL_FEE_FLOOR = "$BatchAMempoolFeeFloor"
    NOVOVM_ADAPTER_BACKEND = "$AdapterBackend"
    NOVOVM_ADAPTER_PLUGIN_PATH = "$AdapterPluginPath"
    NOVOVM_ADAPTER_CHAIN = "$AdapterExpectedChain"
    NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI = "$AdapterPluginExpectedAbi"
    NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS = "$adapterPluginRequiredCapsNormalized"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH = "$adapterPluginRegistryPathResolved"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "$adapterPluginRegistryStrictFlag"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256 = "$adapterPluginRegistrySha256Normalized"
}
$nodeLegacyText = Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run") -EnvVars @{
    NOVOVM_EXEC_PATH = "legacy"
    NOVOVM_AOEM_VARIANT = "core"
    NOVOVM_DEMO_TXS = "$BatchADemoTxs"
    NOVOVM_BATCH_A_BATCHES = "$BatchABatchCount"
    NOVOVM_MEMPOOL_FEE_FLOOR = "$BatchAMempoolFeeFloor"
    NOVOVM_ADAPTER_BACKEND = "$AdapterBackend"
    NOVOVM_ADAPTER_PLUGIN_PATH = "$AdapterPluginPath"
    NOVOVM_ADAPTER_CHAIN = "$AdapterExpectedChain"
    NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI = "$AdapterPluginExpectedAbi"
    NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS = "$adapterPluginRequiredCapsNormalized"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH = "$adapterPluginRegistryPathResolved"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "$adapterPluginRegistryStrictFlag"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256 = "$adapterPluginRegistrySha256Normalized"
}

$nodeFfi = Parse-NodeReportLine -Text $nodeFfiText
$nodeLegacy = Parse-NodeReportLine -Text $nodeLegacyText
$txCodecFfi = Parse-TxCodecLine -Text $nodeFfiText
$txCodecLegacy = Parse-TxCodecLine -Text $nodeLegacyText
$mempoolFfi = Parse-MempoolOutLine -Text $nodeFfiText
$mempoolLegacy = Parse-MempoolOutLine -Text $nodeLegacyText
$txMetaFfi = Parse-TxMetaLine -Text $nodeFfiText
$txMetaLegacy = Parse-TxMetaLine -Text $nodeLegacyText
$adapterFfi = Parse-AdapterOutLine -Text $nodeFfiText
$adapterLegacy = Parse-AdapterOutLine -Text $nodeLegacyText
$adapterPluginAbiFfi = Parse-AdapterPluginAbiLine -Text $nodeFfiText
$adapterPluginAbiLegacy = Parse-AdapterPluginAbiLine -Text $nodeLegacyText
$adapterPluginRegistryFfi = Parse-AdapterPluginRegistryLine -Text $nodeFfiText
$adapterPluginRegistryLegacy = Parse-AdapterPluginRegistryLine -Text $nodeLegacyText
$adapterConsensusFfi = Parse-AdapterConsensusLine -Text $nodeFfiText
$adapterConsensusLegacy = Parse-AdapterConsensusLine -Text $nodeLegacyText
$batchAffi = Parse-BatchALine -Text $nodeFfiText
$batchAlegacy = Parse-BatchALine -Text $nodeLegacyText
$blockOutFfi = Parse-BlockOutLine -Text $nodeFfiText
$blockOutLegacy = Parse-BlockOutLine -Text $nodeLegacyText
$blockWireFfi = Parse-BlockWireLine -Text $nodeFfiText
$blockWireLegacy = Parse-BlockWireLine -Text $nodeLegacyText
$commitOutFfi = Parse-CommitOutLine -Text $nodeFfiText
$commitOutLegacy = Parse-CommitOutLine -Text $nodeLegacyText
$networkOutFfi = Parse-NetworkOutLine -Text $nodeFfiText
$networkOutLegacy = Parse-NetworkOutLine -Text $nodeLegacyText
$networkClosureFfi = Parse-NetworkClosureLine -Text $nodeFfiText
$networkClosureLegacy = Parse-NetworkClosureLine -Text $nodeLegacyText

$expectedBatchMin = [Math]::Min($BatchABatchCount, $BatchADemoTxs)

$nodeCompatPass = (
    $nodeFfi.rc -eq 0 -and
    $nodeLegacy.rc -eq 0 -and
    $nodeFfi.processed -eq $nodeLegacy.processed -and
    $nodeFfi.success -eq $nodeLegacy.success -and
    $nodeFfi.writes -eq $nodeLegacy.writes
)

$txCodecAvailable = ($null -ne $txCodecFfi -and $null -ne $txCodecLegacy)
$txCodecPass = $false
if ($txCodecAvailable) {
    $txCodecPass = (
        $txCodecFfi.parse_ok -and
        $txCodecLegacy.parse_ok -and
        $txCodecFfi.codec -eq $txCodecLegacy.codec -and
        $txCodecFfi.pass -and
        $txCodecLegacy.pass -and
        $txCodecFfi.encoded -eq $txCodecLegacy.encoded -and
        $txCodecFfi.decoded -eq $txCodecLegacy.decoded -and
        $txCodecFfi.decoded -eq $BatchADemoTxs -and
        $txCodecFfi.bytes -eq $txCodecLegacy.bytes
    )
}

$mempoolAvailable = ($null -ne $mempoolFfi -and $null -ne $mempoolLegacy)
$mempoolPass = $false
if ($mempoolAvailable) {
    $mempoolPass = (
        $mempoolFfi.parse_ok -and
        $mempoolLegacy.parse_ok -and
        $mempoolFfi.policy -eq $mempoolLegacy.policy -and
        $mempoolFfi.accepted -eq $mempoolLegacy.accepted -and
        $mempoolFfi.accepted -eq $BatchADemoTxs -and
        $mempoolFfi.rejected -eq $mempoolLegacy.rejected -and
        $mempoolFfi.rejected -eq 0 -and
        $mempoolFfi.fee_floor -eq $mempoolLegacy.fee_floor -and
        $mempoolFfi.fee_floor -eq $BatchAMempoolFeeFloor -and
        $mempoolFfi.nonce_ok -and
        $mempoolLegacy.nonce_ok -and
        $mempoolFfi.sig_ok -and
        $mempoolLegacy.sig_ok
    )
}

$txMetaAvailable = ($null -ne $txMetaFfi -and $null -ne $txMetaLegacy)
$txMetaPass = $false
if ($txMetaAvailable) {
    $txMetaPass = (
        $txMetaFfi.parse_ok -and
        $txMetaLegacy.parse_ok -and
        $txMetaFfi.accounts -eq $txMetaLegacy.accounts -and
        $txMetaFfi.txs -eq $txMetaLegacy.txs -and
        $txMetaFfi.txs -eq $BatchADemoTxs -and
        $txMetaFfi.min_fee -eq $txMetaLegacy.min_fee -and
        $txMetaFfi.max_fee -eq $txMetaLegacy.max_fee -and
        $txMetaFfi.nonce_ok -and
        $txMetaLegacy.nonce_ok -and
        $txMetaFfi.sig_ok -and
        $txMetaLegacy.sig_ok
    )
}

$adapterAvailable = ($null -ne $adapterFfi -and $null -ne $adapterLegacy)
$adapterPass = $false
if ($adapterAvailable) {
    $adapterExpected = $AdapterExpectedChain.ToLowerInvariant()
    $adapterExpectedBackend = $AdapterExpectedBackend.ToLowerInvariant()
    $backendMatchExpected = $true
    if ($adapterExpectedBackend -ne "auto") {
        $backendMatchExpected = ($adapterFfi.backend -eq $adapterExpectedBackend)
    }
    $adapterPass = (
        $adapterFfi.parse_ok -and
        $adapterLegacy.parse_ok -and
        $adapterFfi.backend -eq $adapterLegacy.backend -and
        $backendMatchExpected -and
        $adapterFfi.chain -eq $adapterLegacy.chain -and
        $adapterFfi.chain -eq $adapterExpected -and
        $adapterFfi.txs -eq $adapterLegacy.txs -and
        $adapterFfi.txs -eq $BatchADemoTxs -and
        $adapterFfi.verified -and
        $adapterLegacy.verified -and
        $adapterFfi.applied -and
        $adapterLegacy.applied -and
        $adapterFfi.accounts -eq $adapterLegacy.accounts -and
        $adapterFfi.state_root -eq $adapterLegacy.state_root
    )
}

$adapterPluginAbiAvailable = ($null -ne $adapterPluginAbiFfi -and $null -ne $adapterPluginAbiLegacy)
$adapterPluginAbiPass = $false
if ($adapterPluginAbiAvailable) {
    $adapterExpectedBackend = $AdapterExpectedBackend.ToLowerInvariant()
    $expectEnabled = $null
    if ($adapterExpectedBackend -eq "plugin") {
        $expectEnabled = $true
    } elseif ($adapterExpectedBackend -eq "native") {
        $expectEnabled = $false
    }
    $enabledMatchExpected = $true
    if ($null -ne $expectEnabled) {
        $enabledMatchExpected = ($adapterPluginAbiFfi.enabled -eq $expectEnabled)
    }

    $adapterPluginAbiPass = (
        $adapterPluginAbiFfi.parse_ok -and
        $adapterPluginAbiLegacy.parse_ok -and
        $adapterPluginAbiFfi.enabled -eq $adapterPluginAbiLegacy.enabled -and
        $enabledMatchExpected -and
        $adapterPluginAbiFfi.version -eq $adapterPluginAbiLegacy.version -and
        $adapterPluginAbiFfi.expected -eq $adapterPluginAbiLegacy.expected -and
        $adapterPluginAbiFfi.expected -eq $AdapterPluginExpectedAbi -and
        $adapterPluginAbiFfi.caps -eq $adapterPluginAbiLegacy.caps -and
        $adapterPluginAbiFfi.required -eq $adapterPluginAbiLegacy.required -and
        $adapterPluginAbiFfi.required -eq $adapterPluginRequiredCapsNormalized -and
        $adapterPluginAbiFfi.compatible -and
        $adapterPluginAbiLegacy.compatible
    )
}

$adapterPluginRegistryAvailable = ($null -ne $adapterPluginRegistryFfi -and $null -ne $adapterPluginRegistryLegacy)
$adapterPluginRegistryPass = $false
if ($adapterPluginRegistryAvailable) {
    $expectedStrict = [bool]$AdapterPluginRegistryStrict
    $enabledMatchExpected = ($adapterPluginRegistryFfi.enabled -eq $adapterPluginRegistryExpectedEnabled)
    $strictMatchExpected = ($adapterPluginRegistryFfi.strict -eq $expectedStrict)
    $hashCheckMatchExpected = ($adapterPluginRegistryFfi.hash_check -eq $adapterPluginRegistryExpectedHashCheck)
    $matchedRequirement = $true
    if ($adapterPluginRegistryExpectedEnabled) {
        $matchedRequirement = ($adapterPluginRegistryFfi.matched -and $adapterPluginRegistryFfi.chain_allowed)
    }
    $hashRequirement = (-not $adapterPluginRegistryExpectedHashCheck) -or ($adapterPluginRegistryFfi.hash_check -and $adapterPluginRegistryFfi.hash_match)
    $whitelistRequirement = (-not $adapterPluginRegistryFfi.abi_whitelist) -or $adapterPluginRegistryFfi.abi_allowed
    $adapterPluginRegistryPass = (
        $adapterPluginRegistryFfi.parse_ok -and
        $adapterPluginRegistryLegacy.parse_ok -and
        $adapterPluginRegistryFfi.enabled -eq $adapterPluginRegistryLegacy.enabled -and
        $adapterPluginRegistryFfi.strict -eq $adapterPluginRegistryLegacy.strict -and
        $enabledMatchExpected -and
        $strictMatchExpected -and
        $adapterPluginRegistryFfi.matched -eq $adapterPluginRegistryLegacy.matched -and
        $adapterPluginRegistryFfi.chain_allowed -eq $adapterPluginRegistryLegacy.chain_allowed -and
        $matchedRequirement -and
        $adapterPluginRegistryFfi.entry_abi -eq $adapterPluginRegistryLegacy.entry_abi -and
        $adapterPluginRegistryFfi.entry_required -eq $adapterPluginRegistryLegacy.entry_required -and
        $adapterPluginRegistryFfi.entry_abi -eq $AdapterPluginExpectedAbi -and
        $adapterPluginRegistryFfi.entry_required -eq $adapterPluginRequiredCapsNormalized -and
        $adapterPluginRegistryFfi.hash_check -eq $adapterPluginRegistryLegacy.hash_check -and
        $adapterPluginRegistryFfi.hash_match -eq $adapterPluginRegistryLegacy.hash_match -and
        $adapterPluginRegistryFfi.abi_whitelist -eq $adapterPluginRegistryLegacy.abi_whitelist -and
        $adapterPluginRegistryFfi.abi_allowed -eq $adapterPluginRegistryLegacy.abi_allowed -and
        $hashCheckMatchExpected -and
        $hashRequirement -and
        $whitelistRequirement
    )
}

$adapterConsensusAvailable = ($null -ne $adapterConsensusFfi -and $null -ne $adapterConsensusLegacy)
$adapterConsensusPass = $false
if ($adapterConsensusAvailable) {
    $expectedBackend = $AdapterExpectedBackend.ToLowerInvariant()
    $backendMatchExpected = $true
    if ($expectedBackend -eq "native" -or $expectedBackend -eq "plugin") {
        $backendMatchExpected = ($adapterConsensusFfi.backend -eq $expectedBackend)
    }
    $adapterConsensusPass = (
        $adapterConsensusFfi.parse_ok -and
        $adapterConsensusLegacy.parse_ok -and
        $adapterConsensusFfi.plugin_class -eq $adapterConsensusLegacy.plugin_class -and
        $adapterConsensusFfi.plugin_class -eq "consensus" -and
        $adapterConsensusFfi.plugin_class_code -eq $adapterConsensusLegacy.plugin_class_code -and
        $adapterConsensusFfi.plugin_class_code -eq 1 -and
        $adapterConsensusFfi.consensus_adapter_hash -eq $adapterConsensusLegacy.consensus_adapter_hash -and
        $adapterConsensusFfi.backend -eq $adapterConsensusLegacy.backend -and
        $backendMatchExpected
    )
}

$adapterComparePluginPath = if ($AdapterComparePluginPath) { $AdapterComparePluginPath } else { $AdapterPluginPath }
$adapterBackendCompareSignal = [ordered]@{
    enabled = $IncludeAdapterBackendCompare
    available = $false
    pass = $false
    compared_mode = "ffi_v2"
    expected_chain = $AdapterExpectedChain.ToLowerInvariant()
    plugin_path = $adapterComparePluginPath
    state_root_equal = $false
    native = $null
    plugin = $null
    reason = "disabled"
}
if ($IncludeAdapterBackendCompare) {
    if (-not $adapterComparePluginPath) {
        $adapterBackendCompareSignal.reason = "adapter backend compare requires plugin path"
    } elseif (-not (Test-Path $adapterComparePluginPath)) {
        $adapterBackendCompareSignal.reason = "adapter compare plugin path not found: $adapterComparePluginPath"
    } else {
        try {
            $compareNativeText = Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run") -EnvVars @{
                NOVOVM_EXEC_PATH = "ffi_v2"
                NOVOVM_AOEM_VARIANT = "core"
                NOVOVM_DEMO_TXS = "$BatchADemoTxs"
                NOVOVM_BATCH_A_BATCHES = "$BatchABatchCount"
                NOVOVM_MEMPOOL_FEE_FLOOR = "$BatchAMempoolFeeFloor"
                NOVOVM_ADAPTER_BACKEND = "native"
                NOVOVM_ADAPTER_PLUGIN_PATH = ""
                NOVOVM_ADAPTER_CHAIN = "$AdapterExpectedChain"
                NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI = "$AdapterPluginExpectedAbi"
                NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS = "$adapterPluginRequiredCapsNormalized"
                NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH = "$adapterPluginRegistryPathResolved"
                NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "$adapterPluginRegistryStrictFlag"
                NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256 = "$adapterPluginRegistrySha256Normalized"
            }
            $comparePluginText = Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run") -EnvVars @{
                NOVOVM_EXEC_PATH = "ffi_v2"
                NOVOVM_AOEM_VARIANT = "core"
                NOVOVM_DEMO_TXS = "$BatchADemoTxs"
                NOVOVM_BATCH_A_BATCHES = "$BatchABatchCount"
                NOVOVM_MEMPOOL_FEE_FLOOR = "$BatchAMempoolFeeFloor"
                NOVOVM_ADAPTER_BACKEND = "plugin"
                NOVOVM_ADAPTER_PLUGIN_PATH = "$adapterComparePluginPath"
                NOVOVM_ADAPTER_CHAIN = "$AdapterExpectedChain"
                NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI = "$AdapterPluginExpectedAbi"
                NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS = "$adapterPluginRequiredCapsNormalized"
                NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH = "$adapterPluginRegistryPathResolved"
                NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "$adapterPluginRegistryStrictFlag"
                NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256 = "$adapterPluginRegistrySha256Normalized"
            }

            $compareNativeNode = Parse-NodeReportLine -Text $compareNativeText
            $comparePluginNode = Parse-NodeReportLine -Text $comparePluginText
            $compareNativeAdapter = Parse-AdapterOutLine -Text $compareNativeText
            $comparePluginAdapter = Parse-AdapterOutLine -Text $comparePluginText
            $compareNativePluginAbi = Parse-AdapterPluginAbiLine -Text $compareNativeText
            $comparePluginPluginAbi = Parse-AdapterPluginAbiLine -Text $comparePluginText
            $compareNativeRegistry = Parse-AdapterPluginRegistryLine -Text $compareNativeText
            $comparePluginRegistry = Parse-AdapterPluginRegistryLine -Text $comparePluginText
            $compareAvailable = (
                $null -ne $compareNativeAdapter -and
                $null -ne $comparePluginAdapter -and
                $null -ne $compareNativePluginAbi -and
                $null -ne $comparePluginPluginAbi -and
                $null -ne $compareNativeRegistry -and
                $null -ne $comparePluginRegistry
            )
            $compareStateRootEqual = $false
            $comparePass = $false
            if ($compareAvailable) {
                $compareStateRootEqual = ($compareNativeAdapter.state_root -eq $comparePluginAdapter.state_root)
                $expectedChain = $AdapterExpectedChain.ToLowerInvariant()
                $comparePass = (
                    $compareNativeNode.rc -eq 0 -and
                    $comparePluginNode.rc -eq 0 -and
                    $compareNativeAdapter.parse_ok -and
                    $comparePluginAdapter.parse_ok -and
                    $compareNativeAdapter.backend -eq "native" -and
                    $comparePluginAdapter.backend -eq "plugin" -and
                    $compareNativeAdapter.chain -eq $expectedChain -and
                    $comparePluginAdapter.chain -eq $expectedChain -and
                    $compareNativeAdapter.txs -eq $comparePluginAdapter.txs -and
                    $compareNativeAdapter.txs -eq $BatchADemoTxs -and
                    $compareNativeAdapter.accounts -eq $comparePluginAdapter.accounts -and
                    $compareNativeAdapter.verified -and
                    $comparePluginAdapter.verified -and
                    $compareNativeAdapter.applied -and
                    $comparePluginAdapter.applied -and
                    $compareNativePluginAbi.parse_ok -and
                    $comparePluginPluginAbi.parse_ok -and
                    (-not $compareNativePluginAbi.enabled) -and
                    $comparePluginPluginAbi.enabled -and
                    $compareNativePluginAbi.expected -eq $AdapterPluginExpectedAbi -and
                    $comparePluginPluginAbi.expected -eq $AdapterPluginExpectedAbi -and
                    $compareNativePluginAbi.required -eq $adapterPluginRequiredCapsNormalized -and
                    $comparePluginPluginAbi.required -eq $adapterPluginRequiredCapsNormalized -and
                    $compareNativePluginAbi.compatible -and
                    $comparePluginPluginAbi.compatible -and
                    $compareNativeRegistry.parse_ok -and
                    $comparePluginRegistry.parse_ok -and
                    $compareNativeRegistry.entry_abi -eq $AdapterPluginExpectedAbi -and
                    $comparePluginRegistry.entry_abi -eq $AdapterPluginExpectedAbi -and
                    $compareNativeRegistry.entry_required -eq $adapterPluginRequiredCapsNormalized -and
                    $comparePluginRegistry.entry_required -eq $adapterPluginRequiredCapsNormalized -and
                    $compareNativeRegistry.strict -eq [bool]$AdapterPluginRegistryStrict -and
                    $comparePluginRegistry.strict -eq [bool]$AdapterPluginRegistryStrict -and
                    $compareNativeRegistry.enabled -eq $adapterPluginRegistryExpectedEnabled -and
                    $comparePluginRegistry.enabled -eq $adapterPluginRegistryExpectedEnabled -and
                    $compareNativeRegistry.hash_check -eq $adapterPluginRegistryExpectedHashCheck -and
                    $comparePluginRegistry.hash_check -eq $adapterPluginRegistryExpectedHashCheck -and
                    $compareNativeRegistry.hash_check -eq $comparePluginRegistry.hash_check -and
                    $compareNativeRegistry.hash_match -eq $comparePluginRegistry.hash_match -and
                    ((-not $adapterPluginRegistryExpectedHashCheck) -or ($compareNativeRegistry.hash_match -and $comparePluginRegistry.hash_match)) -and
                    $compareNativeRegistry.abi_whitelist -eq $comparePluginRegistry.abi_whitelist -and
                    $compareNativeRegistry.abi_allowed -eq $comparePluginRegistry.abi_allowed -and
                    ((-not $compareNativeRegistry.abi_whitelist) -or ($compareNativeRegistry.abi_allowed -and $comparePluginRegistry.abi_allowed)) -and
                    ((-not $adapterPluginRegistryExpectedEnabled) -or ($compareNativeRegistry.matched -and $comparePluginRegistry.matched)) -and
                    ((-not $adapterPluginRegistryExpectedEnabled) -or ($compareNativeRegistry.chain_allowed -and $comparePluginRegistry.chain_allowed)) -and
                    $compareStateRootEqual -and
                    $compareNativeNode.processed -eq $comparePluginNode.processed -and
                    $compareNativeNode.success -eq $comparePluginNode.success -and
                    $compareNativeNode.writes -eq $comparePluginNode.writes
                )
            }

            $adapterBackendCompareSignal = [ordered]@{
                enabled = $true
                available = $compareAvailable
                pass = $comparePass
                compared_mode = "ffi_v2"
                expected_chain = $AdapterExpectedChain.ToLowerInvariant()
                plugin_path = $adapterComparePluginPath
                state_root_equal = $compareStateRootEqual
                native = [ordered]@{
                    node = $compareNativeNode
                    adapter = $compareNativeAdapter
                    plugin_abi = $compareNativePluginAbi
                    registry = $compareNativeRegistry
                }
                plugin = [ordered]@{
                    node = $comparePluginNode
                    adapter = $comparePluginAdapter
                    plugin_abi = $comparePluginPluginAbi
                    registry = $comparePluginRegistry
                }
                reason = if ($compareAvailable) { $null } else { "adapter compare parse failure or missing adapter/plugin abi/registry lines" }
            }
        } catch {
            $adapterBackendCompareSignal = [ordered]@{
                enabled = $true
                available = $false
                pass = $false
                compared_mode = "ffi_v2"
                expected_chain = $AdapterExpectedChain.ToLowerInvariant()
                plugin_path = $adapterComparePluginPath
                state_root_equal = $false
                native = $null
                plugin = $null
                reason = "adapter compare execution failed: $($_.Exception.Message)"
            }
        }
    }
}

$adapterNegativePluginPath = if ($AdapterNegativePluginPath) {
    $AdapterNegativePluginPath
} elseif ($AdapterComparePluginPath) {
    $AdapterComparePluginPath
} else {
    $AdapterPluginPath
}
$adapterPluginAbiNegativeSignal = [ordered]@{
    enabled = $IncludeAdapterPluginAbiNegative
    available = $false
    pass = $false
    plugin_path = $adapterNegativePluginPath
    expected_abi = $AdapterPluginExpectedAbi
    required_caps = $adapterPluginRequiredCapsNormalized
    abi_mismatch = $null
    capability_mismatch = $null
    reason = "disabled"
}
if ($IncludeAdapterPluginAbiNegative) {
    if (-not $adapterNegativePluginPath) {
        $adapterPluginAbiNegativeSignal.reason = "adapter plugin abi negative signal requires plugin path"
    } elseif (-not (Test-Path $adapterNegativePluginPath)) {
        $adapterPluginAbiNegativeSignal.reason = "adapter negative plugin path not found: $adapterNegativePluginPath"
    } else {
        $abiMismatchExpected = $AdapterPluginExpectedAbi + 1
        $capMismatchRequired = if ($adapterPluginRequiredCapsNormalized -eq "0x3") { "0x5" } else { "0x3" }

        $commonNegativeEnv = @{
            NOVOVM_EXEC_PATH = "ffi_v2"
            NOVOVM_AOEM_VARIANT = "core"
            NOVOVM_DEMO_TXS = "$BatchADemoTxs"
            NOVOVM_BATCH_A_BATCHES = "$BatchABatchCount"
            NOVOVM_MEMPOOL_FEE_FLOOR = "$BatchAMempoolFeeFloor"
            NOVOVM_ADAPTER_BACKEND = "plugin"
            NOVOVM_ADAPTER_PLUGIN_PATH = "$adapterNegativePluginPath"
            NOVOVM_ADAPTER_CHAIN = "$AdapterExpectedChain"
            NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH = ""
            NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "0"
            NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256 = ""
        }

        $abiMismatchEnv = @{}
        foreach ($k in $commonNegativeEnv.Keys) { $abiMismatchEnv[$k] = $commonNegativeEnv[$k] }
        $abiMismatchEnv["NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI"] = "$abiMismatchExpected"
        $abiMismatchEnv["NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS"] = "$adapterPluginRequiredCapsNormalized"
        $abiMismatchProbe = Invoke-CargoAllowFailure -WorkDir $nodeDir -CargoArgs @("run") -EnvVars $abiMismatchEnv
        $abiMismatchFailedAsExpected = ($abiMismatchProbe.exit_code -ne 0)
        $abiMismatchReasonMatch = ($abiMismatchProbe.output -match "ABI version mismatch")

        $capMismatchEnv = @{}
        foreach ($k in $commonNegativeEnv.Keys) { $capMismatchEnv[$k] = $commonNegativeEnv[$k] }
        $capMismatchEnv["NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI"] = "$AdapterPluginExpectedAbi"
        $capMismatchEnv["NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS"] = "$capMismatchRequired"
        $capMismatchProbe = Invoke-CargoAllowFailure -WorkDir $nodeDir -CargoArgs @("run") -EnvVars $capMismatchEnv
        $capMismatchFailedAsExpected = ($capMismatchProbe.exit_code -ne 0)
        $capMismatchReasonMatch = ($capMismatchProbe.output -match "capability mismatch")

        $adapterPluginAbiNegativeSignal = [ordered]@{
            enabled = $true
            available = $true
            pass = ($abiMismatchFailedAsExpected -and $abiMismatchReasonMatch -and $capMismatchFailedAsExpected -and $capMismatchReasonMatch)
            plugin_path = $adapterNegativePluginPath
            expected_abi = $AdapterPluginExpectedAbi
            required_caps = $adapterPluginRequiredCapsNormalized
            abi_mismatch = [ordered]@{
                expected_override = $abiMismatchExpected
                failed_as_expected = $abiMismatchFailedAsExpected
                reason_match = $abiMismatchReasonMatch
                exit_code = $abiMismatchProbe.exit_code
            }
            capability_mismatch = [ordered]@{
                required_override = $capMismatchRequired
                failed_as_expected = $capMismatchFailedAsExpected
                reason_match = $capMismatchReasonMatch
                exit_code = $capMismatchProbe.exit_code
            }
            reason = $null
        }
    }
}

$adapterSymbolNegativePluginPath = if ($AdapterSymbolNegativePluginPath) {
    $AdapterSymbolNegativePluginPath
} else {
    $candidateAoem = Join-Path $aoemRoot "bin\aoem_ffi.dll"
    if (Test-Path $candidateAoem) {
        $candidateAoem
    } else {
        Join-Path $env:WINDIR "System32\kernel32.dll"
    }
}
$adapterPluginSymbolNegativeSignal = [ordered]@{
    enabled = $IncludeAdapterPluginSymbolNegative
    available = $false
    pass = $false
    plugin_path = $adapterSymbolNegativePluginPath
    failed_as_expected = $false
    reason_match = $false
    exit_code = $null
    reason = "disabled"
}
if ($IncludeAdapterPluginSymbolNegative) {
    if (-not (Test-Path $adapterSymbolNegativePluginPath)) {
        $adapterPluginSymbolNegativeSignal.reason = "adapter symbol negative plugin path not found: $adapterSymbolNegativePluginPath"
    } else {
        $symbolNegativeEnv = @{
            NOVOVM_EXEC_PATH = "ffi_v2"
            NOVOVM_AOEM_VARIANT = "core"
            NOVOVM_DEMO_TXS = "$BatchADemoTxs"
            NOVOVM_BATCH_A_BATCHES = "$BatchABatchCount"
            NOVOVM_MEMPOOL_FEE_FLOOR = "$BatchAMempoolFeeFloor"
            NOVOVM_ADAPTER_BACKEND = "plugin"
            NOVOVM_ADAPTER_PLUGIN_PATH = "$adapterSymbolNegativePluginPath"
            NOVOVM_ADAPTER_CHAIN = "$AdapterExpectedChain"
            NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI = "$AdapterPluginExpectedAbi"
            NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS = "$adapterPluginRequiredCapsNormalized"
            NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH = ""
            NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "0"
            NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256 = ""
        }
        $symbolNegativeProbe = Invoke-CargoAllowFailure -WorkDir $nodeDir -CargoArgs @("run") -EnvVars $symbolNegativeEnv
        $symbolNegativeFailedAsExpected = ($symbolNegativeProbe.exit_code -ne 0)
        $symbolNegativeReasonMatch = (
            ($symbolNegativeProbe.output -match "resolve novovm_adapter_plugin_version failed") -or
            ($symbolNegativeProbe.output -match "resolve novovm_adapter_plugin_capabilities failed") -or
            ($symbolNegativeProbe.output -match "resolve novovm_adapter_plugin_apply_v1 failed") -or
            ($symbolNegativeProbe.output -match "load adapter plugin failed")
        )
        $adapterPluginSymbolNegativeSignal = [ordered]@{
            enabled = $true
            available = $true
            pass = ($symbolNegativeFailedAsExpected -and $symbolNegativeReasonMatch)
            plugin_path = $adapterSymbolNegativePluginPath
            failed_as_expected = $symbolNegativeFailedAsExpected
            reason_match = $symbolNegativeReasonMatch
            exit_code = $symbolNegativeProbe.exit_code
            reason = $null
        }
    }
}

$adapterRegistryNegativePluginPath = if ($AdapterPluginPath) {
    $AdapterPluginPath
} elseif ($AdapterComparePluginPath) {
    $AdapterComparePluginPath
} else {
    $AdapterNegativePluginPath
}
$adapterPluginRegistryNegativeSignal = [ordered]@{
    enabled = $IncludeAdapterPluginRegistryNegative
    available = $false
    pass = $false
    plugin_path = $adapterRegistryNegativePluginPath
    source_registry = $adapterPluginRegistryPathResolved
    hash_mismatch = $null
    whitelist_mismatch = $null
    reason = "disabled"
}
if ($IncludeAdapterPluginRegistryNegative) {
    if (-not $adapterRegistryNegativePluginPath) {
        $adapterPluginRegistryNegativeSignal.reason = "adapter plugin registry negative signal requires plugin path"
    } elseif (-not (Test-Path $adapterRegistryNegativePluginPath)) {
        $adapterPluginRegistryNegativeSignal.reason = "adapter registry negative plugin path not found: $adapterRegistryNegativePluginPath"
    } elseif (-not $adapterPluginRegistryPathResolved) {
        $adapterPluginRegistryNegativeSignal.reason = "adapter plugin registry negative signal requires registry path"
    } elseif (-not (Test-Path $adapterPluginRegistryPathResolved)) {
        $adapterPluginRegistryNegativeSignal.reason = "adapter plugin registry path not found: $adapterPluginRegistryPathResolved"
    } else {
        $baseRegistryHash = if ($adapterPluginRegistrySha256Normalized) {
            $adapterPluginRegistrySha256Normalized
        } else {
            (Get-FileHash -Path $adapterPluginRegistryPathResolved -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        $hashMismatchSha = if ($baseRegistryHash.StartsWith("0")) {
            "1" + $baseRegistryHash.Substring(1)
        } else {
            "0" + $baseRegistryHash.Substring(1)
        }

        $commonRegistryNegativeEnv = @{
            NOVOVM_EXEC_PATH = "ffi_v2"
            NOVOVM_AOEM_VARIANT = "core"
            NOVOVM_DEMO_TXS = "$BatchADemoTxs"
            NOVOVM_BATCH_A_BATCHES = "$BatchABatchCount"
            NOVOVM_MEMPOOL_FEE_FLOOR = "$BatchAMempoolFeeFloor"
            NOVOVM_ADAPTER_BACKEND = "plugin"
            NOVOVM_ADAPTER_PLUGIN_PATH = "$adapterRegistryNegativePluginPath"
            NOVOVM_ADAPTER_CHAIN = "$AdapterExpectedChain"
            NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI = "$AdapterPluginExpectedAbi"
            NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS = "$adapterPluginRequiredCapsNormalized"
            NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "1"
        }

        $hashMismatchEnv = @{}
        foreach ($k in $commonRegistryNegativeEnv.Keys) { $hashMismatchEnv[$k] = $commonRegistryNegativeEnv[$k] }
        $hashMismatchEnv["NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH"] = "$adapterPluginRegistryPathResolved"
        $hashMismatchEnv["NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256"] = "$hashMismatchSha"
        $hashMismatchProbe = Invoke-CargoAllowFailure -WorkDir $nodeDir -CargoArgs @("run") -EnvVars $hashMismatchEnv
        $hashMismatchFailedAsExpected = ($hashMismatchProbe.exit_code -ne 0)
        $hashMismatchReasonMatch = ($hashMismatchProbe.output -match "registry hash mismatch")

        $registryNegativeWhitelistPath = Join-Path $OutputDir "adapter-registry-whitelist-negative.json"
        $registryNegativeWhitelist = Get-Content -Path $adapterPluginRegistryPathResolved -Raw | ConvertFrom-Json
        $registryNegativeWhitelist.allowed_abi_versions = @([int]($AdapterPluginExpectedAbi + 99))
        $registryNegativeWhitelist | ConvertTo-Json -Depth 20 | Set-Content -Path $registryNegativeWhitelistPath -Encoding UTF8
        $registryNegativeWhitelistHash = (Get-FileHash -Path $registryNegativeWhitelistPath -Algorithm SHA256).Hash.ToLowerInvariant()

        $whitelistMismatchEnv = @{}
        foreach ($k in $commonRegistryNegativeEnv.Keys) { $whitelistMismatchEnv[$k] = $commonRegistryNegativeEnv[$k] }
        $whitelistMismatchEnv["NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH"] = "$registryNegativeWhitelistPath"
        $whitelistMismatchEnv["NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256"] = "$registryNegativeWhitelistHash"
        $whitelistMismatchProbe = Invoke-CargoAllowFailure -WorkDir $nodeDir -CargoArgs @("run") -EnvVars $whitelistMismatchEnv
        $whitelistMismatchFailedAsExpected = ($whitelistMismatchProbe.exit_code -ne 0)
        $whitelistMismatchReasonMatch = ($whitelistMismatchProbe.output -match "abi whitelist mismatch")

        $adapterPluginRegistryNegativeSignal = [ordered]@{
            enabled = $true
            available = $true
            pass = ($hashMismatchFailedAsExpected -and $hashMismatchReasonMatch -and $whitelistMismatchFailedAsExpected -and $whitelistMismatchReasonMatch)
            plugin_path = $adapterRegistryNegativePluginPath
            source_registry = $adapterPluginRegistryPathResolved
            hash_mismatch = [ordered]@{
                expected_override = $hashMismatchSha
                failed_as_expected = $hashMismatchFailedAsExpected
                reason_match = $hashMismatchReasonMatch
                exit_code = $hashMismatchProbe.exit_code
            }
            whitelist_mismatch = [ordered]@{
                whitelist_registry = $registryNegativeWhitelistPath
                whitelist_hash = $registryNegativeWhitelistHash
                failed_as_expected = $whitelistMismatchFailedAsExpected
                reason_match = $whitelistMismatchReasonMatch
                exit_code = $whitelistMismatchProbe.exit_code
            }
            reason = $null
        }
    }
}

$networkBlockWireNegativeSignal = [ordered]@{
    enabled = $IncludeNetworkBlockWireNegative
    available = $false
    pass = $false
    source_json = $null
    tamper_mode = "hash_mismatch"
    expected_fail = $false
    reason_match = $false
    block_wire_pass = $null
    block_wire_verified = 0
    block_wire_total = 0
    reason = "disabled"
}
if ($IncludeNetworkBlockWireNegative) {
    $networkBlockWireNegativeSignal = Get-NetworkBlockWireNegativeSignal -RepoRoot $RepoRoot
}

$batchAAvailable = ($null -ne $batchAffi -and $null -ne $batchAlegacy)
$batchAPass = $false
if ($batchAAvailable) {
    $batchAPass = (
        $batchAffi.parse_ok -and
        $batchAlegacy.parse_ok -and
        $batchAffi.committed -and
        $batchAlegacy.committed -and
        $batchAffi.txs -eq $batchAlegacy.txs -and
        $batchAffi.txs -eq $BatchADemoTxs
    )
}

$blockOutAvailable = ($null -ne $blockOutFfi -and $null -ne $blockOutLegacy)
$blockOutPass = $false
if ($blockOutAvailable) {
    $blockOutPass = (
        $blockOutFfi.parse_ok -and
        $blockOutLegacy.parse_ok -and
        $blockOutFfi.batches -eq $blockOutLegacy.batches -and
        $blockOutFfi.batches -ge $expectedBatchMin -and
        $blockOutFfi.txs -eq $blockOutLegacy.txs -and
        $blockOutFfi.txs -eq $BatchADemoTxs -and
        $blockOutFfi.block_hash -eq $blockOutLegacy.block_hash -and
        $blockOutFfi.state_root -eq $blockOutLegacy.state_root
    )
}

$blockWireAvailable = ($null -ne $blockWireFfi -and $null -ne $blockWireLegacy)
$blockWirePass = $false
if ($blockWireAvailable) {
    $blockWirePass = (
        $blockWireFfi.parse_ok -and
        $blockWireLegacy.parse_ok -and
        $blockWireFfi.codec -eq $blockWireLegacy.codec -and
        $blockWireFfi.bytes -eq $blockWireLegacy.bytes -and
        $blockWireFfi.bytes -gt 0 -and
        $blockWireFfi.pass -and
        $blockWireLegacy.pass
    )
}

$commitOutAvailable = ($null -ne $commitOutFfi -and $null -ne $commitOutLegacy)
$commitOutPass = $false
if ($commitOutAvailable) {
    $commitOutPass = (
        $commitOutFfi.parse_ok -and
        $commitOutLegacy.parse_ok -and
        $commitOutFfi.committed -and
        $commitOutLegacy.committed -and
        $commitOutFfi.block_hash -eq $commitOutLegacy.block_hash -and
        $commitOutFfi.state_root -eq $commitOutLegacy.state_root
    )
}

$networkOutAvailable = ($null -ne $networkOutFfi -and $null -ne $networkOutLegacy)
$networkOutPass = $false
if ($networkOutAvailable) {
    $networkOutPass = (
        $networkOutFfi.parse_ok -and
        $networkOutLegacy.parse_ok -and
        $networkOutFfi.transport -eq $networkOutLegacy.transport -and
        $networkOutFfi.msg_kind -eq $networkOutLegacy.msg_kind -and
        $networkOutFfi.sent -eq $networkOutLegacy.sent -and
        $networkOutFfi.received -eq $networkOutLegacy.received -and
        $networkOutFfi.received -ge 1
    )
}

$networkClosureAvailable = ($null -ne $networkClosureFfi -and $null -ne $networkClosureLegacy)
$networkClosurePass = $false
if ($networkClosureAvailable) {
    $networkClosurePass = (
        $networkClosureFfi.parse_ok -and
        $networkClosureLegacy.parse_ok -and
        $networkClosureFfi.nodes -eq $networkClosureLegacy.nodes -and
        $networkClosureFfi.discovery -and
        $networkClosureLegacy.discovery -and
        $networkClosureFfi.gossip -and
        $networkClosureLegacy.gossip -and
        $networkClosureFfi.sync -and
        $networkClosureLegacy.sync
    )
}

$variants = @("core", "persist", "wasm")
$digests = @()
foreach ($variant in $variants) {
    $dll = Get-DllPathForVariant -AoemRoot $aoemRoot -Variant $variant
    $text = Invoke-Cargo -WorkDir $bindingsDir -CargoArgs @(
        "run", "--example", "ffi_consistency_digest", "--",
        "--dll", $dll,
        "--rounds", "$Rounds",
        "--points", "$Points",
        "--key-space", "$KeySpace",
        "--rw", "$Rw",
        "--seed", "$Seed"
    ) -EnvVars @{}
    $parsed = Parse-ConsistencyDigestLine -Text $text
    $parsed["variant"] = $variant
    $parsed["dll"] = $dll
    $digests += [pscustomobject]$parsed
}

$coreDigest = ($digests | Where-Object { $_.variant -eq "core" } | Select-Object -First 1).digest
$crossVariantPass = ($digests | Where-Object { $_.digest -ne $coreDigest } | Measure-Object).Count -eq 0

$stateRootConsistency = [ordered]@{
    available = $false
    pass = $crossVariantPass
    method = "deterministic_digest_proxy"
    root_field = "state_root"
    value = $null
    proxy_digest = $coreDigest
    compared_variants = @($variants)
    reason = "aoem_execute_ops_v2 result does not expose state_root in current AOEM FFI ABI"
}

$capabilitySnapshot = $null
if ($IncludeCapabilitySnapshot) {
    $capabilitySnapshot = Get-CapabilitySnapshot -RepoRoot $RepoRoot -Variant $CapabilityVariant -CapabilityJson $CapabilityJson
}

$networkProcessSignal = [ordered]@{
    available = $false
    pass = $false
    source_json = $null
    mode = $null
    rounds = $null
    rounds_passed = $null
    round_pass_ratio = $null
    node_count = $null
    total_pairs = $null
    passed_pairs = $null
    pair_pass_ratio = $null
    directed_edges_total = $null
    directed_edges_up = $null
    directed_edge_ratio = $null
    block_wire_available = $null
    block_wire_pass = $null
    block_wire_rounds_passed = $null
    block_wire_pass_ratio = $null
    block_wire_verified = $null
    block_wire_total = $null
    block_wire_verified_ratio = $null
    node_a_exit_code = $null
    node_b_exit_code = $null
    reason = "disabled"
}
if ($IncludeNetworkProcessSignal) {
    $networkProcessSignal = Get-NetworkProcessSignal `
        -RepoRoot $RepoRoot `
        -NetworkProcessJson $NetworkProcessJson `
        -NodeCount $NetworkProcessNodeCount `
        -ProbeRounds $NetworkProcessRounds
}

$overallPass = (
    $nodeCompatPass -and
    $crossVariantPass -and
    $txCodecAvailable -and
    $txCodecPass -and
    $mempoolAvailable -and
    $mempoolPass -and
    $txMetaAvailable -and
    $txMetaPass -and
    $adapterAvailable -and
    $adapterPass -and
    $adapterPluginAbiAvailable -and
    $adapterPluginAbiPass -and
    $adapterPluginRegistryAvailable -and
    $adapterPluginRegistryPass -and
    $adapterConsensusAvailable -and
    $adapterConsensusPass -and
    $blockWireAvailable -and
    $blockWirePass
)
if ($IncludeNetworkProcessSignal) {
    $overallPass = ($overallPass -and $networkProcessSignal.available -and $networkProcessSignal.pass)
}
if ($IncludeAdapterBackendCompare) {
    $overallPass = ($overallPass -and $adapterBackendCompareSignal.available -and $adapterBackendCompareSignal.pass)
}
if ($IncludeAdapterPluginAbiNegative) {
    $overallPass = ($overallPass -and $adapterPluginAbiNegativeSignal.available -and $adapterPluginAbiNegativeSignal.pass)
}
if ($IncludeAdapterPluginSymbolNegative) {
    $overallPass = ($overallPass -and $adapterPluginSymbolNegativeSignal.available -and $adapterPluginSymbolNegativeSignal.pass)
}
if ($IncludeAdapterPluginRegistryNegative) {
    $overallPass = ($overallPass -and $adapterPluginRegistryNegativeSignal.available -and $adapterPluginRegistryNegativeSignal.pass)
}
if ($IncludeNetworkBlockWireNegative) {
    $overallPass = ($overallPass -and $networkBlockWireNegativeSignal.available -and $networkBlockWireNegativeSignal.pass)
}

$adapterPluginAbiNote = "adapter_plugin_abi_signal validates ABI/version/capability compatibility (expected_abi=$AdapterPluginExpectedAbi, required_caps=$adapterPluginRequiredCapsNormalized)"
$adapterPluginRegistryNote = "adapter_plugin_registry_signal validates startup registry match (enabled=$adapterPluginRegistryExpectedEnabled, strict=$AdapterPluginRegistryStrict, hash_check=$adapterPluginRegistryExpectedHashCheck, path=$adapterPluginRegistryPathResolved)"
$adapterConsensusNote = "adapter_consensus_binding_signal validates consensus plugin class/hash are present and stable across ffi_v2 and legacy_compat"
$blockWireNote = "block_wire_signal validates protocol-level block header wire encode/decode closure with consensus binding"
$adapterBackendCompareNote = if ($IncludeAdapterBackendCompare) {
    "adapter_backend_compare_signal compares native/plugin backends on identical ffi_v2 input and requires state_root parity"
} else {
    "adapter_backend_compare_signal is disabled for this run"
}
$adapterPluginAbiNegativeNote = if ($IncludeAdapterPluginAbiNegative) {
    "adapter_plugin_abi_negative_signal validates mismatch gates (abi/capability) fail as expected in plugin backend"
} else {
    "adapter_plugin_abi_negative_signal is disabled for this run"
}
$adapterPluginSymbolNegativeNote = if ($IncludeAdapterPluginSymbolNegative) {
    "adapter_plugin_symbol_negative_signal validates invalid plugin symbol load path fails as expected"
} else {
    "adapter_plugin_symbol_negative_signal is disabled for this run"
}
$adapterPluginRegistryNegativeNote = if ($IncludeAdapterPluginRegistryNegative) {
    "adapter_plugin_registry_negative_signal validates strict registry mismatch gates (hash/whitelist) fail as expected"
} else {
    "adapter_plugin_registry_negative_signal is disabled for this run"
}
$networkBlockWireNegativeNote = if ($IncludeNetworkBlockWireNegative) {
    "network_block_wire_negative_signal validates tampered block_header_wire_v1 payload fails with consensus binding mismatch in UDP process probe"
} else {
    "network_block_wire_negative_signal is disabled for this run"
}

$result = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    node_mode_consistency = [ordered]@{
        compared = @("ffi_v2", "legacy_compat")
        pass = $nodeCompatPass
        ffi_v2 = $nodeFfi
        legacy_compat = $nodeLegacy
    }
    tx_codec_signal = [ordered]@{
        available = $txCodecAvailable
        pass = $txCodecPass
        ffi_v2 = $txCodecFfi
        legacy_compat = $txCodecLegacy
    }
    mempool_admission_signal = [ordered]@{
        available = $mempoolAvailable
        pass = $mempoolPass
        ffi_v2 = $mempoolFfi
        legacy_compat = $mempoolLegacy
    }
    tx_metadata_signal = [ordered]@{
        available = $txMetaAvailable
        pass = $txMetaPass
        ffi_v2 = $txMetaFfi
        legacy_compat = $txMetaLegacy
    }
    adapter_signal = [ordered]@{
        available = $adapterAvailable
        pass = $adapterPass
        ffi_v2 = $adapterFfi
        legacy_compat = $adapterLegacy
    }
    adapter_plugin_abi_signal = [ordered]@{
        available = $adapterPluginAbiAvailable
        pass = $adapterPluginAbiPass
        ffi_v2 = $adapterPluginAbiFfi
        legacy_compat = $adapterPluginAbiLegacy
        expected_abi = $AdapterPluginExpectedAbi
        required_caps = $adapterPluginRequiredCapsNormalized
    }
    adapter_plugin_registry_signal = [ordered]@{
        available = $adapterPluginRegistryAvailable
        pass = $adapterPluginRegistryPass
        ffi_v2 = $adapterPluginRegistryFfi
        legacy_compat = $adapterPluginRegistryLegacy
        expected_enabled = $adapterPluginRegistryExpectedEnabled
        expected_strict = [bool]$AdapterPluginRegistryStrict
        expected_hash_check = $adapterPluginRegistryExpectedHashCheck
        expected_registry_sha256 = $adapterPluginRegistrySha256Normalized
        source_path = $adapterPluginRegistryPathResolved
    }
    adapter_consensus_binding_signal = [ordered]@{
        available = $adapterConsensusAvailable
        pass = $adapterConsensusPass
        ffi_v2 = $adapterConsensusFfi
        legacy_compat = $adapterConsensusLegacy
    }
    adapter_backend_compare_signal = $adapterBackendCompareSignal
    adapter_plugin_abi_negative_signal = $adapterPluginAbiNegativeSignal
    adapter_plugin_symbol_negative_signal = $adapterPluginSymbolNegativeSignal
    adapter_plugin_registry_negative_signal = $adapterPluginRegistryNegativeSignal
    network_block_wire_negative_signal = $networkBlockWireNegativeSignal
    batch_a_closure = [ordered]@{
        available = $batchAAvailable
        pass = $batchAPass
        ffi_v2 = $batchAffi
        legacy_compat = $batchAlegacy
    }
    block_wire_signal = [ordered]@{
        available = $blockWireAvailable
        pass = $blockWirePass
        ffi_v2 = $blockWireFfi
        legacy_compat = $blockWireLegacy
    }
    block_output_signal = [ordered]@{
        available = $blockOutAvailable
        pass = $blockOutPass
        ffi_v2 = $blockOutFfi
        legacy_compat = $blockOutLegacy
    }
    commit_output_signal = [ordered]@{
        available = $commitOutAvailable
        pass = $commitOutPass
        ffi_v2 = $commitOutFfi
        legacy_compat = $commitOutLegacy
    }
    network_output_signal = [ordered]@{
        available = $networkOutAvailable
        pass = $networkOutPass
        ffi_v2 = $networkOutFfi
        legacy_compat = $networkOutLegacy
    }
    network_closure_signal = [ordered]@{
        available = $networkClosureAvailable
        pass = $networkClosurePass
        ffi_v2 = $networkClosureFfi
        legacy_compat = $networkClosureLegacy
    }
    variant_digest_consistency = [ordered]@{
        rounds = $Rounds
        points = $Points
        key_space = $KeySpace
        rw = $Rw
        seed = $Seed
        pass = $crossVariantPass
        items = $digests
    }
    state_root_consistency = $stateRootConsistency
    capability_contract = $capabilitySnapshot
    network_process_signal = $networkProcessSignal
    batch_a_input_profile = [ordered]@{
        demo_txs = $BatchADemoTxs
        target_batches = $BatchABatchCount
        expected_min_batches = $expectedBatchMin
        mempool_fee_floor = $BatchAMempoolFeeFloor
    }
    adapter_expected_chain = $AdapterExpectedChain.ToLowerInvariant()
    adapter_expected_backend = $AdapterExpectedBackend.ToLowerInvariant()
    overall_pass = $overallPass
    notes = @(
        "state_root field is recorded as unavailable and validated through deterministic digest proxy",
        "tx_codec_signal validates novovm_local_tx_wire_v1 encode/decode parity across ffi_v2 and legacy_compat routes",
        "mempool_admission_signal validates basic mempool policy (fee floor / nonce / signature) across ffi_v2 and legacy_compat routes",
        "tx_metadata_signal validates account/nonce/fee/signature metadata parity across ffi_v2 and legacy_compat routes",
        "adapter_signal validates host-side tx->adapter IR mapping and deterministic adapter state_root across ffi_v2 and legacy_compat routes (expected_chain=$($AdapterExpectedChain.ToLowerInvariant()), expected_backend=$($AdapterExpectedBackend.ToLowerInvariant()))",
        $adapterPluginAbiNote,
        $adapterPluginRegistryNote,
        $adapterConsensusNote,
        $adapterBackendCompareNote,
        $adapterPluginAbiNegativeNote,
        $adapterPluginSymbolNegativeNote,
        $adapterPluginRegistryNegativeNote,
        $networkBlockWireNegativeNote,
        "batch_a_closure is reported as an execution-to-consensus integration signal and is not a hard gate yet",
        $blockWireNote,
        "block_output_signal compares deterministic block_hash output across ffi_v2 and legacy_compat routes",
        "commit_output_signal compares commit records from in-memory block store across ffi_v2 and legacy_compat routes",
        "network_output_signal compares in-memory transport delivery signal across ffi_v2 and legacy_compat routes",
        "network_closure_signal validates two-node discovery/gossip/sync closure across ffi_v2 and legacy_compat routes",
        "network_process_signal validates mesh/pair-matrix process probe over UDP transport with block_header_wire_v1 payload decode + consensus binding verification (rounds=$NetworkProcessRounds)",
        "capability snapshot records zk/msm readiness at report generation time"
    )
}

$jsonPath = Join-Path $OutputDir "functional-consistency.json"
$mdPath = Join-Path $OutputDir "functional-consistency.md"

$result | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

$md = @(
    "# Functional Consistency Report"
    ""
    "- generated_at_utc: $($result.generated_at_utc)"
    "- overall_pass: $($result.overall_pass)"
    "- node_mode_consistency.pass: $($result.node_mode_consistency.pass)"
    "- tx_codec_signal.available: $($result.tx_codec_signal.available)"
    "- tx_codec_signal.pass: $($result.tx_codec_signal.pass)"
    "- mempool_admission_signal.available: $($result.mempool_admission_signal.available)"
    "- mempool_admission_signal.pass: $($result.mempool_admission_signal.pass)"
    "- tx_metadata_signal.available: $($result.tx_metadata_signal.available)"
    "- tx_metadata_signal.pass: $($result.tx_metadata_signal.pass)"
    "- adapter_signal.available: $($result.adapter_signal.available)"
    "- adapter_signal.pass: $($result.adapter_signal.pass)"
    "- adapter_plugin_abi_signal.available: $($result.adapter_plugin_abi_signal.available)"
    "- adapter_plugin_abi_signal.pass: $($result.adapter_plugin_abi_signal.pass)"
    "- adapter_plugin_abi_signal.expected_abi: $($result.adapter_plugin_abi_signal.expected_abi)"
    "- adapter_plugin_abi_signal.required_caps: $($result.adapter_plugin_abi_signal.required_caps)"
    "- adapter_plugin_registry_signal.available: $($result.adapter_plugin_registry_signal.available)"
    "- adapter_plugin_registry_signal.pass: $($result.adapter_plugin_registry_signal.pass)"
    "- adapter_plugin_registry_signal.expected_enabled: $($result.adapter_plugin_registry_signal.expected_enabled)"
    "- adapter_plugin_registry_signal.expected_strict: $($result.adapter_plugin_registry_signal.expected_strict)"
    "- adapter_plugin_registry_signal.expected_hash_check: $($result.adapter_plugin_registry_signal.expected_hash_check)"
    "- adapter_plugin_registry_signal.expected_registry_sha256: $($result.adapter_plugin_registry_signal.expected_registry_sha256)"
    "- adapter_consensus_binding_signal.available: $($result.adapter_consensus_binding_signal.available)"
    "- adapter_consensus_binding_signal.pass: $($result.adapter_consensus_binding_signal.pass)"
    "- adapter_backend_compare_signal.enabled: $($result.adapter_backend_compare_signal.enabled)"
    "- adapter_backend_compare_signal.available: $($result.adapter_backend_compare_signal.available)"
    "- adapter_backend_compare_signal.pass: $($result.adapter_backend_compare_signal.pass)"
    "- adapter_backend_compare_signal.state_root_equal: $($result.adapter_backend_compare_signal.state_root_equal)"
    "- adapter_plugin_abi_negative_signal.enabled: $($result.adapter_plugin_abi_negative_signal.enabled)"
    "- adapter_plugin_abi_negative_signal.available: $($result.adapter_plugin_abi_negative_signal.available)"
    "- adapter_plugin_abi_negative_signal.pass: $($result.adapter_plugin_abi_negative_signal.pass)"
    "- adapter_plugin_symbol_negative_signal.enabled: $($result.adapter_plugin_symbol_negative_signal.enabled)"
    "- adapter_plugin_symbol_negative_signal.available: $($result.adapter_plugin_symbol_negative_signal.available)"
    "- adapter_plugin_symbol_negative_signal.pass: $($result.adapter_plugin_symbol_negative_signal.pass)"
    "- adapter_plugin_registry_negative_signal.enabled: $($result.adapter_plugin_registry_negative_signal.enabled)"
    "- adapter_plugin_registry_negative_signal.available: $($result.adapter_plugin_registry_negative_signal.available)"
    "- adapter_plugin_registry_negative_signal.pass: $($result.adapter_plugin_registry_negative_signal.pass)"
    "- network_block_wire_negative_signal.enabled: $($result.network_block_wire_negative_signal.enabled)"
    "- network_block_wire_negative_signal.available: $($result.network_block_wire_negative_signal.available)"
    "- network_block_wire_negative_signal.pass: $($result.network_block_wire_negative_signal.pass)"
    "- adapter_expected_chain: $($result.adapter_expected_chain)"
    "- adapter_expected_backend: $($result.adapter_expected_backend)"
    "- batch_a_input_profile.demo_txs: $($result.batch_a_input_profile.demo_txs)"
    "- batch_a_input_profile.target_batches: $($result.batch_a_input_profile.target_batches)"
    "- batch_a_input_profile.expected_min_batches: $($result.batch_a_input_profile.expected_min_batches)"
    "- batch_a_input_profile.mempool_fee_floor: $($result.batch_a_input_profile.mempool_fee_floor)"
    "- batch_a_closure.available: $($result.batch_a_closure.available)"
    "- batch_a_closure.pass: $($result.batch_a_closure.pass)"
    "- block_wire_signal.available: $($result.block_wire_signal.available)"
    "- block_wire_signal.pass: $($result.block_wire_signal.pass)"
    "- block_output_signal.available: $($result.block_output_signal.available)"
    "- block_output_signal.pass: $($result.block_output_signal.pass)"
    "- commit_output_signal.available: $($result.commit_output_signal.available)"
    "- commit_output_signal.pass: $($result.commit_output_signal.pass)"
    "- network_output_signal.available: $($result.network_output_signal.available)"
    "- network_output_signal.pass: $($result.network_output_signal.pass)"
    "- network_closure_signal.available: $($result.network_closure_signal.available)"
    "- network_closure_signal.pass: $($result.network_closure_signal.pass)"
    "- network_process_signal.available: $($result.network_process_signal.available)"
    "- network_process_signal.pass: $($result.network_process_signal.pass)"
    "- network_process_signal.rounds: $($result.network_process_signal.rounds)"
    "- network_process_signal.rounds_passed: $($result.network_process_signal.rounds_passed)"
    "- network_process_signal.round_pass_ratio: $($result.network_process_signal.round_pass_ratio)"
    "- network_process_signal.block_wire_available: $($result.network_process_signal.block_wire_available)"
    "- network_process_signal.block_wire_pass: $($result.network_process_signal.block_wire_pass)"
    "- network_process_signal.block_wire_pass_ratio: $($result.network_process_signal.block_wire_pass_ratio)"
    "- variant_digest_consistency.pass: $($result.variant_digest_consistency.pass)"
    "- state_root_consistency.available: $($result.state_root_consistency.available)"
    "- state_root_consistency.pass: $($result.state_root_consistency.pass)"
    ""
    "## Node Mode Consistency"
    ""
    "| mode | rc | processed | success | writes |"
    "|---|---:|---:|---:|---:|"
    "| ffi_v2 | $($nodeFfi.rc) | $($nodeFfi.processed) | $($nodeFfi.success) | $($nodeFfi.writes) |"
    "| legacy_compat | $($nodeLegacy.rc) | $($nodeLegacy.processed) | $($nodeLegacy.success) | $($nodeLegacy.writes) |"
    ""
    "## Tx Codec Signal"
    ""
    "- available: $($result.tx_codec_signal.available)"
    "- pass: $($result.tx_codec_signal.pass)"
    ""
    "## Mempool Admission Signal"
    ""
    "- available: $($result.mempool_admission_signal.available)"
    "- pass: $($result.mempool_admission_signal.pass)"
    ""
    "## Tx Metadata Signal"
    ""
    "- available: $($result.tx_metadata_signal.available)"
    "- pass: $($result.tx_metadata_signal.pass)"
    ""
    "## Adapter Signal"
    ""
    "- available: $($result.adapter_signal.available)"
    "- pass: $($result.adapter_signal.pass)"
    "- expected_chain: $($result.adapter_expected_chain)"
    "- expected_backend: $($result.adapter_expected_backend)"
    ""
    "## Adapter Plugin ABI Signal"
    ""
    "- available: $($result.adapter_plugin_abi_signal.available)"
    "- pass: $($result.adapter_plugin_abi_signal.pass)"
    "- expected_abi: $($result.adapter_plugin_abi_signal.expected_abi)"
    "- required_caps: $($result.adapter_plugin_abi_signal.required_caps)"
    ""
    "## Adapter Plugin Registry Signal"
    ""
    "- available: $($result.adapter_plugin_registry_signal.available)"
    "- pass: $($result.adapter_plugin_registry_signal.pass)"
    "- expected_enabled: $($result.adapter_plugin_registry_signal.expected_enabled)"
    "- expected_strict: $($result.adapter_plugin_registry_signal.expected_strict)"
    ""
    "## Adapter Backend Compare Signal"
    ""
    "- enabled: $($result.adapter_backend_compare_signal.enabled)"
    "- available: $($result.adapter_backend_compare_signal.available)"
    "- pass: $($result.adapter_backend_compare_signal.pass)"
    "- compared_mode: $($result.adapter_backend_compare_signal.compared_mode)"
    "- expected_chain: $($result.adapter_backend_compare_signal.expected_chain)"
    "- state_root_equal: $($result.adapter_backend_compare_signal.state_root_equal)"
    ""
    "## Adapter Plugin ABI Negative Signal"
    ""
    "- enabled: $($result.adapter_plugin_abi_negative_signal.enabled)"
    "- available: $($result.adapter_plugin_abi_negative_signal.available)"
    "- pass: $($result.adapter_plugin_abi_negative_signal.pass)"
    ""
    "## Adapter Plugin Symbol Negative Signal"
    ""
    "- enabled: $($result.adapter_plugin_symbol_negative_signal.enabled)"
    "- available: $($result.adapter_plugin_symbol_negative_signal.available)"
    "- pass: $($result.adapter_plugin_symbol_negative_signal.pass)"
    ""
    "## Adapter Plugin Registry Negative Signal"
    ""
    "- enabled: $($result.adapter_plugin_registry_negative_signal.enabled)"
    "- available: $($result.adapter_plugin_registry_negative_signal.available)"
    "- pass: $($result.adapter_plugin_registry_negative_signal.pass)"
    ""
    "## Network Block Wire Negative Signal"
    ""
    "- enabled: $($result.network_block_wire_negative_signal.enabled)"
    "- available: $($result.network_block_wire_negative_signal.available)"
    "- pass: $($result.network_block_wire_negative_signal.pass)"
    ""
    "## Batch A Closure Signal"
    ""
    "- available: $($result.batch_a_closure.available)"
    "- pass: $($result.batch_a_closure.pass)"
    ""
    "## Block Wire Signal"
    ""
    "- available: $($result.block_wire_signal.available)"
    "- pass: $($result.block_wire_signal.pass)"
    ""
    "## Block Output Signal"
    ""
    "- available: $($result.block_output_signal.available)"
    "- pass: $($result.block_output_signal.pass)"
    ""
    "## Commit Output Signal"
    ""
    "- available: $($result.commit_output_signal.available)"
    "- pass: $($result.commit_output_signal.pass)"
    ""
    "## Network Output Signal"
    ""
    "- available: $($result.network_output_signal.available)"
    "- pass: $($result.network_output_signal.pass)"
    ""
    "## Network Closure Signal"
    ""
    "- available: $($result.network_closure_signal.available)"
    "- pass: $($result.network_closure_signal.pass)"
    ""
    "## Network Process Signal"
    ""
    "- available: $($result.network_process_signal.available)"
    "- pass: $($result.network_process_signal.pass)"
    "- block_wire_available: $($result.network_process_signal.block_wire_available)"
    "- block_wire_pass: $($result.network_process_signal.block_wire_pass)"
    ""
    "## Variant Digest Consistency"
    ""
    "| variant | digest | total_processed | total_success | total_writes |"
    "|---|---|---:|---:|---:|"
)

foreach ($item in $digests) {
    $md += "| $($item.variant) | $($item.digest) | $($item.total_processed) | $($item.total_success) | $($item.total_writes) |"
}

if ($result.tx_codec_signal.available -and $result.tx_codec_signal.ffi_v2.parse_ok -and $result.tx_codec_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Tx codec signal:"
    $md += ""
    $md += "- ffi_v2.codec: $($result.tx_codec_signal.ffi_v2.codec)"
    $md += "- legacy_compat.codec: $($result.tx_codec_signal.legacy_compat.codec)"
    $md += "- ffi_v2.encoded: $($result.tx_codec_signal.ffi_v2.encoded)"
    $md += "- legacy_compat.encoded: $($result.tx_codec_signal.legacy_compat.encoded)"
    $md += "- ffi_v2.decoded: $($result.tx_codec_signal.ffi_v2.decoded)"
    $md += "- legacy_compat.decoded: $($result.tx_codec_signal.legacy_compat.decoded)"
    $md += "- ffi_v2.bytes: $($result.tx_codec_signal.ffi_v2.bytes)"
    $md += "- legacy_compat.bytes: $($result.tx_codec_signal.legacy_compat.bytes)"
    $md += "- ffi_v2.pass: $($result.tx_codec_signal.ffi_v2.pass)"
    $md += "- legacy_compat.pass: $($result.tx_codec_signal.legacy_compat.pass)"
}

if ($result.mempool_admission_signal.available -and $result.mempool_admission_signal.ffi_v2.parse_ok -and $result.mempool_admission_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Mempool admission signal:"
    $md += ""
    $md += "- ffi_v2.policy: $($result.mempool_admission_signal.ffi_v2.policy)"
    $md += "- legacy_compat.policy: $($result.mempool_admission_signal.legacy_compat.policy)"
    $md += "- ffi_v2.accepted: $($result.mempool_admission_signal.ffi_v2.accepted)"
    $md += "- legacy_compat.accepted: $($result.mempool_admission_signal.legacy_compat.accepted)"
    $md += "- ffi_v2.rejected: $($result.mempool_admission_signal.ffi_v2.rejected)"
    $md += "- legacy_compat.rejected: $($result.mempool_admission_signal.legacy_compat.rejected)"
    $md += "- ffi_v2.fee_floor: $($result.mempool_admission_signal.ffi_v2.fee_floor)"
    $md += "- legacy_compat.fee_floor: $($result.mempool_admission_signal.legacy_compat.fee_floor)"
    $md += "- ffi_v2.nonce_ok: $($result.mempool_admission_signal.ffi_v2.nonce_ok)"
    $md += "- legacy_compat.nonce_ok: $($result.mempool_admission_signal.legacy_compat.nonce_ok)"
    $md += "- ffi_v2.sig_ok: $($result.mempool_admission_signal.ffi_v2.sig_ok)"
    $md += "- legacy_compat.sig_ok: $($result.mempool_admission_signal.legacy_compat.sig_ok)"
}

if ($result.tx_metadata_signal.available -and $result.tx_metadata_signal.ffi_v2.parse_ok -and $result.tx_metadata_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Tx metadata signal:"
    $md += ""
    $md += "- ffi_v2.accounts: $($result.tx_metadata_signal.ffi_v2.accounts)"
    $md += "- legacy_compat.accounts: $($result.tx_metadata_signal.legacy_compat.accounts)"
    $md += "- ffi_v2.txs: $($result.tx_metadata_signal.ffi_v2.txs)"
    $md += "- legacy_compat.txs: $($result.tx_metadata_signal.legacy_compat.txs)"
    $md += "- ffi_v2.min_fee: $($result.tx_metadata_signal.ffi_v2.min_fee)"
    $md += "- legacy_compat.min_fee: $($result.tx_metadata_signal.legacy_compat.min_fee)"
    $md += "- ffi_v2.max_fee: $($result.tx_metadata_signal.ffi_v2.max_fee)"
    $md += "- legacy_compat.max_fee: $($result.tx_metadata_signal.legacy_compat.max_fee)"
    $md += "- ffi_v2.nonce_ok: $($result.tx_metadata_signal.ffi_v2.nonce_ok)"
    $md += "- legacy_compat.nonce_ok: $($result.tx_metadata_signal.legacy_compat.nonce_ok)"
    $md += "- ffi_v2.sig_ok: $($result.tx_metadata_signal.ffi_v2.sig_ok)"
    $md += "- legacy_compat.sig_ok: $($result.tx_metadata_signal.legacy_compat.sig_ok)"
}

if ($result.adapter_plugin_abi_signal.available -and $result.adapter_plugin_abi_signal.ffi_v2.parse_ok -and $result.adapter_plugin_abi_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Adapter plugin ABI signal:"
    $md += ""
    $md += "- ffi_v2.enabled: $($result.adapter_plugin_abi_signal.ffi_v2.enabled)"
    $md += "- legacy_compat.enabled: $($result.adapter_plugin_abi_signal.legacy_compat.enabled)"
    $md += "- ffi_v2.version: $($result.adapter_plugin_abi_signal.ffi_v2.version)"
    $md += "- legacy_compat.version: $($result.adapter_plugin_abi_signal.legacy_compat.version)"
    $md += "- ffi_v2.expected: $($result.adapter_plugin_abi_signal.ffi_v2.expected)"
    $md += "- legacy_compat.expected: $($result.adapter_plugin_abi_signal.legacy_compat.expected)"
    $md += "- ffi_v2.caps: $($result.adapter_plugin_abi_signal.ffi_v2.caps)"
    $md += "- legacy_compat.caps: $($result.adapter_plugin_abi_signal.legacy_compat.caps)"
    $md += "- ffi_v2.required: $($result.adapter_plugin_abi_signal.ffi_v2.required)"
    $md += "- legacy_compat.required: $($result.adapter_plugin_abi_signal.legacy_compat.required)"
    $md += "- ffi_v2.compatible: $($result.adapter_plugin_abi_signal.ffi_v2.compatible)"
    $md += "- legacy_compat.compatible: $($result.adapter_plugin_abi_signal.legacy_compat.compatible)"
}

if ($result.adapter_plugin_registry_signal.available -and $result.adapter_plugin_registry_signal.ffi_v2.parse_ok -and $result.adapter_plugin_registry_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Adapter plugin registry signal:"
    $md += ""
    $md += "- source_path: $($result.adapter_plugin_registry_signal.source_path)"
    $md += "- expected_enabled: $($result.adapter_plugin_registry_signal.expected_enabled)"
    $md += "- expected_strict: $($result.adapter_plugin_registry_signal.expected_strict)"
    $md += "- expected_hash_check: $($result.adapter_plugin_registry_signal.expected_hash_check)"
    $md += "- expected_registry_sha256: $($result.adapter_plugin_registry_signal.expected_registry_sha256)"
    $md += "- ffi_v2.enabled: $($result.adapter_plugin_registry_signal.ffi_v2.enabled)"
    $md += "- legacy_compat.enabled: $($result.adapter_plugin_registry_signal.legacy_compat.enabled)"
    $md += "- ffi_v2.strict: $($result.adapter_plugin_registry_signal.ffi_v2.strict)"
    $md += "- legacy_compat.strict: $($result.adapter_plugin_registry_signal.legacy_compat.strict)"
    $md += "- ffi_v2.matched: $($result.adapter_plugin_registry_signal.ffi_v2.matched)"
    $md += "- legacy_compat.matched: $($result.adapter_plugin_registry_signal.legacy_compat.matched)"
    $md += "- ffi_v2.chain_allowed: $($result.adapter_plugin_registry_signal.ffi_v2.chain_allowed)"
    $md += "- legacy_compat.chain_allowed: $($result.adapter_plugin_registry_signal.legacy_compat.chain_allowed)"
    $md += "- ffi_v2.entry_abi: $($result.adapter_plugin_registry_signal.ffi_v2.entry_abi)"
    $md += "- legacy_compat.entry_abi: $($result.adapter_plugin_registry_signal.legacy_compat.entry_abi)"
    $md += "- ffi_v2.entry_required: $($result.adapter_plugin_registry_signal.ffi_v2.entry_required)"
    $md += "- legacy_compat.entry_required: $($result.adapter_plugin_registry_signal.legacy_compat.entry_required)"
    $md += "- ffi_v2.hash_check: $($result.adapter_plugin_registry_signal.ffi_v2.hash_check)"
    $md += "- legacy_compat.hash_check: $($result.adapter_plugin_registry_signal.legacy_compat.hash_check)"
    $md += "- ffi_v2.hash_match: $($result.adapter_plugin_registry_signal.ffi_v2.hash_match)"
    $md += "- legacy_compat.hash_match: $($result.adapter_plugin_registry_signal.legacy_compat.hash_match)"
    $md += "- ffi_v2.abi_whitelist: $($result.adapter_plugin_registry_signal.ffi_v2.abi_whitelist)"
    $md += "- legacy_compat.abi_whitelist: $($result.adapter_plugin_registry_signal.legacy_compat.abi_whitelist)"
    $md += "- ffi_v2.abi_allowed: $($result.adapter_plugin_registry_signal.ffi_v2.abi_allowed)"
    $md += "- legacy_compat.abi_allowed: $($result.adapter_plugin_registry_signal.legacy_compat.abi_allowed)"
}

if ($result.adapter_consensus_binding_signal.available -and $result.adapter_consensus_binding_signal.ffi_v2.parse_ok -and $result.adapter_consensus_binding_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Adapter consensus binding signal:"
    $md += ""
    $md += "- ffi_v2.plugin_class: $($result.adapter_consensus_binding_signal.ffi_v2.plugin_class)"
    $md += "- legacy_compat.plugin_class: $($result.adapter_consensus_binding_signal.legacy_compat.plugin_class)"
    $md += "- ffi_v2.plugin_class_code: $($result.adapter_consensus_binding_signal.ffi_v2.plugin_class_code)"
    $md += "- legacy_compat.plugin_class_code: $($result.adapter_consensus_binding_signal.legacy_compat.plugin_class_code)"
    $md += "- ffi_v2.consensus_adapter_hash: $($result.adapter_consensus_binding_signal.ffi_v2.consensus_adapter_hash)"
    $md += "- legacy_compat.consensus_adapter_hash: $($result.adapter_consensus_binding_signal.legacy_compat.consensus_adapter_hash)"
    $md += "- ffi_v2.backend: $($result.adapter_consensus_binding_signal.ffi_v2.backend)"
    $md += "- legacy_compat.backend: $($result.adapter_consensus_binding_signal.legacy_compat.backend)"
}

if ($result.adapter_signal.available -and $result.adapter_signal.ffi_v2.parse_ok -and $result.adapter_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Adapter signal:"
    $md += ""
    $md += "- ffi_v2.backend: $($result.adapter_signal.ffi_v2.backend)"
    $md += "- legacy_compat.backend: $($result.adapter_signal.legacy_compat.backend)"
    $md += "- ffi_v2.chain: $($result.adapter_signal.ffi_v2.chain)"
    $md += "- legacy_compat.chain: $($result.adapter_signal.legacy_compat.chain)"
    $md += "- ffi_v2.txs: $($result.adapter_signal.ffi_v2.txs)"
    $md += "- legacy_compat.txs: $($result.adapter_signal.legacy_compat.txs)"
    $md += "- ffi_v2.accounts: $($result.adapter_signal.ffi_v2.accounts)"
    $md += "- legacy_compat.accounts: $($result.adapter_signal.legacy_compat.accounts)"
    $md += "- ffi_v2.verified: $($result.adapter_signal.ffi_v2.verified)"
    $md += "- legacy_compat.verified: $($result.adapter_signal.legacy_compat.verified)"
    $md += "- ffi_v2.applied: $($result.adapter_signal.ffi_v2.applied)"
    $md += "- legacy_compat.applied: $($result.adapter_signal.legacy_compat.applied)"
    $md += "- ffi_v2.state_root: $($result.adapter_signal.ffi_v2.state_root)"
    $md += "- legacy_compat.state_root: $($result.adapter_signal.legacy_compat.state_root)"
}

if ($result.adapter_backend_compare_signal.available -and
    $result.adapter_backend_compare_signal.native.adapter.parse_ok -and
    $result.adapter_backend_compare_signal.plugin.adapter.parse_ok) {
    $md += ""
    $md += "Adapter backend compare signal:"
    $md += ""
    $md += "- plugin_path: $($result.adapter_backend_compare_signal.plugin_path)"
    $md += "- native.backend: $($result.adapter_backend_compare_signal.native.adapter.backend)"
    $md += "- plugin.backend: $($result.adapter_backend_compare_signal.plugin.adapter.backend)"
    $md += "- native.chain: $($result.adapter_backend_compare_signal.native.adapter.chain)"
    $md += "- plugin.chain: $($result.adapter_backend_compare_signal.plugin.adapter.chain)"
    $md += "- native.txs: $($result.adapter_backend_compare_signal.native.adapter.txs)"
    $md += "- plugin.txs: $($result.adapter_backend_compare_signal.plugin.adapter.txs)"
    $md += "- native.accounts: $($result.adapter_backend_compare_signal.native.adapter.accounts)"
    $md += "- plugin.accounts: $($result.adapter_backend_compare_signal.plugin.adapter.accounts)"
    $md += "- native.state_root: $($result.adapter_backend_compare_signal.native.adapter.state_root)"
    $md += "- plugin.state_root: $($result.adapter_backend_compare_signal.plugin.adapter.state_root)"
    $md += "- native.plugin_abi.enabled: $($result.adapter_backend_compare_signal.native.plugin_abi.enabled)"
    $md += "- plugin.plugin_abi.enabled: $($result.adapter_backend_compare_signal.plugin.plugin_abi.enabled)"
    $md += "- native.plugin_abi.expected: $($result.adapter_backend_compare_signal.native.plugin_abi.expected)"
    $md += "- plugin.plugin_abi.expected: $($result.adapter_backend_compare_signal.plugin.plugin_abi.expected)"
    $md += "- native.plugin_abi.required: $($result.adapter_backend_compare_signal.native.plugin_abi.required)"
    $md += "- plugin.plugin_abi.required: $($result.adapter_backend_compare_signal.plugin.plugin_abi.required)"
    $md += "- native.plugin_abi.compatible: $($result.adapter_backend_compare_signal.native.plugin_abi.compatible)"
    $md += "- plugin.plugin_abi.compatible: $($result.adapter_backend_compare_signal.plugin.plugin_abi.compatible)"
    $md += "- native.registry.enabled: $($result.adapter_backend_compare_signal.native.registry.enabled)"
    $md += "- plugin.registry.enabled: $($result.adapter_backend_compare_signal.plugin.registry.enabled)"
    $md += "- native.registry.matched: $($result.adapter_backend_compare_signal.native.registry.matched)"
    $md += "- plugin.registry.matched: $($result.adapter_backend_compare_signal.plugin.registry.matched)"
    $md += "- native.registry.chain_allowed: $($result.adapter_backend_compare_signal.native.registry.chain_allowed)"
    $md += "- plugin.registry.chain_allowed: $($result.adapter_backend_compare_signal.plugin.registry.chain_allowed)"
    $md += "- native.registry.hash_check: $($result.adapter_backend_compare_signal.native.registry.hash_check)"
    $md += "- plugin.registry.hash_check: $($result.adapter_backend_compare_signal.plugin.registry.hash_check)"
    $md += "- native.registry.hash_match: $($result.adapter_backend_compare_signal.native.registry.hash_match)"
    $md += "- plugin.registry.hash_match: $($result.adapter_backend_compare_signal.plugin.registry.hash_match)"
    $md += "- native.registry.abi_whitelist: $($result.adapter_backend_compare_signal.native.registry.abi_whitelist)"
    $md += "- plugin.registry.abi_whitelist: $($result.adapter_backend_compare_signal.plugin.registry.abi_whitelist)"
    $md += "- native.registry.abi_allowed: $($result.adapter_backend_compare_signal.native.registry.abi_allowed)"
    $md += "- plugin.registry.abi_allowed: $($result.adapter_backend_compare_signal.plugin.registry.abi_allowed)"
    $md += "- native.processed: $($result.adapter_backend_compare_signal.native.node.processed)"
    $md += "- plugin.processed: $($result.adapter_backend_compare_signal.plugin.node.processed)"
    $md += "- native.success: $($result.adapter_backend_compare_signal.native.node.success)"
    $md += "- plugin.success: $($result.adapter_backend_compare_signal.plugin.node.success)"
    $md += "- native.writes: $($result.adapter_backend_compare_signal.native.node.writes)"
    $md += "- plugin.writes: $($result.adapter_backend_compare_signal.plugin.node.writes)"
} elseif ($result.adapter_backend_compare_signal.enabled) {
    $md += ""
    $md += "Adapter backend compare signal reason:"
    $md += ""
    $md += "- reason: $($result.adapter_backend_compare_signal.reason)"
}

if ($result.adapter_plugin_abi_negative_signal.available) {
    $md += ""
    $md += "Adapter plugin ABI negative signal:"
    $md += ""
    $md += "- plugin_path: $($result.adapter_plugin_abi_negative_signal.plugin_path)"
    $md += "- expected_abi: $($result.adapter_plugin_abi_negative_signal.expected_abi)"
    $md += "- required_caps: $($result.adapter_plugin_abi_negative_signal.required_caps)"
    $md += "- abi_mismatch.expected_override: $($result.adapter_plugin_abi_negative_signal.abi_mismatch.expected_override)"
    $md += "- abi_mismatch.failed_as_expected: $($result.adapter_plugin_abi_negative_signal.abi_mismatch.failed_as_expected)"
    $md += "- abi_mismatch.reason_match: $($result.adapter_plugin_abi_negative_signal.abi_mismatch.reason_match)"
    $md += "- abi_mismatch.exit_code: $($result.adapter_plugin_abi_negative_signal.abi_mismatch.exit_code)"
    $md += "- capability_mismatch.required_override: $($result.adapter_plugin_abi_negative_signal.capability_mismatch.required_override)"
    $md += "- capability_mismatch.failed_as_expected: $($result.adapter_plugin_abi_negative_signal.capability_mismatch.failed_as_expected)"
    $md += "- capability_mismatch.reason_match: $($result.adapter_plugin_abi_negative_signal.capability_mismatch.reason_match)"
    $md += "- capability_mismatch.exit_code: $($result.adapter_plugin_abi_negative_signal.capability_mismatch.exit_code)"
} elseif ($result.adapter_plugin_abi_negative_signal.enabled) {
    $md += ""
    $md += "Adapter plugin ABI negative signal reason:"
    $md += ""
    $md += "- reason: $($result.adapter_plugin_abi_negative_signal.reason)"
}

if ($result.adapter_plugin_symbol_negative_signal.available) {
    $md += ""
    $md += "Adapter plugin symbol negative signal:"
    $md += ""
    $md += "- plugin_path: $($result.adapter_plugin_symbol_negative_signal.plugin_path)"
    $md += "- failed_as_expected: $($result.adapter_plugin_symbol_negative_signal.failed_as_expected)"
    $md += "- reason_match: $($result.adapter_plugin_symbol_negative_signal.reason_match)"
    $md += "- exit_code: $($result.adapter_plugin_symbol_negative_signal.exit_code)"
} elseif ($result.adapter_plugin_symbol_negative_signal.enabled) {
    $md += ""
    $md += "Adapter plugin symbol negative signal reason:"
    $md += ""
    $md += "- reason: $($result.adapter_plugin_symbol_negative_signal.reason)"
}

if ($result.adapter_plugin_registry_negative_signal.available) {
    $md += ""
    $md += "Adapter plugin registry negative signal:"
    $md += ""
    $md += "- plugin_path: $($result.adapter_plugin_registry_negative_signal.plugin_path)"
    $md += "- source_registry: $($result.adapter_plugin_registry_negative_signal.source_registry)"
    $md += "- hash_mismatch.expected_override: $($result.adapter_plugin_registry_negative_signal.hash_mismatch.expected_override)"
    $md += "- hash_mismatch.failed_as_expected: $($result.adapter_plugin_registry_negative_signal.hash_mismatch.failed_as_expected)"
    $md += "- hash_mismatch.reason_match: $($result.adapter_plugin_registry_negative_signal.hash_mismatch.reason_match)"
    $md += "- hash_mismatch.exit_code: $($result.adapter_plugin_registry_negative_signal.hash_mismatch.exit_code)"
    $md += "- whitelist_mismatch.whitelist_registry: $($result.adapter_plugin_registry_negative_signal.whitelist_mismatch.whitelist_registry)"
    $md += "- whitelist_mismatch.whitelist_hash: $($result.adapter_plugin_registry_negative_signal.whitelist_mismatch.whitelist_hash)"
    $md += "- whitelist_mismatch.failed_as_expected: $($result.adapter_plugin_registry_negative_signal.whitelist_mismatch.failed_as_expected)"
    $md += "- whitelist_mismatch.reason_match: $($result.adapter_plugin_registry_negative_signal.whitelist_mismatch.reason_match)"
    $md += "- whitelist_mismatch.exit_code: $($result.adapter_plugin_registry_negative_signal.whitelist_mismatch.exit_code)"
} elseif ($result.adapter_plugin_registry_negative_signal.enabled) {
    $md += ""
    $md += "Adapter plugin registry negative signal reason:"
    $md += ""
    $md += "- reason: $($result.adapter_plugin_registry_negative_signal.reason)"
}

if ($result.network_block_wire_negative_signal.available) {
    $md += ""
    $md += "Network block wire negative signal:"
    $md += ""
    $md += "- source_json: $($result.network_block_wire_negative_signal.source_json)"
    $md += "- tamper_mode: $($result.network_block_wire_negative_signal.tamper_mode)"
    $md += "- expected_fail: $($result.network_block_wire_negative_signal.expected_fail)"
    $md += "- reason_match: $($result.network_block_wire_negative_signal.reason_match)"
    $md += "- block_wire_pass: $($result.network_block_wire_negative_signal.block_wire_pass)"
    $md += "- block_wire_verified: $($result.network_block_wire_negative_signal.block_wire_verified)"
    $md += "- block_wire_total: $($result.network_block_wire_negative_signal.block_wire_total)"
} elseif ($result.network_block_wire_negative_signal.enabled) {
    $md += ""
    $md += "Network block wire negative signal reason:"
    $md += ""
    $md += "- reason: $($result.network_block_wire_negative_signal.reason)"
}

if ($result.batch_a_closure.available -and $result.batch_a_closure.ffi_v2.parse_ok -and $result.batch_a_closure.legacy_compat.parse_ok) {
    $md += ""
    $md += "Batch A state roots:"
    $md += ""
    $md += "- ffi_v2: $($result.batch_a_closure.ffi_v2.state_root)"
    $md += "- legacy_compat: $($result.batch_a_closure.legacy_compat.state_root)"
}

if ($result.block_wire_signal.available -and $result.block_wire_signal.ffi_v2.parse_ok -and $result.block_wire_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Block wire signal:"
    $md += ""
    $md += "- ffi_v2.codec: $($result.block_wire_signal.ffi_v2.codec)"
    $md += "- legacy_compat.codec: $($result.block_wire_signal.legacy_compat.codec)"
    $md += "- ffi_v2.bytes: $($result.block_wire_signal.ffi_v2.bytes)"
    $md += "- legacy_compat.bytes: $($result.block_wire_signal.legacy_compat.bytes)"
    $md += "- ffi_v2.pass: $($result.block_wire_signal.ffi_v2.pass)"
    $md += "- legacy_compat.pass: $($result.block_wire_signal.legacy_compat.pass)"
}

if ($result.block_output_signal.available -and $result.block_output_signal.ffi_v2.parse_ok -and $result.block_output_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Block output hashes:"
    $md += ""
    $md += "- ffi_v2.batches: $($result.block_output_signal.ffi_v2.batches)"
    $md += "- legacy_compat.batches: $($result.block_output_signal.legacy_compat.batches)"
    $md += "- ffi_v2.txs: $($result.block_output_signal.ffi_v2.txs)"
    $md += "- legacy_compat.txs: $($result.block_output_signal.legacy_compat.txs)"
    $md += "- ffi_v2.block_hash: $($result.block_output_signal.ffi_v2.block_hash)"
    $md += "- legacy_compat.block_hash: $($result.block_output_signal.legacy_compat.block_hash)"
}

if ($result.commit_output_signal.available -and $result.commit_output_signal.ffi_v2.parse_ok -and $result.commit_output_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Commit output hashes:"
    $md += ""
    $md += "- ffi_v2.block_hash: $($result.commit_output_signal.ffi_v2.block_hash)"
    $md += "- legacy_compat.block_hash: $($result.commit_output_signal.legacy_compat.block_hash)"
}

if ($result.network_output_signal.available -and $result.network_output_signal.ffi_v2.parse_ok -and $result.network_output_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Network output signal:"
    $md += ""
    $md += "- ffi_v2.msg_kind: $($result.network_output_signal.ffi_v2.msg_kind)"
    $md += "- legacy_compat.msg_kind: $($result.network_output_signal.legacy_compat.msg_kind)"
}

if ($result.network_closure_signal.available -and $result.network_closure_signal.ffi_v2.parse_ok -and $result.network_closure_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Network closure signal:"
    $md += ""
    $md += "- ffi_v2: nodes=$($result.network_closure_signal.ffi_v2.nodes), discovery=$($result.network_closure_signal.ffi_v2.discovery), gossip=$($result.network_closure_signal.ffi_v2.gossip), sync=$($result.network_closure_signal.ffi_v2.sync)"
    $md += "- legacy_compat: nodes=$($result.network_closure_signal.legacy_compat.nodes), discovery=$($result.network_closure_signal.legacy_compat.discovery), gossip=$($result.network_closure_signal.legacy_compat.gossip), sync=$($result.network_closure_signal.legacy_compat.sync)"
}

if ($result.network_process_signal.available) {
    $md += ""
    $md += "Network process signal:"
    $md += ""
    $md += "- source_json: $($result.network_process_signal.source_json)"
    $md += "- mode: $($result.network_process_signal.mode)"
    $md += "- rounds: $($result.network_process_signal.rounds)"
    $md += "- rounds_passed: $($result.network_process_signal.rounds_passed)"
    $md += "- round_pass_ratio: $($result.network_process_signal.round_pass_ratio)"
    $md += "- node_count: $($result.network_process_signal.node_count)"
    $md += "- total_pairs: $($result.network_process_signal.total_pairs)"
    $md += "- passed_pairs: $($result.network_process_signal.passed_pairs)"
    $md += "- pair_pass_ratio: $($result.network_process_signal.pair_pass_ratio)"
    $md += "- directed_edges_up: $($result.network_process_signal.directed_edges_up)"
    $md += "- directed_edges_total: $($result.network_process_signal.directed_edges_total)"
    $md += "- directed_edge_ratio: $($result.network_process_signal.directed_edge_ratio)"
    $md += "- block_wire_available: $($result.network_process_signal.block_wire_available)"
    $md += "- block_wire_pass: $($result.network_process_signal.block_wire_pass)"
    $md += "- block_wire_rounds_passed: $($result.network_process_signal.block_wire_rounds_passed)"
    $md += "- block_wire_pass_ratio: $($result.network_process_signal.block_wire_pass_ratio)"
    $md += "- block_wire_verified: $($result.network_process_signal.block_wire_verified)"
    $md += "- block_wire_total: $($result.network_process_signal.block_wire_total)"
    $md += "- block_wire_verified_ratio: $($result.network_process_signal.block_wire_verified_ratio)"
    $md += "- node_a_exit_code: $($result.network_process_signal.node_a_exit_code)"
    $md += "- node_b_exit_code: $($result.network_process_signal.node_b_exit_code)"
}

$md += ""
$md += "## State Root Consistency"
$md += ""
$md += "- available: $($stateRootConsistency.available)"
$md += "- method: $($stateRootConsistency.method)"
$md += "- pass: $($stateRootConsistency.pass)"
$md += "- proxy_digest: $($stateRootConsistency.proxy_digest)"
$md += "- reason: $($stateRootConsistency.reason)"

$md += ""
$md += "## Notes"
$md += ""
foreach ($n in $result.notes) {
    $md += "- $n"
}

if ($capabilitySnapshot) {
    $md += ""
    $md += "## Capability Snapshot"
    $md += ""
    $md += "- source_json: $($capabilitySnapshot.source_json)"
    $md += "- variant: $($capabilitySnapshot.variant)"
    $md += "- execute_ops_v2: $($capabilitySnapshot.execute_ops_v2)"
    $md += "- zkvm_prove: $($capabilitySnapshot.zkvm_prove)"
    $md += "- zkvm_verify: $($capabilitySnapshot.zkvm_verify)"
    $md += "- msm_accel: $($capabilitySnapshot.msm_accel)"
    $md += "- msm_backend: $($capabilitySnapshot.msm_backend)"
    $md += "- inferred_from_legacy_fields: $($capabilitySnapshot.inferred_from_legacy_fields)"
}

$md -join "`n" | Set-Content -Path $mdPath -Encoding UTF8

Write-Host "functional consistency report generated:"
Write-Host "  $jsonPath"
Write-Host "  $mdPath"
