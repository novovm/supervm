param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateSet("core", "persist", "wasm")]
    [string]$CapabilityVariant = "persist",
    [ValidateSet("evm", "polygon", "bnb", "avalanche")]
    [string]$AdapterChain = "evm",
    [ValidateRange(1, 1024)]
    [int]$DemoTxs = 8,
    [ValidateRange(1, 256)]
    [int]$BatchCount = 2,
    [ValidateRange(1, 1000000)]
    [int64]$FeeFloor = 1,
    [string]$PluginPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if (-not (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue)) {
    $IsWindows = ($env:OS -eq "Windows_NT")
}

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\evm"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Resolve-RepoPath {
    param(
        [string]$RepoRootValue,
        [string]$PathText
    )

    if (-not $PathText) {
        return ""
    }
    if (Test-Path $PathText) {
        return (Resolve-Path $PathText).Path
    }
    $repoRelative = Join-Path $RepoRootValue $PathText
    if (Test-Path $repoRelative) {
        return (Resolve-Path $repoRelative).Path
    }
    return $PathText
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
    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        output = ($stdout + $stderr).Trim()
    }
}

function Resolve-EvmPluginPath {
    param(
        [string]$RepoRootValue,
        [string]$PluginPathText,
        [bool]$AllowBuild
    )

    $explicit = Resolve-RepoPath -RepoRootValue $RepoRootValue -PathText $PluginPathText
    if ($explicit) {
        return $explicit
    }

    $pluginNames = if ($IsWindows) {
        @("novovm_adapter_evm_plugin.dll")
    } elseif ($IsMacOS) {
        @("libnovovm_adapter_evm_plugin.dylib", "novovm_adapter_evm_plugin.dylib")
    } else {
        @("libnovovm_adapter_evm_plugin.so", "novovm_adapter_evm_plugin.so")
    }

    $pluginCrateDir = Join-Path $RepoRootValue "crates\novovm-adapter-evm-plugin"
    $targetRoots = @(
        (Join-Path $RepoRootValue "target"),
        (Join-Path $pluginCrateDir "target")
    )
    if ($env:CARGO_TARGET_DIR) {
        $targetRoots += $env:CARGO_TARGET_DIR
    }
    $targetRoots = $targetRoots | Where-Object { $_ -and (Test-Path $_) } | Select-Object -Unique
    $searchDirs = @()
    foreach ($root in $targetRoots) {
        $searchDirs += (Join-Path $root "debug")
        $searchDirs += (Join-Path $root "release")
        $searchDirs += (Join-Path $root "debug\deps")
        $searchDirs += (Join-Path $root "release\deps")
    }
    $searchDirs = $searchDirs | Where-Object { Test-Path $_ } | Select-Object -Unique

    foreach ($dir in $searchDirs) {
        foreach ($name in $pluginNames) {
            $candidate = Join-Path $dir $name
            if (Test-Path $candidate) {
                return (Resolve-Path $candidate).Path
            }
        }
    }

    if (-not $AllowBuild) {
        return ""
    }
    $manifest = Join-Path $pluginCrateDir "Cargo.toml"
    if (-not (Test-Path $manifest)) {
        return ""
    }

    $buildResult = Invoke-CargoAllowFailure -WorkDir $RepoRootValue -CargoArgs @(
        "build",
        "--manifest-path",
        "crates/novovm-adapter-evm-plugin/Cargo.toml"
    ) -EnvVars @{}
    if ($buildResult.exit_code -ne 0) {
        throw "failed to build evm plugin: $($buildResult.output)"
    }

    foreach ($dir in $searchDirs) {
        foreach ($name in $pluginNames) {
            $candidate = Join-Path $dir $name
            if (Test-Path $candidate) {
                return (Resolve-Path $candidate).Path
            }
        }
    }
    return ""
}

function Parse-BoolToken {
    param([string]$Value)
    if (-not $Value) {
        return $false
    }
    $v = $Value.Trim().ToLowerInvariant()
    return ($v -eq "true" -or $v -eq "1")
}

function Parse-NodeReportLine {
    param([string]$Text)

    $m = [regex]::Match(
        $Text,
        "mode=ffi_v2 .*? rc=(?<rc>\d+)\([^)]+\) .*? processed=(?<processed>\d+) success=(?<success>\d+) writes=(?<writes>\d+) elapsed_us=(?<elapsed_us>\d+)",
        [System.Text.RegularExpressions.RegexOptions]::Multiline
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            rc = -1
            processed = 0
            success = 0
            writes = 0
            elapsed_us = 0
        }
    }

    return [ordered]@{
        parse_ok = $true
        rc = [int]$m.Groups["rc"].Value
        processed = [int]$m.Groups["processed"].Value
        success = [int]$m.Groups["success"].Value
        writes = [int]$m.Groups["writes"].Value
        elapsed_us = [int64]$m.Groups["elapsed_us"].Value
    }
}

function Parse-AdapterOutLine {
    param([string]$Text)

    $m = [regex]::Match(
        $Text,
        "adapter_out:\s+backend=(?<backend>\S+)\s+chain=(?<chain>\S+)\s+txs=(?<txs>\d+)\s+verified=(?<verified>\S+)\s+applied=(?<applied>\S+)\s+accounts=(?<accounts>\d+)\s+state_root=(?<state_root>[0-9a-fA-F]+)",
        [System.Text.RegularExpressions.RegexOptions]::Multiline
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            backend = ""
            chain = ""
            txs = 0
            verified = $false
            applied = $false
            accounts = 0
            state_root = ""
        }
    }

    return [ordered]@{
        parse_ok = $true
        backend = $m.Groups["backend"].Value.ToLowerInvariant()
        chain = $m.Groups["chain"].Value.ToLowerInvariant()
        txs = [int]$m.Groups["txs"].Value
        verified = Parse-BoolToken $m.Groups["verified"].Value
        applied = Parse-BoolToken $m.Groups["applied"].Value
        accounts = [int]$m.Groups["accounts"].Value
        state_root = $m.Groups["state_root"].Value.ToLowerInvariant()
    }
}

function Parse-AdapterPluginAbiLine {
    param([string]$Text)

    $m = [regex]::Match(
        $Text,
        "adapter_plugin_abi:\s+enabled=(?<enabled>\S+)\s+version=(?<version>\d+)\s+expected=(?<expected>\d+)\s+caps=(?<caps>0x[0-9a-fA-F]+)\s+required=(?<required>0x[0-9a-fA-F]+)\s+compatible=(?<compatible>\S+)",
        [System.Text.RegularExpressions.RegexOptions]::Multiline
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            enabled = $false
            version = 0
            expected = 0
            caps = "0x0"
            required = "0x0"
            compatible = $false
        }
    }

    return [ordered]@{
        parse_ok = $true
        enabled = Parse-BoolToken $m.Groups["enabled"].Value
        version = [int]$m.Groups["version"].Value
        expected = [int]$m.Groups["expected"].Value
        caps = $m.Groups["caps"].Value.ToLowerInvariant()
        required = $m.Groups["required"].Value.ToLowerInvariant()
        compatible = Parse-BoolToken $m.Groups["compatible"].Value
    }
}

function Parse-AdapterPluginRegistryLine {
    param([string]$Text)

    $m = [regex]::Match(
        $Text,
        "adapter_plugin_registry:\s+enabled=(?<enabled>\S+)\s+strict=(?<strict>\S+)\s+matched=(?<matched>\S+)\s+chain_allowed=(?<chain_allowed>\S+)\s+entry_abi=(?<entry_abi>\d+)\s+entry_required=(?<entry_required>0x[0-9a-fA-F]+)\s+hash_check=(?<hash_check>\S+)\s+hash_match=(?<hash_match>\S+)\s+abi_whitelist=(?<abi_whitelist>\S+)\s+abi_allowed=(?<abi_allowed>\S+)",
        [System.Text.RegularExpressions.RegexOptions]::Multiline
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            enabled = $false
            strict = $false
            matched = $false
            chain_allowed = $false
            entry_abi = 0
            entry_required = "0x0"
            hash_check = $false
            hash_match = $false
            abi_whitelist = $false
            abi_allowed = $false
        }
    }

    return [ordered]@{
        parse_ok = $true
        enabled = Parse-BoolToken $m.Groups["enabled"].Value
        strict = Parse-BoolToken $m.Groups["strict"].Value
        matched = Parse-BoolToken $m.Groups["matched"].Value
        chain_allowed = Parse-BoolToken $m.Groups["chain_allowed"].Value
        entry_abi = [int]$m.Groups["entry_abi"].Value
        entry_required = $m.Groups["entry_required"].Value.ToLowerInvariant()
        hash_check = Parse-BoolToken $m.Groups["hash_check"].Value
        hash_match = Parse-BoolToken $m.Groups["hash_match"].Value
        abi_whitelist = Parse-BoolToken $m.Groups["abi_whitelist"].Value
        abi_allowed = Parse-BoolToken $m.Groups["abi_allowed"].Value
    }
}

function Resolve-BackendCompareStateBase {
    param(
        [string]$RepoRootValue,
        [string]$OutputDirValue
    )

    $overrideRoot = ""
    if ($env:NOVOVM_EVM_BACKEND_COMPARE_STATE_ROOT) {
        $overrideRoot = Resolve-RepoPath -RepoRootValue $RepoRootValue -PathText $env:NOVOVM_EVM_BACKEND_COMPARE_STATE_ROOT
    }
    if ($overrideRoot) {
        return $overrideRoot
    }

    if ($IsWindows) {
        # Keep RocksDB path short on Windows to avoid MAX_PATH-related open failures.
        return (Join-Path $RepoRootValue "artifacts\migration\evm\backend-compare-state")
    }

    return (Join-Path $OutputDirValue "backend-compare-state")
}

$resolvedPluginPath = Resolve-EvmPluginPath -RepoRootValue $RepoRoot -PluginPathText $PluginPath -AllowBuild $true
if (-not $resolvedPluginPath) {
    throw "unable to resolve EVM plugin path; pass -PluginPath"
}
if (-not (Test-Path $resolvedPluginPath)) {
    throw "evm plugin path not found: $resolvedPluginPath"
}

$nodeDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not (Test-Path (Join-Path $nodeDir "Cargo.toml"))) {
    throw "missing novovm-node crate: $nodeDir"
}

$registryPath = Join-Path $RepoRoot "config\novovm-adapter-plugin-registry.json"
$registryPathResolved = ""
if (Test-Path $registryPath) {
    $registryPathResolved = (Resolve-Path $registryPath).Path
}

$backendCompareStateBase = Resolve-BackendCompareStateBase -RepoRootValue $RepoRoot -OutputDirValue $OutputDir
New-Item -ItemType Directory -Force -Path $backendCompareStateBase | Out-Null

function Invoke-BackendRun {
    param(
        [string]$Backend,
        [string]$BackendPluginPath,
        [string]$StorageSuffix
    )

    $persistRoot = Join-Path $backendCompareStateBase ("{0}-{1}" -f $AdapterChain, $StorageSuffix)
    if (Test-Path $persistRoot) {
        Remove-Item -Path $persistRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $persistRoot | Out-Null
    $envVars = @{
        NOVOVM_EXEC_PATH = "ffi_v2"
        NOVOVM_AOEM_VARIANT = "$CapabilityVariant"
        NOVOVM_DEMO_TXS = "$DemoTxs"
        NOVOVM_BATCH_A_BATCHES = "$BatchCount"
        NOVOVM_MEMPOOL_FEE_FLOOR = "$FeeFloor"
        NOVOVM_ADAPTER_BACKEND = "$Backend"
        NOVOVM_ADAPTER_PLUGIN_PATH = "$BackendPluginPath"
        NOVOVM_ADAPTER_CHAIN = "$AdapterChain"
        NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI = "1"
        NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS = "0x1"
        NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH = "$registryPathResolved"
        NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "0"
        NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256 = ""
        NOVOVM_D2D3_STORAGE_ROOT = "$persistRoot"
    }

    $result = Invoke-CargoAllowFailure -WorkDir $nodeDir -CargoArgs @("run", "--quiet") -EnvVars $envVars
    $logPath = Join-Path $OutputDir ("backend_compare_" + $Backend + ".log")
    Set-Content -Path $logPath -Encoding UTF8 -Value $result.output
    return [ordered]@{
        exit_code = $result.exit_code
        output = $result.output
        log_path = $logPath
        node = Parse-NodeReportLine -Text $result.output
        adapter = Parse-AdapterOutLine -Text $result.output
        plugin_abi = Parse-AdapterPluginAbiLine -Text $result.output
        registry = Parse-AdapterPluginRegistryLine -Text $result.output
    }
}

$native = Invoke-BackendRun -Backend "native" -BackendPluginPath "" -StorageSuffix "native"
$plugin = Invoke-BackendRun -Backend "plugin" -BackendPluginPath $resolvedPluginPath -StorageSuffix "plugin"

$expectedChain = $AdapterChain.ToLowerInvariant()
$expectedRequiredCaps = "0x1"
$available = (
    $native.node.parse_ok -and
    $plugin.node.parse_ok -and
    $native.adapter.parse_ok -and
    $plugin.adapter.parse_ok -and
    $native.plugin_abi.parse_ok -and
    $plugin.plugin_abi.parse_ok -and
    $native.registry.parse_ok -and
    $plugin.registry.parse_ok
)
$stateRootEqual = $available -and ($native.adapter.state_root -eq $plugin.adapter.state_root)

$pass = $false
if ($available) {
    $pass = (
        $native.exit_code -eq 0 -and
        $plugin.exit_code -eq 0 -and
        $native.node.rc -eq 0 -and
        $plugin.node.rc -eq 0 -and
        $native.adapter.backend -eq "native" -and
        $plugin.adapter.backend -eq "plugin" -and
        $native.adapter.chain -eq $expectedChain -and
        $plugin.adapter.chain -eq $expectedChain -and
        $native.adapter.txs -eq $plugin.adapter.txs -and
        $native.adapter.txs -eq $DemoTxs -and
        $native.adapter.accounts -eq $plugin.adapter.accounts -and
        $native.adapter.verified -and
        $plugin.adapter.verified -and
        $native.adapter.applied -and
        $plugin.adapter.applied -and
        (-not $native.plugin_abi.enabled) -and
        $plugin.plugin_abi.enabled -and
        $native.plugin_abi.expected -eq 1 -and
        $plugin.plugin_abi.expected -eq 1 -and
        $native.plugin_abi.required -eq $expectedRequiredCaps -and
        $plugin.plugin_abi.required -eq $expectedRequiredCaps -and
        $native.plugin_abi.compatible -and
        $plugin.plugin_abi.compatible -and
        $native.registry.enabled -and
        $plugin.registry.enabled -and
        $native.registry.entry_abi -eq 1 -and
        $plugin.registry.entry_abi -eq 1 -and
        $native.registry.entry_required -eq $expectedRequiredCaps -and
        $plugin.registry.entry_required -eq $expectedRequiredCaps -and
        $native.registry.matched -and
        $plugin.registry.matched -and
        $native.registry.chain_allowed -and
        $plugin.registry.chain_allowed -and
        $stateRootEqual -and
        $native.node.processed -eq $plugin.node.processed -and
        $native.node.success -eq $plugin.node.success -and
        $native.node.writes -eq $plugin.node.writes
    )
}

$reason = $null
if (-not $available) {
    $reason = "backend compare parse failure (missing node/adapter/plugin_abi/registry line)"
} elseif (-not $pass) {
    $reason = "backend compare checks failed (see native/plugin logs)"
}

$signal = [ordered]@{
    signal = "evm_backend_compare_signal"
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    pass = $pass
    available = $available
    compared_mode = "ffi_v2"
    expected_chain = $expectedChain
    expected_txs = $DemoTxs
    plugin_path = $resolvedPluginPath
    state_root_equal = $stateRootEqual
    native = $native
    plugin = $plugin
    reason = $reason
}

$jsonPath = Join-Path $OutputDir "backend_compare_signal.json"
$signal | ConvertTo-Json -Depth 12 | Set-Content -Path $jsonPath -Encoding UTF8

Write-Host "evm_backend_compare_signal_out: pass=$pass path=$jsonPath plugin=$resolvedPluginPath"
if (-not $pass) {
    throw "evm backend compare signal failed"
}
