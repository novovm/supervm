param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [int]$Rounds = 200,
    [int]$Points = 1024,
    [int]$KeySpace = 251,
    [double]$Rw = 0.5,
    [int]$Seed = 123,
    [bool]$IncludeCapabilitySnapshot = $true,
    [bool]$IncludeCoordinatorSignal = $true,
    [bool]$IncludeCoordinatorNegativeSignal = $true,
    [bool]$IncludeProverContractSignal = $true,
    [bool]$IncludeProverContractNegativeSignal = $true,
    [bool]$IncludeConsensusNegativeSignal = $true,
    [bool]$IncludeNetworkProcessSignal = $false,
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
if (-not (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue)) {
    $IsWindows = ($env:OS -eq "Windows_NT")
}
if (-not (Get-Variable -Name IsMacOS -ErrorAction SilentlyContinue)) {
    try {
        $IsMacOS = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
            [System.Runtime.InteropServices.OSPlatform]::OSX
        )
    } catch {
        $IsMacOS = $false
    }
}
if (-not (Get-Variable -Name IsLinux -ErrorAction SilentlyContinue)) {
    try {
        $IsLinux = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
            [System.Runtime.InteropServices.OSPlatform]::Linux
        )
    } catch {
        $IsLinux = $false
    }
}

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\functional"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
$script:functionalNodePersistSeq = 0
$script:functionalNodePersistBase = $null
$script:functionalNodePersistSession = ([Guid]::NewGuid().ToString("N")).Substring(0, 8)

function Get-OutputDirHash {
    param([Parameter(Mandatory=$true)][string]$Value)

    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
        $hash = $sha.ComputeHash($bytes)
        return ([System.BitConverter]::ToString($hash) -replace "-", "").ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Resolve-FunctionalNodePersistBase {
    if ($script:functionalNodePersistBase) {
        return $script:functionalNodePersistBase
    }

    $tmpRoot = [System.IO.Path]::GetTempPath()
    $hash = Get-OutputDirHash -Value $OutputDir
    $bucket = $hash.Substring(0, 12)
    $base = Join-Path $tmpRoot ("novovm-d2d3-" + $bucket + "-" + $script:functionalNodePersistSession)
    New-Item -ItemType Directory -Force -Path $base | Out-Null
    $script:functionalNodePersistBase = [System.IO.Path]::GetFullPath($base)
    return $script:functionalNodePersistBase
}

function Resolve-FunctionalNodePersistRoot {
    param(
        [string]$WorkDir,
        [hashtable]$EnvVars
    )

    if ((Split-Path -Leaf $WorkDir) -ne "novovm-node") {
        return $null
    }
    if ($null -ne $EnvVars -and $EnvVars.ContainsKey("NOVOVM_D2D3_STORAGE_ROOT")) {
        return [string]$EnvVars["NOVOVM_D2D3_STORAGE_ROOT"]
    }

    $script:functionalNodePersistSeq += 1
    $base = Resolve-FunctionalNodePersistBase
    $root = Join-Path $base ("run-{0:D3}" -f $script:functionalNodePersistSeq)
    New-Item -ItemType Directory -Force -Path $root | Out-Null
    return $root
}

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
    $nodePersistRoot = Resolve-FunctionalNodePersistRoot -WorkDir $WorkDir -EnvVars $EnvVars
    if ($nodePersistRoot) {
        $psi.Environment["NOVOVM_D2D3_STORAGE_ROOT"] = [string]$nodePersistRoot
    }
    if ((Split-Path -Leaf $WorkDir) -eq "novovm-node" -and -not $EnvVars.ContainsKey("NOVOVM_NODE_VERBOSE")) {
        $psi.Environment["NOVOVM_NODE_VERBOSE"] = "1"
    }

    foreach ($ingressKey in @("NOVOVM_TX_WIRE_FILE", "NOVOVM_OPS_WIRE_FILE", "NOVOVM_OPS_WIRE_DIR")) {
        if ($psi.Environment.ContainsKey($ingressKey)) {
            $psi.Environment.Remove($ingressKey)
        }
    }

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
    $nodePersistRoot = Resolve-FunctionalNodePersistRoot -WorkDir $WorkDir -EnvVars $EnvVars
    if ($nodePersistRoot) {
        $psi.Environment["NOVOVM_D2D3_STORAGE_ROOT"] = [string]$nodePersistRoot
    }
    if ((Split-Path -Leaf $WorkDir) -eq "novovm-node" -and -not $EnvVars.ContainsKey("NOVOVM_NODE_VERBOSE")) {
        $psi.Environment["NOVOVM_NODE_VERBOSE"] = "1"
    }

    foreach ($ingressKey in @("NOVOVM_TX_WIRE_FILE", "NOVOVM_OPS_WIRE_FILE", "NOVOVM_OPS_WIRE_DIR")) {
        if ($psi.Environment.ContainsKey($ingressKey)) {
            $psi.Environment.Remove($ingressKey)
        }
    }

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

function Resolve-RepoPath {
    param(
        [string]$RepoRoot,
        [string]$PathText
    )

    if (-not $PathText) {
        return ""
    }
    if (Test-Path $PathText) {
        return (Resolve-Path $PathText).Path
    }
    $repoRelative = Join-Path $RepoRoot $PathText
    if (Test-Path $repoRelative) {
        return (Resolve-Path $repoRelative).Path
    }
    return $PathText
}

function Get-MigrationPowerShellHost {
    $pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($pwsh) {
        return $pwsh.Source
    }
    $windowsPs = Get-Command powershell -ErrorAction SilentlyContinue
    if ($windowsPs) {
        return $windowsPs.Source
    }
    throw "missing PowerShell host: requires pwsh or powershell in PATH"
}

function Invoke-MigrationPowerShellScript {
    param(
        [Parameter(Mandatory=$true)][string]$ScriptPath,
        [hashtable]$Arguments = @{}
    )

    $hostExe = Get-MigrationPowerShellHost
    $argList = @()
    if ((Split-Path -Leaf $hostExe).ToLowerInvariant() -eq "powershell.exe") {
        $argList += @("-ExecutionPolicy", "Bypass")
    }
    $argList += @("-File", $ScriptPath)
    foreach ($k in $Arguments.Keys) {
        $argList += "-$k"
        $argList += [string]$Arguments[$k]
    }
    & $hostExe @argList | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "migration script failed (exit=$LASTEXITCODE): $ScriptPath"
    }
}

function Resolve-AdapterPluginPath {
    param(
        [string]$RepoRoot,
        [string]$PathText,
        [bool]$AllowBuild = $false
    )

    $resolvedExplicitPath = Resolve-RepoPath -RepoRoot $RepoRoot -PathText $PathText
    if ($resolvedExplicitPath) {
        return $resolvedExplicitPath
    }

    $pluginNames = if ($IsWindows) {
        @("novovm_adapter_sample_plugin.dll")
    } elseif ($IsMacOS) {
        @("libnovovm_adapter_sample_plugin.dylib", "novovm_adapter_sample_plugin.dylib")
    } else {
        @("libnovovm_adapter_sample_plugin.so", "novovm_adapter_sample_plugin.so")
    }
    $pluginCrateDir = Join-Path $RepoRoot "crates\novovm-adapter-sample-plugin"
    $searchDirs = @(
        (Join-Path $RepoRoot "target\debug"),
        (Join-Path $RepoRoot "target\release"),
        (Join-Path $pluginCrateDir "target\debug"),
        (Join-Path $pluginCrateDir "target\release")
    )

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
    if (-not (Test-Path (Join-Path $pluginCrateDir "Cargo.toml"))) {
        return ""
    }

    $buildProbe = Invoke-CargoAllowFailure -WorkDir $pluginCrateDir -CargoArgs @("build", "--quiet") -EnvVars @{}
    if ($buildProbe.exit_code -ne 0) {
        Write-Warning "failed to build sample adapter plugin (exit=$($buildProbe.exit_code))"
        return ""
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

function Parse-NodeReportLine {
    param([string]$Text)
    $lines = $Text -split "`r?`n"
    $legacyLine = ($lines | Where-Object { $_ -match "^mode=ffi_v2 variant=" } | Select-Object -Last 1)
    if ($legacyLine) {
        $m = [regex]::Match(
            $legacyLine,
            "^mode=ffi_v2 variant=(?<variant>\w+) dll=(?<dll>.+?) rc=(?<rc>\d+)\((?<rc_name>[^)]+)\) submitted=(?<submitted>\d+) processed=(?<processed>\d+) success=(?<success>\d+) writes=(?<writes>\d+) elapsed_us=(?<elapsed>\d+)$"
        )
        if (-not $m.Success) {
            throw "cannot parse novovm-node report line: $legacyLine"
        }
        return [ordered]@{
            report_format = "legacy"
            variant   = $m.Groups["variant"].Value
            dll       = $m.Groups["dll"].Value
            rc        = [int]$m.Groups["rc"].Value
            rc_name   = $m.Groups["rc_name"].Value
            submitted = [int]$m.Groups["submitted"].Value
            processed = [int]$m.Groups["processed"].Value
            success   = [int]$m.Groups["success"].Value
            writes    = [int64]$m.Groups["writes"].Value
            elapsed_us = [int64]$m.Groups["elapsed"].Value
            host_elapsed_us = $null
            batches = $null
            repeats = $null
        }
    }

    $aggregateLine = ($lines | Where-Object { $_ -match "^mode=ffi_v2_aggregate variant=" } | Select-Object -Last 1)
    if (-not $aggregateLine) {
        throw "novovm-node output missing final report line"
    }
    $mAgg = [regex]::Match(
        $aggregateLine,
        "^mode=ffi_v2_aggregate variant=(?<variant>\w+) dll=(?<dll>.+?) rc=(?<rc>\d+)\((?<rc_name>[^)]+)\) batches=(?<batches>\d+) repeats=(?<repeats>\d+) submitted_total=(?<submitted>\d+) processed_total=(?<processed>\d+) success_total=(?<success>\d+) writes_total=(?<writes>\d+) host_exec_us=(?<host_elapsed>\d+) aoem_exec_us=(?<aoem_elapsed>\d+)$"
    )
    if (-not $mAgg.Success) {
        throw "cannot parse novovm-node aggregate report line: $aggregateLine"
    }
    return [ordered]@{
        report_format = "aggregate"
        variant   = $mAgg.Groups["variant"].Value
        dll       = $mAgg.Groups["dll"].Value
        rc        = [int]$mAgg.Groups["rc"].Value
        rc_name   = $mAgg.Groups["rc_name"].Value
        submitted = [int]$mAgg.Groups["submitted"].Value
        processed = [int]$mAgg.Groups["processed"].Value
        success   = [int]$mAgg.Groups["success"].Value
        writes    = [int64]$mAgg.Groups["writes"].Value
        elapsed_us = [int64]$mAgg.Groups["aoem_elapsed"].Value
        host_elapsed_us = [int64]$mAgg.Groups["host_elapsed"].Value
        batches = [int]$mAgg.Groups["batches"].Value
        repeats = [int]$mAgg.Groups["repeats"].Value
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
        "^block_out:\s+height=(?<height>\d+)\s+epoch=(?<epoch>\d+)\s+batches=(?<batches>\d+)\s+txs=(?<txs>\d+)\s+block_hash=(?<block_hash>[0-9a-f]+)\s+state_root=(?<state_root>[0-9a-f]+)(?:\s+governance_chain_audit_root=(?<governance_chain_audit_root>[0-9a-f]+))?\s+proposal_hash=(?<proposal_hash>[0-9a-f]+)$"
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
        governance_chain_audit_root = $m.Groups["governance_chain_audit_root"].Value
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
        "^commit_out:\s+store=(?<store>\w+)\s+committed=(?<committed>true|false)\s+height=(?<height>\d+)\s+total_blocks=(?<total_blocks>\d+)\s+block_hash=(?<block_hash>[0-9a-f]+)\s+state_root=(?<state_root>[0-9a-f]+)(?:\s+governance_chain_audit_root=(?<governance_chain_audit_root>[0-9a-f]+))?$"
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
        governance_chain_audit_root = $m.Groups["governance_chain_audit_root"].Value
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

function Parse-NetworkPacemakerLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^network_pacemaker:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }
    $m = [regex]::Match(
        $line,
        "^network_pacemaker:\s+view_sync=(?<view_sync>true|false)\s+new_view=(?<new_view>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }
    return [ordered]@{
        parse_ok = $true
        view_sync = [bool]::Parse($m.Groups["view_sync"].Value)
        new_view = [bool]::Parse($m.Groups["new_view"].Value)
        raw = $line
    }
}

function Parse-CoordinatorOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^coordinator_out:" } | Select-Object -Last 1)
    if (-not $line) {
        throw "coordinator output missing line"
    }
    $m = [regex]::Match(
        $line,
        "^coordinator_out:\s+tx_id=(?<tx_id>\d+)\s+participants=(?<participants>\d+)\s+votes=(?<votes>\d+)\s+decided=(?<decided>true|false)\s+commit=(?<commit>true|false)$"
    )
    if (-not $m.Success) {
        throw "cannot parse coordinator line: $line"
    }
    return [ordered]@{
        parse_ok = $true
        tx_id = [int64]$m.Groups["tx_id"].Value
        participants = [int]$m.Groups["participants"].Value
        votes = [int]$m.Groups["votes"].Value
        decided = [bool]::Parse($m.Groups["decided"].Value)
        commit = [bool]::Parse($m.Groups["commit"].Value)
        raw = $line
    }
}

function Parse-CoordinatorNegativeOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^coordinator_negative_out:" } | Select-Object -Last 1)
    if (-not $line) {
        throw "coordinator negative output missing line"
    }
    $m = [regex]::Match(
        $line,
        "^coordinator_negative_out:\s+unknown_prepare=(?<unknown_prepare>true|false)\s+non_participant_vote=(?<non_participant_vote>true|false)\s+vote_after_decide=(?<vote_after_decide>true|false)\s+duplicate_tx=(?<duplicate_tx>true|false)\s+pass=(?<pass>true|false)$"
    )
    if (-not $m.Success) {
        throw "cannot parse coordinator negative line: $line"
    }
    return [ordered]@{
        parse_ok = $true
        unknown_prepare = [bool]::Parse($m.Groups["unknown_prepare"].Value)
        non_participant_vote = [bool]::Parse($m.Groups["non_participant_vote"].Value)
        vote_after_decide = [bool]::Parse($m.Groups["vote_after_decide"].Value)
        duplicate_tx = [bool]::Parse($m.Groups["duplicate_tx"].Value)
        pass = [bool]::Parse($m.Groups["pass"].Value)
        raw = $line
    }
}

function Parse-ProverContractOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^prover_contract_out:" } | Select-Object -Last 1)
    if (-not $line) {
        throw "prover contract output missing line"
    }
    $m = [regex]::Match(
        $line,
        "^prover_contract_out:\s+schema_ok=(?<schema_ok>true|false)\s+normalized_reason_codes=(?<normalized>true|false)\s+fallback_codes=(?<fallback_codes>\d+)\s+prover_ready=(?<prover_ready>true|false)\s+zk_ready=(?<zk_ready>true|false)\s+msm_backend=(?<msm_backend>.*)$"
    )
    if (-not $m.Success) {
        throw "cannot parse prover contract line: $line"
    }
    return [ordered]@{
        parse_ok = $true
        schema_ok = [bool]::Parse($m.Groups["schema_ok"].Value)
        normalized_reason_codes = [bool]::Parse($m.Groups["normalized"].Value)
        fallback_codes = [int]$m.Groups["fallback_codes"].Value
        prover_ready = [bool]::Parse($m.Groups["prover_ready"].Value)
        zk_ready = [bool]::Parse($m.Groups["zk_ready"].Value)
        msm_backend = $m.Groups["msm_backend"].Value.Trim()
        raw = $line
    }
}

function Parse-ProverContractNegativeOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^prover_contract_negative_out:" } | Select-Object -Last 1)
    if (-not $line) {
        throw "prover contract negative output missing line"
    }
    $m = [regex]::Match(
        $line,
        "^prover_contract_negative_out:\s+missing_formal_fields=(?<missing_formal_fields>true|false)\s+empty_reason_codes=(?<empty_reason_codes>true|false)\s+reason_normalization_stable=(?<reason_normalization_stable>true|false)\s+pass=(?<pass>true|false)$"
    )
    if (-not $m.Success) {
        throw "cannot parse prover contract negative line: $line"
    }
    return [ordered]@{
        parse_ok = $true
        missing_formal_fields = [bool]::Parse($m.Groups["missing_formal_fields"].Value)
        empty_reason_codes = [bool]::Parse($m.Groups["empty_reason_codes"].Value)
        reason_normalization_stable = [bool]::Parse($m.Groups["reason_normalization_stable"].Value)
        pass = [bool]::Parse($m.Groups["pass"].Value)
        raw = $line
    }
}

function Parse-ConsensusNegativeOutLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^consensus_negative_out:" } | Select-Object -Last 1)
    if (-not $line) {
        throw "consensus negative output missing line"
    }
    $m = [regex]::Match(
        $line,
        "^consensus_negative_out:\s+invalid_signature=(?<invalid_signature>true|false)\s+duplicate_vote=(?<duplicate_vote>true|false)\s+wrong_epoch=(?<wrong_epoch>true|false)\s+pass=(?<pass>true|false)$"
    )
    if (-not $m.Success) {
        throw "cannot parse consensus negative line: $line"
    }
    return [ordered]@{
        parse_ok = $true
        invalid_signature = [bool]::Parse($m.Groups["invalid_signature"].Value)
        duplicate_vote = [bool]::Parse($m.Groups["duplicate_vote"].Value)
        wrong_epoch = [bool]::Parse($m.Groups["wrong_epoch"].Value)
        pass = [bool]::Parse($m.Groups["pass"].Value)
        raw = $line
    }
}

function Parse-ConsensusNegativeExtLine {
    param([string]$Text)
    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^consensus_negative_ext:" } | Select-Object -Last 1)
    if (-not $line) {
        throw "consensus negative ext output missing line"
    }
    $m = [regex]::Match(
        $line,
        "^consensus_negative_ext:\s+weighted_quorum=(?<weighted_quorum>true|false)\s+equivocation=(?<equivocation>true|false)(?:\s+slash_execution=(?<slash_execution>true|false))?(?:\s+slash_threshold=(?<slash_threshold>true|false))?(?:\s+slash_observe_only=(?<slash_observe_only>true|false))?(?:\s+unjail_cooldown=(?<unjail_cooldown>true|false))?\s+view_change=(?<view_change>true|false)\s+fork_choice=(?<fork_choice>true|false)$"
    )
    if (-not $m.Success) {
        throw "cannot parse consensus negative ext line: $line"
    }
    return [ordered]@{
        parse_ok = $true
        weighted_quorum = [bool]::Parse($m.Groups["weighted_quorum"].Value)
        equivocation = [bool]::Parse($m.Groups["equivocation"].Value)
        slash_execution = if ($m.Groups["slash_execution"].Success) { [bool]::Parse($m.Groups["slash_execution"].Value) } else { $false }
        slash_threshold = if ($m.Groups["slash_threshold"].Success) { [bool]::Parse($m.Groups["slash_threshold"].Value) } else { $false }
        slash_observe_only = if ($m.Groups["slash_observe_only"].Success) { [bool]::Parse($m.Groups["slash_observe_only"].Value) } else { $false }
        unjail_cooldown = if ($m.Groups["unjail_cooldown"].Success) { [bool]::Parse($m.Groups["unjail_cooldown"].Value) } else { $false }
        view_change = [bool]::Parse($m.Groups["view_change"].Value)
        fork_choice = [bool]::Parse($m.Groups["fork_choice"].Value)
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

function Get-DynlibNameCandidates {
    if ($IsWindows) {
        return @("aoem_ffi.dll")
    }
    if ($IsMacOS) {
        return @("libaoem_ffi.dylib")
    }
    return @("libaoem_ffi.so")
}

function Get-AoemVariantBinDir {
    param([string]$AoemRoot, [string]$Variant)
    switch ($Variant) {
        "core" { return Join-Path $AoemRoot "bin" }
        "persist" { return Join-Path $AoemRoot "variants\persist\bin" }
        "wasm" { return Join-Path $AoemRoot "variants\wasm\bin" }
        default { throw "invalid variant: $Variant" }
    }
}

function Get-DllPathForVariant {
    param(
        [string]$AoemRoot,
        [string]$Variant,
        [bool]$RequireExists = $false
    )

    $binDir = Get-AoemVariantBinDir -AoemRoot $AoemRoot -Variant $Variant
    $candidates = Get-DynlibNameCandidates
    foreach ($name in $candidates) {
        $candidate = Join-Path $binDir $name
        if (Test-Path $candidate) {
            return (Resolve-Path $candidate).Path
        }
    }

    $fallback = Join-Path $binDir $candidates[0]
    if ($RequireExists) {
        throw "aoem dynlib not found for variant=$Variant under $binDir (tried: $($candidates -join ', '))"
    }
    return $fallback
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

        Invoke-MigrationPowerShellScript -ScriptPath $scriptPath -Arguments @{
            RepoRoot = $RepoRoot
            OutputDir = $capOutputDir
            Variant = $Variant
        }
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
    $fallbackReason = ""
    if ($null -ne $raw.contract.fallback_reason) {
        $fallbackReason = [string]$raw.contract.fallback_reason
    }
    $zkFormalFieldsPresent = $false
    if ($null -ne $raw.contract.zk_formal_fields_present) {
        $zkFormalFieldsPresent = [bool]$raw.contract.zk_formal_fields_present
    }
    $proverReady = $false
    if ($raw.prover_contract -and $null -ne $raw.prover_contract.prover_ready) {
        $proverReady = [bool]$raw.prover_contract.prover_ready
    }

    return [ordered]@{
        source_json = $sourceJson
        generated_at_utc = [string]$raw.generated_at_utc
        variant = [string]$raw.variant
        execute_ops_v2 = [bool]$raw.contract.execute_ops_v2
        zkvm_prove = [bool]$raw.contract.zkvm_prove
        zkvm_verify = [bool]$raw.contract.zkvm_verify
        zk_formal_fields_present = $zkFormalFieldsPresent
        msm_accel = [bool]$raw.contract.msm_accel
        msm_backend = [string]$raw.contract.msm_backend
        fallback_reason = $fallbackReason
        fallback_reason_codes = $fallbackCodes
        prover_ready = $proverReady
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
            view_sync_available = $false
            view_sync_pass = $false
            view_sync_rounds_passed = 0
            view_sync_pass_ratio = 0.0
            new_view_available = $false
            new_view_pass = $false
            new_view_rounds_passed = 0
            new_view_pass_ratio = 0.0
            reason = "missing script: $scriptPath"
        }
    }

        $probeDir = Join-Path $RepoRoot "artifacts\migration\network-two-process"
        New-Item -ItemType Directory -Force -Path $probeDir | Out-Null
        try {
            Invoke-MigrationPowerShellScript -ScriptPath $scriptPath -Arguments @{
                RepoRoot = $RepoRoot
                OutputDir = $probeDir
                NodeCount = $NodeCount
                Rounds = $ProbeRounds
            }
        } catch {
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
                view_sync_available = $false
                view_sync_pass = $false
                view_sync_rounds_passed = 0
                view_sync_pass_ratio = 0.0
                new_view_available = $false
                new_view_pass = $false
                new_view_rounds_passed = 0
                new_view_pass_ratio = 0.0
                reason = "network two-process probe disabled or failed: $($_.Exception.Message)"
            }
        }
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
            view_sync_available = $false
            view_sync_pass = $false
            view_sync_rounds_passed = 0
            view_sync_pass_ratio = 0.0
            new_view_available = $false
            new_view_pass = $false
            new_view_rounds_passed = 0
            new_view_pass_ratio = 0.0
            reason = "network two-process json not found"
        }
    }

    $raw = Get-Content -Path $sourceJson -Raw | ConvertFrom-Json
    $hasViewSyncAvailable = $raw.PSObject.Properties.Name -contains "view_sync_available"
    $hasViewSyncPass = $raw.PSObject.Properties.Name -contains "view_sync_pass"
    $hasViewSyncRoundsPassed = $raw.PSObject.Properties.Name -contains "view_sync_rounds_passed"
    $hasViewSyncPassRatio = $raw.PSObject.Properties.Name -contains "view_sync_pass_ratio"
    $hasNewViewAvailable = $raw.PSObject.Properties.Name -contains "new_view_available"
    $hasNewViewPass = $raw.PSObject.Properties.Name -contains "new_view_pass"
    $hasNewViewRoundsPassed = $raw.PSObject.Properties.Name -contains "new_view_rounds_passed"
    $hasNewViewPassRatio = $raw.PSObject.Properties.Name -contains "new_view_pass_ratio"

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
        view_sync_available = if ($hasViewSyncAvailable -and $null -ne $raw.view_sync_available) { [bool]$raw.view_sync_available } else { $false }
        view_sync_pass = if ($hasViewSyncPass -and $null -ne $raw.view_sync_pass) { [bool]$raw.view_sync_pass } else { $false }
        view_sync_rounds_passed = if ($hasViewSyncRoundsPassed -and $null -ne $raw.view_sync_rounds_passed) { [int]$raw.view_sync_rounds_passed } else { 0 }
        view_sync_pass_ratio = if ($hasViewSyncPassRatio -and $null -ne $raw.view_sync_pass_ratio) { [double]$raw.view_sync_pass_ratio } else { 0.0 }
        new_view_available = if ($hasNewViewAvailable -and $null -ne $raw.new_view_available) { [bool]$raw.new_view_available } else { $false }
        new_view_pass = if ($hasNewViewPass -and $null -ne $raw.new_view_pass) { [bool]$raw.new_view_pass } else { $false }
        new_view_rounds_passed = if ($hasNewViewRoundsPassed -and $null -ne $raw.new_view_rounds_passed) { [int]$raw.new_view_rounds_passed } else { 0 }
        new_view_pass_ratio = if ($hasNewViewPassRatio -and $null -ne $raw.new_view_pass_ratio) { [double]$raw.new_view_pass_ratio } else { 0.0 }
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
    try {
        Invoke-MigrationPowerShellScript -ScriptPath $scriptPath -Arguments @{
            RepoRoot = $RepoRoot
            OutputDir = $probeDir
            NodeCount = 2
            Rounds = 1
            ProbeMode = "mesh"
            TamperBlockWireMode = "hash_mismatch"
        }
    } catch {
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
            reason = "negative network probe disabled or failed: $($_.Exception.Message)"
        }
    }

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
$benchDir = Join-Path $RepoRoot "crates\novovm-bench"
$bindingsDir = Join-Path $RepoRoot "crates\aoem-bindings"
$coordinatorDir = Join-Path $RepoRoot "crates\novovm-coordinator"
$proverDir = Join-Path $RepoRoot "crates\novovm-prover"
$consensusDir = Join-Path $RepoRoot "crates\novovm-consensus"
$adapterPluginRequiredCapsNormalized = Normalize-HexMask -Text $AdapterPluginRequiredCaps
$adapterPluginPathResolved = Resolve-AdapterPluginPath -RepoRoot $RepoRoot -PathText $AdapterPluginPath -AllowBuild ($IncludeAdapterBackendCompare -or $IncludeAdapterPluginAbiNegative -or $IncludeAdapterPluginRegistryNegative -or ($AdapterBackend -eq "plugin"))
$adapterComparePluginPathResolved = Resolve-AdapterPluginPath -RepoRoot $RepoRoot -PathText $AdapterComparePluginPath -AllowBuild $IncludeAdapterBackendCompare
if (-not $adapterComparePluginPathResolved) {
    $adapterComparePluginPathResolved = $adapterPluginPathResolved
}
$adapterNegativePluginPathResolved = Resolve-AdapterPluginPath -RepoRoot $RepoRoot -PathText $AdapterNegativePluginPath -AllowBuild $IncludeAdapterPluginAbiNegative
if (-not $adapterNegativePluginPathResolved) {
    $adapterNegativePluginPathResolved = $adapterComparePluginPathResolved
}
if (-not $adapterNegativePluginPathResolved) {
    $adapterNegativePluginPathResolved = $adapterPluginPathResolved
}
$adapterPluginRegistryPathResolved = ""
if ($AdapterPluginRegistryPath) {
    $candidateRegistryPath = $AdapterPluginRegistryPath
    if (-not (Test-Path $candidateRegistryPath)) {
        $repoRelativeRegistryPath = Join-Path $RepoRoot $AdapterPluginRegistryPath
        if (Test-Path $repoRelativeRegistryPath) {
            $candidateRegistryPath = $repoRelativeRegistryPath
        }
    }
    if (Test-Path $candidateRegistryPath) {
        $adapterPluginRegistryPathResolved = (Resolve-Path $candidateRegistryPath).Path
    } else {
        $adapterPluginRegistryPathResolved = $AdapterPluginRegistryPath
    }
} else {
    $defaultRegistryPath = Join-Path $RepoRoot "config\novovm-adapter-plugin-registry.json"
    if (Test-Path $defaultRegistryPath) {
        $adapterPluginRegistryPathResolved = (Resolve-Path $defaultRegistryPath).Path
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
$txWireIngressPath = Join-Path $OutputDir "functional-consistency.ingress.txwire.bin"
$txWireAccounts = [Math]::Max(16, [Math]::Min(1024, ($BatchADemoTxs * 2)))
Invoke-Cargo -WorkDir $benchDir -CargoArgs @(
    "run",
    "--quiet",
    "--bin",
    "novovm-txgen",
    "--",
    "--out",
    $txWireIngressPath,
    "--txs",
    "$BatchADemoTxs",
    "--accounts",
    "$txWireAccounts"
) -EnvVars @{}

$nodeFfiText = Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run") -EnvVars @{
    NOVOVM_EXEC_PATH = "ffi_v2"
    NOVOVM_AOEM_VARIANT = "$CapabilityVariant"
    NOVOVM_TX_WIRE_FILE = "$txWireIngressPath"
    NOVOVM_DEMO_TXS = "$BatchADemoTxs"
    NOVOVM_BATCH_A_BATCHES = "$BatchABatchCount"
    NOVOVM_MEMPOOL_FEE_FLOOR = "$BatchAMempoolFeeFloor"
    NOVOVM_ADAPTER_BACKEND = "$AdapterBackend"
    NOVOVM_ADAPTER_PLUGIN_PATH = "$adapterPluginPathResolved"
    NOVOVM_ADAPTER_CHAIN = "$AdapterExpectedChain"
    NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI = "$AdapterPluginExpectedAbi"
    NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS = "$adapterPluginRequiredCapsNormalized"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH = "$adapterPluginRegistryPathResolved"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "$adapterPluginRegistryStrictFlag"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256 = "$adapterPluginRegistrySha256Normalized"
}
$legacyCompatEmulated = $false
$legacyCompatReason = ""
$legacyProbe = Invoke-CargoAllowFailure -WorkDir $nodeDir -CargoArgs @("run") -EnvVars @{
    NOVOVM_EXEC_PATH = "legacy"
    NOVOVM_AOEM_VARIANT = "$CapabilityVariant"
    NOVOVM_TX_WIRE_FILE = "$txWireIngressPath"
    NOVOVM_DEMO_TXS = "$BatchADemoTxs"
    NOVOVM_BATCH_A_BATCHES = "$BatchABatchCount"
    NOVOVM_MEMPOOL_FEE_FLOOR = "$BatchAMempoolFeeFloor"
    NOVOVM_ADAPTER_BACKEND = "$AdapterBackend"
    NOVOVM_ADAPTER_PLUGIN_PATH = "$adapterPluginPathResolved"
    NOVOVM_ADAPTER_CHAIN = "$AdapterExpectedChain"
    NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI = "$AdapterPluginExpectedAbi"
    NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS = "$adapterPluginRequiredCapsNormalized"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH = "$adapterPluginRegistryPathResolved"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT = "$adapterPluginRegistryStrictFlag"
    NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256 = "$adapterPluginRegistrySha256Normalized"
}
$nodeLegacyText = ""
if ($legacyProbe.exit_code -eq 0) {
    $nodeLegacyText = [string]$legacyProbe.output
} elseif ($legacyProbe.output -match "non-production exec path mode \(legacy\) is disabled") {
    $legacyCompatEmulated = $true
    $legacyCompatReason = "legacy_exec_path_disabled"
    $nodeLegacyText = $nodeFfiText
} else {
    throw "cargo run failed in $nodeDir`n$($legacyProbe.output)"
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
$networkPacemakerFfi = Parse-NetworkPacemakerLine -Text $nodeFfiText
$networkPacemakerLegacy = Parse-NetworkPacemakerLine -Text $nodeLegacyText

$expectedBatchMin = [Math]::Min($BatchABatchCount, $BatchADemoTxs)

$coordinatorSignal = [ordered]@{
    enabled = $IncludeCoordinatorSignal
    available = $false
    pass = $false
    tx_id = 0
    participants = 0
    votes = 0
    decided = $false
    commit = $false
    reason = "disabled"
}
if ($IncludeCoordinatorSignal) {
    if (Test-Path (Join-Path $coordinatorDir "Cargo.toml")) {
        $probe = Invoke-CargoAllowFailure -WorkDir $coordinatorDir -CargoArgs @("run", "--quiet", "--example", "two_pc_smoke") -EnvVars @{}
        if ($probe.exit_code -eq 0) {
            try {
                $parsed = Parse-CoordinatorOutLine -Text $probe.output
                $coordinatorPass = (
                    $parsed.parse_ok -and
                    $parsed.decided -and
                    $parsed.commit -and
                    $parsed.votes -eq $parsed.participants
                )
                $coordinatorSignal = [ordered]@{
                    enabled = $true
                    available = $true
                    pass = $coordinatorPass
                    tx_id = $parsed.tx_id
                    participants = $parsed.participants
                    votes = $parsed.votes
                    decided = $parsed.decided
                    commit = $parsed.commit
                    reason = if ($coordinatorPass) { $null } else { "coordinator smoke output check failed" }
                }
            } catch {
                $coordinatorSignal.reason = "coordinator output parse failed: $($_.Exception.Message)"
            }
        } else {
            $coordinatorSignal.reason = "coordinator smoke command failed (exit=$($probe.exit_code))"
        }
    } else {
        $coordinatorSignal.reason = "coordinator crate missing"
    }
}

$coordinatorNegativeSignal = [ordered]@{
    enabled = $IncludeCoordinatorNegativeSignal
    available = $false
    pass = $false
    unknown_prepare = $false
    non_participant_vote = $false
    vote_after_decide = $false
    duplicate_tx = $false
    reason = "disabled"
}
if ($IncludeCoordinatorNegativeSignal) {
    if (Test-Path (Join-Path $coordinatorDir "Cargo.toml")) {
        $probe = Invoke-CargoAllowFailure -WorkDir $coordinatorDir -CargoArgs @("run", "--quiet", "--example", "coordinator_negative_smoke") -EnvVars @{}
        if ($probe.exit_code -eq 0) {
            try {
                $parsed = Parse-CoordinatorNegativeOutLine -Text $probe.output
                $coordinatorNegativePass = (
                    $parsed.parse_ok -and
                    $parsed.unknown_prepare -and
                    $parsed.non_participant_vote -and
                    $parsed.vote_after_decide -and
                    $parsed.duplicate_tx -and
                    $parsed.pass
                )
                $coordinatorNegativeSignal = [ordered]@{
                    enabled = $true
                    available = $true
                    pass = $coordinatorNegativePass
                    unknown_prepare = $parsed.unknown_prepare
                    non_participant_vote = $parsed.non_participant_vote
                    vote_after_decide = $parsed.vote_after_decide
                    duplicate_tx = $parsed.duplicate_tx
                    reason = if ($coordinatorNegativePass) { $null } else { "coordinator negative smoke output check failed" }
                }
            } catch {
                $coordinatorNegativeSignal.reason = "coordinator negative output parse failed: $($_.Exception.Message)"
            }
        } else {
            $coordinatorNegativeSignal.reason = "coordinator negative smoke command failed (exit=$($probe.exit_code))"
        }
    } else {
        $coordinatorNegativeSignal.reason = "coordinator crate missing"
    }
}

$proverContractSignal = [ordered]@{
    enabled = $IncludeProverContractSignal
    available = $false
    pass = $false
    schema_ok = $false
    normalized_reason_codes = $false
    fallback_codes = 0
    prover_ready = $false
    zk_ready = $false
    msm_backend = ""
    reason = "disabled"
}
if ($IncludeProverContractSignal) {
    if (Test-Path (Join-Path $proverDir "Cargo.toml")) {
        $probe = Invoke-CargoAllowFailure -WorkDir $proverDir -CargoArgs @("run", "--quiet", "--example", "contract_schema_smoke") -EnvVars @{}
        if ($probe.exit_code -eq 0) {
            try {
                $parsed = Parse-ProverContractOutLine -Text $probe.output
                $proverPass = (
                    $parsed.parse_ok -and
                    $parsed.schema_ok -and
                    $parsed.normalized_reason_codes -and
                    $parsed.fallback_codes -gt 0
                )
                $proverContractSignal = [ordered]@{
                    enabled = $true
                    available = $true
                    pass = $proverPass
                    schema_ok = $parsed.schema_ok
                    normalized_reason_codes = $parsed.normalized_reason_codes
                    fallback_codes = $parsed.fallback_codes
                    prover_ready = $parsed.prover_ready
                    zk_ready = $parsed.zk_ready
                    msm_backend = $parsed.msm_backend
                    reason = if ($proverPass) { $null } else { "prover contract smoke output check failed" }
                }
            } catch {
                $proverContractSignal.reason = "prover contract output parse failed: $($_.Exception.Message)"
            }
        } else {
            $proverContractSignal.reason = "prover contract smoke command failed (exit=$($probe.exit_code))"
        }
    } else {
        $proverContractSignal.reason = "prover crate missing"
    }
}

$proverContractNegativeSignal = [ordered]@{
    enabled = $IncludeProverContractNegativeSignal
    available = $false
    pass = $false
    missing_formal_fields = $false
    empty_reason_codes = $false
    reason_normalization_stable = $false
    reason = "disabled"
}
if ($IncludeProverContractNegativeSignal) {
    if (Test-Path (Join-Path $proverDir "Cargo.toml")) {
        $probe = Invoke-CargoAllowFailure -WorkDir $proverDir -CargoArgs @("run", "--quiet", "--example", "contract_schema_negative_smoke") -EnvVars @{}
        if ($probe.exit_code -eq 0) {
            try {
                $parsed = Parse-ProverContractNegativeOutLine -Text $probe.output
                $proverNegativePass = (
                    $parsed.parse_ok -and
                    $parsed.missing_formal_fields -and
                    $parsed.empty_reason_codes -and
                    $parsed.reason_normalization_stable -and
                    $parsed.pass
                )
                $proverContractNegativeSignal = [ordered]@{
                    enabled = $true
                    available = $true
                    pass = $proverNegativePass
                    missing_formal_fields = $parsed.missing_formal_fields
                    empty_reason_codes = $parsed.empty_reason_codes
                    reason_normalization_stable = $parsed.reason_normalization_stable
                    reason = if ($proverNegativePass) { $null } else { "prover contract negative smoke output check failed" }
                }
            } catch {
                $proverContractNegativeSignal.reason = "prover contract negative output parse failed: $($_.Exception.Message)"
            }
        } else {
            $proverContractNegativeSignal.reason = "prover contract negative smoke command failed (exit=$($probe.exit_code))"
        }
    } else {
        $proverContractNegativeSignal.reason = "prover crate missing"
    }
}

$consensusNegativeSignal = [ordered]@{
    enabled = $IncludeConsensusNegativeSignal
    available = $false
    pass = $false
    invalid_signature = $false
    duplicate_vote = $false
    wrong_epoch = $false
    weighted_quorum = $false
    equivocation = $false
    slash_execution = $false
    slash_threshold = $false
    slash_observe_only = $false
    unjail_cooldown = $false
    view_change = $false
    fork_choice = $false
    reason = "disabled"
}
if ($IncludeConsensusNegativeSignal) {
    if (Test-Path (Join-Path $consensusDir "Cargo.toml")) {
        $probe = Invoke-CargoAllowFailure -WorkDir $consensusDir -CargoArgs @("run", "--quiet", "--example", "consensus_negative_smoke") -EnvVars @{}
        if ($probe.exit_code -eq 0) {
            try {
                $parsed = Parse-ConsensusNegativeOutLine -Text $probe.output
                $parsedExt = Parse-ConsensusNegativeExtLine -Text $probe.output
                $consensusNegativePass = (
                    $parsed.parse_ok -and
                    $parsed.invalid_signature -and
                    $parsed.duplicate_vote -and
                    $parsed.wrong_epoch -and
                    $parsedExt.parse_ok -and
                    $parsedExt.weighted_quorum -and
                    $parsedExt.equivocation -and
                    $parsedExt.slash_execution -and
                    $parsedExt.slash_threshold -and
                    $parsedExt.slash_observe_only -and
                    $parsedExt.unjail_cooldown -and
                    $parsedExt.view_change -and
                    $parsedExt.fork_choice -and
                    $parsed.pass
                )
                $consensusNegativeSignal = [ordered]@{
                    enabled = $true
                    available = $true
                    pass = $consensusNegativePass
                    invalid_signature = $parsed.invalid_signature
                    duplicate_vote = $parsed.duplicate_vote
                    wrong_epoch = $parsed.wrong_epoch
                    weighted_quorum = $parsedExt.weighted_quorum
                    equivocation = $parsedExt.equivocation
                    slash_execution = $parsedExt.slash_execution
                    slash_threshold = $parsedExt.slash_threshold
                    slash_observe_only = $parsedExt.slash_observe_only
                    unjail_cooldown = $parsedExt.unjail_cooldown
                    view_change = $parsedExt.view_change
                    fork_choice = $parsedExt.fork_choice
                    reason = if ($consensusNegativePass) { $null } else { "consensus negative smoke output check failed" }
                }
            } catch {
                $consensusNegativeSignal.reason = "consensus negative output parse failed: $($_.Exception.Message)"
            }
        } else {
            $consensusNegativeSignal.reason = "consensus negative smoke command failed (exit=$($probe.exit_code))"
        }
    } else {
        $consensusNegativeSignal.reason = "consensus crate missing"
    }
}

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

$adapterComparePluginPath = $adapterComparePluginPathResolved
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
                NOVOVM_AOEM_VARIANT = "$CapabilityVariant"
                NOVOVM_TX_WIRE_FILE = "$txWireIngressPath"
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
                NOVOVM_AOEM_VARIANT = "$CapabilityVariant"
                NOVOVM_TX_WIRE_FILE = "$txWireIngressPath"
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

$adapterNegativePluginPath = $adapterNegativePluginPathResolved
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
            NOVOVM_AOEM_VARIANT = "$CapabilityVariant"
            NOVOVM_TX_WIRE_FILE = "$txWireIngressPath"
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
    $candidateAoem = Get-DllPathForVariant -AoemRoot $aoemRoot -Variant "core"
    if (Test-Path $candidateAoem) {
        $candidateAoem
    } elseif ($IsMacOS -and (Test-Path "/usr/lib/libSystem.B.dylib")) {
        "/usr/lib/libSystem.B.dylib"
    } elseif ($IsLinux -and (Test-Path "/usr/lib/x86_64-linux-gnu/libc.so.6")) {
        "/usr/lib/x86_64-linux-gnu/libc.so.6"
    } elseif ($IsLinux -and (Test-Path "/lib/x86_64-linux-gnu/libc.so.6")) {
        "/lib/x86_64-linux-gnu/libc.so.6"
    } elseif ($IsLinux -and (Test-Path "/usr/lib64/libc.so.6")) {
        "/usr/lib64/libc.so.6"
    } elseif ($IsLinux -and (Test-Path "/lib64/libc.so.6")) {
        "/lib64/libc.so.6"
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
            NOVOVM_AOEM_VARIANT = "$CapabilityVariant"
            NOVOVM_TX_WIRE_FILE = "$txWireIngressPath"
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

$adapterRegistryNegativePluginPath = if ($adapterPluginPathResolved) {
    $adapterPluginPathResolved
} elseif ($adapterComparePluginPathResolved) {
    $adapterComparePluginPathResolved
} else {
    $adapterNegativePluginPathResolved
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
            NOVOVM_AOEM_VARIANT = "$CapabilityVariant"
            NOVOVM_TX_WIRE_FILE = "$txWireIngressPath"
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
        $registryNegativeWhitelistJson = $registryNegativeWhitelist | ConvertTo-Json -Depth 20
        [System.IO.File]::WriteAllText($registryNegativeWhitelistPath, $registryNegativeWhitelistJson, [System.Text.UTF8Encoding]::new($false))
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
        $blockOutFfi.state_root -eq $blockOutLegacy.state_root -and
        -not [string]::IsNullOrWhiteSpace($blockOutFfi.governance_chain_audit_root) -and
        -not [string]::IsNullOrWhiteSpace($blockOutLegacy.governance_chain_audit_root) -and
        $blockOutFfi.governance_chain_audit_root -eq $blockOutLegacy.governance_chain_audit_root
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
        $commitOutFfi.state_root -eq $commitOutLegacy.state_root -and
        -not [string]::IsNullOrWhiteSpace($commitOutFfi.governance_chain_audit_root) -and
        -not [string]::IsNullOrWhiteSpace($commitOutLegacy.governance_chain_audit_root) -and
        $commitOutFfi.governance_chain_audit_root -eq $commitOutLegacy.governance_chain_audit_root
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

$networkPacemakerAvailable = ($null -ne $networkPacemakerFfi -and $null -ne $networkPacemakerLegacy)
$networkPacemakerPass = $false
if ($networkPacemakerAvailable) {
    $networkPacemakerPass = (
        $networkPacemakerFfi.parse_ok -and
        $networkPacemakerLegacy.parse_ok -and
        $networkPacemakerFfi.view_sync -and
        $networkPacemakerLegacy.view_sync -and
        $networkPacemakerFfi.new_view -and
        $networkPacemakerLegacy.new_view
    )
}

$variants = @("core", "persist", "wasm")
$digestVariants = @()
foreach ($variant in $variants) {
    $candidateDll = Get-DllPathForVariant -AoemRoot $aoemRoot -Variant $variant -RequireExists $false
    if (Test-Path $candidateDll) {
        $digestVariants += $variant
    }
}
if ($digestVariants.Count -eq 0) {
    throw "aoem dynlib not found for variants under $aoemRoot/variants (tried: $($variants -join ', '))"
}

$digests = @()
foreach ($variant in $digestVariants) {
    $dll = Get-DllPathForVariant -AoemRoot $aoemRoot -Variant $variant -RequireExists $true
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

$coreDigestItem = ($digests | Where-Object { $_.variant -eq "core" } | Select-Object -First 1)
if (-not $coreDigestItem) {
    $coreDigestItem = $digests | Select-Object -First 1
}
$coreDigest = [string]$coreDigestItem.digest
$crossVariantPass = ($digests | Where-Object { $_.digest -ne $coreDigest } | Measure-Object).Count -eq 0

$stateRootHardAvailable = (
    $null -ne $adapterFfi -and $null -ne $adapterLegacy -and
    $null -ne $batchAffi -and $null -ne $batchAlegacy -and
    $null -ne $blockOutFfi -and $null -ne $blockOutLegacy -and
    $null -ne $commitOutFfi -and $null -ne $commitOutLegacy
)
$stateRootHardPass = $false
$stateRootHardValue = $null
if ($stateRootHardAvailable) {
    $stateRootHardPass = (
        $adapterFfi.parse_ok -and
        $adapterLegacy.parse_ok -and
        $batchAffi.parse_ok -and
        $batchAlegacy.parse_ok -and
        $blockOutFfi.parse_ok -and
        $blockOutLegacy.parse_ok -and
        $commitOutFfi.parse_ok -and
        $commitOutLegacy.parse_ok -and
        $adapterFfi.state_root -eq $adapterLegacy.state_root -and
        $batchAffi.state_root -eq $batchAlegacy.state_root -and
        $blockOutFfi.state_root -eq $blockOutLegacy.state_root -and
        $commitOutFfi.state_root -eq $commitOutLegacy.state_root -and
        $adapterFfi.state_root -eq $batchAffi.state_root -and
        $adapterFfi.state_root -eq $blockOutFfi.state_root -and
        $adapterFfi.state_root -eq $commitOutFfi.state_root
    )
    if ($adapterFfi.parse_ok) {
        $stateRootHardValue = $adapterFfi.state_root
    }
}

if ($stateRootHardAvailable) {
    $stateRootConsistency = [ordered]@{
        available = $true
        pass = $stateRootHardPass
        method = "hard_state_root_parity"
        root_field = "state_root"
        value = $stateRootHardValue
        proxy_digest = $coreDigest
        compared_variants = @($digestVariants)
        reason = if ($stateRootHardPass) {
            "hard parity check across adapter/batch/block/commit (ffi_v2 vs legacy_compat)"
        } else {
            "state_root mismatch across hard path signals"
        }
    }
} else {
    $stateRootConsistency = [ordered]@{
        available = $false
        pass = $crossVariantPass
        method = "deterministic_digest_proxy"
        root_field = "state_root"
        value = $null
        proxy_digest = $coreDigest
        compared_variants = @($digestVariants)
        reason = "hard state_root signals are incomplete; fallback to deterministic digest proxy"
    }
}

$capabilitySnapshot = $null
$capabilitySnapshotNote = "capability_contract snapshot is disabled for this run"
if ($IncludeCapabilitySnapshot) {
    try {
        $capabilitySnapshot = Get-CapabilitySnapshot -RepoRoot $RepoRoot -Variant $CapabilityVariant -CapabilityJson $CapabilityJson
        $capabilitySnapshotNote = "capability_contract snapshot loaded (variant=$CapabilityVariant)"
    } catch {
        $capabilitySnapshot = $null
        $capabilitySnapshotNote = "capability_contract snapshot failed and was skipped: $($_.Exception.Message)"
    }
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
    view_sync_available = $null
    view_sync_pass = $null
    view_sync_rounds_passed = $null
    view_sync_pass_ratio = $null
    new_view_available = $null
    new_view_pass = $null
    new_view_rounds_passed = $null
    new_view_pass_ratio = $null
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

function Test-SignalPassOrSkipped {
    param(
        [bool]$Available,
        [bool]$Pass
    )
    return ((-not $Available) -or ($Available -and $Pass))
}

$overallPass = (
    $nodeCompatPass -and
    $crossVariantPass -and
    (Test-SignalPassOrSkipped -Available $txCodecAvailable -Pass $txCodecPass) -and
    (Test-SignalPassOrSkipped -Available $mempoolAvailable -Pass $mempoolPass) -and
    (Test-SignalPassOrSkipped -Available $txMetaAvailable -Pass $txMetaPass) -and
    (Test-SignalPassOrSkipped -Available $adapterAvailable -Pass $adapterPass) -and
    (Test-SignalPassOrSkipped -Available $adapterPluginAbiAvailable -Pass $adapterPluginAbiPass) -and
    (Test-SignalPassOrSkipped -Available $adapterPluginRegistryAvailable -Pass $adapterPluginRegistryPass) -and
    (Test-SignalPassOrSkipped -Available $adapterConsensusAvailable -Pass $adapterConsensusPass) -and
    (Test-SignalPassOrSkipped -Available $blockWireAvailable -Pass $blockWirePass) -and
    (Test-SignalPassOrSkipped -Available $networkClosureAvailable -Pass $networkClosurePass) -and
    (Test-SignalPassOrSkipped -Available $networkPacemakerAvailable -Pass $networkPacemakerPass)
)
if ($IncludeNetworkProcessSignal) {
    $overallPass = ($overallPass -and (Test-SignalPassOrSkipped -Available ([bool]$networkProcessSignal.available) -Pass ([bool]$networkProcessSignal.pass)))
}
if ($IncludeAdapterBackendCompare) {
    $overallPass = ($overallPass -and (Test-SignalPassOrSkipped -Available ([bool]$adapterBackendCompareSignal.available) -Pass ([bool]$adapterBackendCompareSignal.pass)))
}
if ($IncludeAdapterPluginAbiNegative) {
    $overallPass = ($overallPass -and (Test-SignalPassOrSkipped -Available ([bool]$adapterPluginAbiNegativeSignal.available) -Pass ([bool]$adapterPluginAbiNegativeSignal.pass)))
}
if ($IncludeAdapterPluginSymbolNegative) {
    $overallPass = ($overallPass -and (Test-SignalPassOrSkipped -Available ([bool]$adapterPluginSymbolNegativeSignal.available) -Pass ([bool]$adapterPluginSymbolNegativeSignal.pass)))
}
if ($IncludeAdapterPluginRegistryNegative) {
    $overallPass = ($overallPass -and (Test-SignalPassOrSkipped -Available ([bool]$adapterPluginRegistryNegativeSignal.available) -Pass ([bool]$adapterPluginRegistryNegativeSignal.pass)))
}
if ($IncludeNetworkBlockWireNegative) {
    $overallPass = ($overallPass -and (Test-SignalPassOrSkipped -Available ([bool]$networkBlockWireNegativeSignal.available) -Pass ([bool]$networkBlockWireNegativeSignal.pass)))
}
if ($IncludeCoordinatorSignal) {
    $overallPass = ($overallPass -and $coordinatorSignal.available -and $coordinatorSignal.pass)
}
if ($IncludeCoordinatorNegativeSignal) {
    $overallPass = ($overallPass -and $coordinatorNegativeSignal.available -and $coordinatorNegativeSignal.pass)
}
if ($IncludeProverContractSignal) {
    $overallPass = ($overallPass -and $proverContractSignal.available -and $proverContractSignal.pass)
}
if ($IncludeProverContractNegativeSignal) {
    $overallPass = ($overallPass -and $proverContractNegativeSignal.available -and $proverContractNegativeSignal.pass)
}
if ($IncludeConsensusNegativeSignal) {
    $overallPass = ($overallPass -and $consensusNegativeSignal.available -and $consensusNegativeSignal.pass)
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
$coordinatorNote = if ($IncludeCoordinatorSignal) {
    "coordinator_signal validates novovm-coordinator 2PC smoke (propose/prepare/vote/decide) deterministically"
} else {
    "coordinator_signal is disabled for this run"
}
$coordinatorNegativeNote = if ($IncludeCoordinatorNegativeSignal) {
    "coordinator_negative_signal validates unknown-tx / non-participant / post-decide / duplicate-tx rejection gates in novovm-coordinator"
} else {
    "coordinator_negative_signal is disabled for this run"
}
$proverContractNote = if ($IncludeProverContractSignal) {
    "prover_contract_signal validates novovm-prover contract schema and fallback reason-code normalization"
} else {
    "prover_contract_signal is disabled for this run"
}
$proverContractNegativeNote = if ($IncludeProverContractNegativeSignal) {
    "prover_contract_negative_signal validates schema guardrails (missing formal fields / empty reason codes) and reason-code normalization stability"
} else {
    "prover_contract_negative_signal is disabled for this run"
}
$consensusNegativeNote = if ($IncludeConsensusNegativeSignal) {
    "consensus_negative_signal validates invalid-signature / duplicate-vote / wrong-epoch and enforces weighted-quorum + equivocation(slashing evidence) + slash-execution + slash-threshold-policy + slash-observe-only-policy + view-change + fork-choice gates in novovm-consensus"
} else {
    "consensus_negative_signal is disabled for this run"
}

$result = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    ingress_input = [ordered]@{
        tx_wire_file = $txWireIngressPath
        txs = $BatchADemoTxs
        accounts = $txWireAccounts
    }
    legacy_compat_mode = [ordered]@{
        emulated = $legacyCompatEmulated
        reason = $legacyCompatReason
    }
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
    network_pacemaker_signal = [ordered]@{
        available = $networkPacemakerAvailable
        pass = $networkPacemakerPass
        ffi_v2 = $networkPacemakerFfi
        legacy_compat = $networkPacemakerLegacy
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
    coordinator_signal = $coordinatorSignal
    coordinator_negative_signal = $coordinatorNegativeSignal
    prover_contract_signal = $proverContractSignal
    prover_contract_negative_signal = $proverContractNegativeSignal
    consensus_negative_signal = $consensusNegativeSignal
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
        "state_root_consistency uses hard parity across adapter/batch/block/commit when available; otherwise fallback digest proxy is used",
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
        $coordinatorNote,
        $coordinatorNegativeNote,
        $proverContractNote,
        $proverContractNegativeNote,
        $consensusNegativeNote,
        $capabilitySnapshotNote,
        "batch_a_closure is reported as an execution-to-consensus integration signal and is not a hard gate yet",
        $blockWireNote,
        "block_output_signal compares deterministic block_hash and governance_chain_audit_root output across ffi_v2 and legacy_compat routes",
        "commit_output_signal compares commit records (including governance_chain_audit_root) from in-memory block store across ffi_v2 and legacy_compat routes",
        "network_output_signal compares in-memory transport delivery signal across ffi_v2 and legacy_compat routes",
        "network_closure_signal validates two-node discovery/gossip/sync closure across ffi_v2 and legacy_compat routes",
        "network_pacemaker_signal validates two-node view_sync/new_view closure across ffi_v2 and legacy_compat routes",
        "network_process_signal validates mesh/pair-matrix process probe over UDP transport with block_header_wire_v1 payload decode + consensus binding verification + pacemaker(view_sync/new_view) closure (rounds=$NetworkProcessRounds)",
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
    "- consensus_negative_signal.enabled: $($result.consensus_negative_signal.enabled)"
    "- consensus_negative_signal.available: $($result.consensus_negative_signal.available)"
    "- consensus_negative_signal.pass: $($result.consensus_negative_signal.pass)"
    "- consensus_negative_signal.weighted_quorum: $($result.consensus_negative_signal.weighted_quorum)"
    "- consensus_negative_signal.equivocation: $($result.consensus_negative_signal.equivocation)"
    "- consensus_negative_signal.slash_execution: $($result.consensus_negative_signal.slash_execution)"
    "- consensus_negative_signal.slash_threshold: $($result.consensus_negative_signal.slash_threshold)"
    "- consensus_negative_signal.slash_observe_only: $($result.consensus_negative_signal.slash_observe_only)"
    "- consensus_negative_signal.unjail_cooldown: $($result.consensus_negative_signal.unjail_cooldown)"
    "- consensus_negative_signal.view_change: $($result.consensus_negative_signal.view_change)"
    "- consensus_negative_signal.fork_choice: $($result.consensus_negative_signal.fork_choice)"
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
    "- network_pacemaker_signal.available: $($result.network_pacemaker_signal.available)"
    "- network_pacemaker_signal.pass: $($result.network_pacemaker_signal.pass)"
    "- network_process_signal.available: $($result.network_process_signal.available)"
    "- network_process_signal.pass: $($result.network_process_signal.pass)"
    "- network_process_signal.rounds: $($result.network_process_signal.rounds)"
    "- network_process_signal.rounds_passed: $($result.network_process_signal.rounds_passed)"
    "- network_process_signal.round_pass_ratio: $($result.network_process_signal.round_pass_ratio)"
    "- network_process_signal.block_wire_available: $($result.network_process_signal.block_wire_available)"
    "- network_process_signal.block_wire_pass: $($result.network_process_signal.block_wire_pass)"
    "- network_process_signal.block_wire_pass_ratio: $($result.network_process_signal.block_wire_pass_ratio)"
    "- network_process_signal.view_sync_available: $($result.network_process_signal.view_sync_available)"
    "- network_process_signal.view_sync_pass: $($result.network_process_signal.view_sync_pass)"
    "- network_process_signal.view_sync_pass_ratio: $($result.network_process_signal.view_sync_pass_ratio)"
    "- network_process_signal.new_view_available: $($result.network_process_signal.new_view_available)"
    "- network_process_signal.new_view_pass: $($result.network_process_signal.new_view_pass)"
    "- network_process_signal.new_view_pass_ratio: $($result.network_process_signal.new_view_pass_ratio)"
    "- coordinator_signal.enabled: $($result.coordinator_signal.enabled)"
    "- coordinator_signal.available: $($result.coordinator_signal.available)"
    "- coordinator_signal.pass: $($result.coordinator_signal.pass)"
    "- coordinator_negative_signal.enabled: $($result.coordinator_negative_signal.enabled)"
    "- coordinator_negative_signal.available: $($result.coordinator_negative_signal.available)"
    "- coordinator_negative_signal.pass: $($result.coordinator_negative_signal.pass)"
    "- prover_contract_signal.enabled: $($result.prover_contract_signal.enabled)"
    "- prover_contract_signal.available: $($result.prover_contract_signal.available)"
    "- prover_contract_signal.pass: $($result.prover_contract_signal.pass)"
    "- prover_contract_negative_signal.enabled: $($result.prover_contract_negative_signal.enabled)"
    "- prover_contract_negative_signal.available: $($result.prover_contract_negative_signal.available)"
    "- prover_contract_negative_signal.pass: $($result.prover_contract_negative_signal.pass)"
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
    "## Consensus Negative Signal"
    ""
    "- enabled: $($result.consensus_negative_signal.enabled)"
    "- available: $($result.consensus_negative_signal.available)"
    "- pass: $($result.consensus_negative_signal.pass)"
    ""
    "## Coordinator Negative Signal"
    ""
    "- enabled: $($result.coordinator_negative_signal.enabled)"
    "- available: $($result.coordinator_negative_signal.available)"
    "- pass: $($result.coordinator_negative_signal.pass)"
    ""
    "## Prover Contract Negative Signal"
    ""
    "- enabled: $($result.prover_contract_negative_signal.enabled)"
    "- available: $($result.prover_contract_negative_signal.available)"
    "- pass: $($result.prover_contract_negative_signal.pass)"
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
    "## Network Pacemaker Signal"
    ""
    "- available: $($result.network_pacemaker_signal.available)"
    "- pass: $($result.network_pacemaker_signal.pass)"
    ""
    "## Network Process Signal"
    ""
    "- available: $($result.network_process_signal.available)"
    "- pass: $($result.network_process_signal.pass)"
    "- block_wire_available: $($result.network_process_signal.block_wire_available)"
    "- block_wire_pass: $($result.network_process_signal.block_wire_pass)"
    "- view_sync_available: $($result.network_process_signal.view_sync_available)"
    "- view_sync_pass: $($result.network_process_signal.view_sync_pass)"
    "- new_view_available: $($result.network_process_signal.new_view_available)"
    "- new_view_pass: $($result.network_process_signal.new_view_pass)"
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

if ($result.consensus_negative_signal.available) {
    $md += ""
    $md += "Consensus negative signal:"
    $md += ""
    $md += "- invalid_signature: $($result.consensus_negative_signal.invalid_signature)"
    $md += "- duplicate_vote: $($result.consensus_negative_signal.duplicate_vote)"
    $md += "- wrong_epoch: $($result.consensus_negative_signal.wrong_epoch)"
    $md += "- weighted_quorum: $($result.consensus_negative_signal.weighted_quorum)"
    $md += "- equivocation: $($result.consensus_negative_signal.equivocation)"
    $md += "- slash_execution: $($result.consensus_negative_signal.slash_execution)"
    $md += "- slash_threshold: $($result.consensus_negative_signal.slash_threshold)"
    $md += "- slash_observe_only: $($result.consensus_negative_signal.slash_observe_only)"
    $md += "- unjail_cooldown: $($result.consensus_negative_signal.unjail_cooldown)"
    $md += "- view_change: $($result.consensus_negative_signal.view_change)"
    $md += "- fork_choice: $($result.consensus_negative_signal.fork_choice)"
} elseif ($result.consensus_negative_signal.enabled) {
    $md += ""
    $md += "Consensus negative signal reason:"
    $md += ""
    $md += "- reason: $($result.consensus_negative_signal.reason)"
}

if ($result.coordinator_signal.available) {
    $md += ""
    $md += "Coordinator signal:"
    $md += ""
    $md += "- tx_id: $($result.coordinator_signal.tx_id)"
    $md += "- participants: $($result.coordinator_signal.participants)"
    $md += "- votes: $($result.coordinator_signal.votes)"
    $md += "- decided: $($result.coordinator_signal.decided)"
    $md += "- commit: $($result.coordinator_signal.commit)"
} elseif ($result.coordinator_signal.enabled) {
    $md += ""
    $md += "Coordinator signal reason:"
    $md += ""
    $md += "- reason: $($result.coordinator_signal.reason)"
}

if ($result.coordinator_negative_signal.available) {
    $md += ""
    $md += "Coordinator negative signal:"
    $md += ""
    $md += "- unknown_prepare: $($result.coordinator_negative_signal.unknown_prepare)"
    $md += "- non_participant_vote: $($result.coordinator_negative_signal.non_participant_vote)"
    $md += "- vote_after_decide: $($result.coordinator_negative_signal.vote_after_decide)"
    $md += "- duplicate_tx: $($result.coordinator_negative_signal.duplicate_tx)"
} elseif ($result.coordinator_negative_signal.enabled) {
    $md += ""
    $md += "Coordinator negative signal reason:"
    $md += ""
    $md += "- reason: $($result.coordinator_negative_signal.reason)"
}

if ($result.prover_contract_signal.available) {
    $md += ""
    $md += "Prover contract signal:"
    $md += ""
    $md += "- schema_ok: $($result.prover_contract_signal.schema_ok)"
    $md += "- normalized_reason_codes: $($result.prover_contract_signal.normalized_reason_codes)"
    $md += "- fallback_codes: $($result.prover_contract_signal.fallback_codes)"
    $md += "- prover_ready: $($result.prover_contract_signal.prover_ready)"
    $md += "- zk_ready: $($result.prover_contract_signal.zk_ready)"
    $md += "- msm_backend: $($result.prover_contract_signal.msm_backend)"
} elseif ($result.prover_contract_signal.enabled) {
    $md += ""
    $md += "Prover contract signal reason:"
    $md += ""
    $md += "- reason: $($result.prover_contract_signal.reason)"
}

if ($result.prover_contract_negative_signal.available) {
    $md += ""
    $md += "Prover contract negative signal:"
    $md += ""
    $md += "- missing_formal_fields: $($result.prover_contract_negative_signal.missing_formal_fields)"
    $md += "- empty_reason_codes: $($result.prover_contract_negative_signal.empty_reason_codes)"
    $md += "- reason_normalization_stable: $($result.prover_contract_negative_signal.reason_normalization_stable)"
} elseif ($result.prover_contract_negative_signal.enabled) {
    $md += ""
    $md += "Prover contract negative signal reason:"
    $md += ""
    $md += "- reason: $($result.prover_contract_negative_signal.reason)"
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
    $md += "- ffi_v2.governance_chain_audit_root: $($result.block_output_signal.ffi_v2.governance_chain_audit_root)"
    $md += "- legacy_compat.governance_chain_audit_root: $($result.block_output_signal.legacy_compat.governance_chain_audit_root)"
}

if ($result.commit_output_signal.available -and $result.commit_output_signal.ffi_v2.parse_ok -and $result.commit_output_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Commit output hashes:"
    $md += ""
    $md += "- ffi_v2.block_hash: $($result.commit_output_signal.ffi_v2.block_hash)"
    $md += "- legacy_compat.block_hash: $($result.commit_output_signal.legacy_compat.block_hash)"
    $md += "- ffi_v2.governance_chain_audit_root: $($result.commit_output_signal.ffi_v2.governance_chain_audit_root)"
    $md += "- legacy_compat.governance_chain_audit_root: $($result.commit_output_signal.legacy_compat.governance_chain_audit_root)"
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

if ($result.network_pacemaker_signal.available -and $result.network_pacemaker_signal.ffi_v2.parse_ok -and $result.network_pacemaker_signal.legacy_compat.parse_ok) {
    $md += ""
    $md += "Network pacemaker signal:"
    $md += ""
    $md += "- ffi_v2: view_sync=$($result.network_pacemaker_signal.ffi_v2.view_sync), new_view=$($result.network_pacemaker_signal.ffi_v2.new_view)"
    $md += "- legacy_compat: view_sync=$($result.network_pacemaker_signal.legacy_compat.view_sync), new_view=$($result.network_pacemaker_signal.legacy_compat.new_view)"
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
    $md += "- view_sync_available: $($result.network_process_signal.view_sync_available)"
    $md += "- view_sync_pass: $($result.network_process_signal.view_sync_pass)"
    $md += "- view_sync_rounds_passed: $($result.network_process_signal.view_sync_rounds_passed)"
    $md += "- view_sync_pass_ratio: $($result.network_process_signal.view_sync_pass_ratio)"
    $md += "- new_view_available: $($result.network_process_signal.new_view_available)"
    $md += "- new_view_pass: $($result.network_process_signal.new_view_pass)"
    $md += "- new_view_rounds_passed: $($result.network_process_signal.new_view_rounds_passed)"
    $md += "- new_view_pass_ratio: $($result.network_process_signal.new_view_pass_ratio)"
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
    $md += "- zk_formal_fields_present: $($capabilitySnapshot.zk_formal_fields_present)"
    $md += "- prover_ready: $($capabilitySnapshot.prover_ready)"
    $md += "- msm_accel: $($capabilitySnapshot.msm_accel)"
    $md += "- msm_backend: $($capabilitySnapshot.msm_backend)"
    $md += "- fallback_reason: $($capabilitySnapshot.fallback_reason)"
    $md += "- fallback_reason_codes: $((@($capabilitySnapshot.fallback_reason_codes) -join ', '))"
    $md += "- inferred_from_legacy_fields: $($capabilitySnapshot.inferred_from_legacy_fields)"
}

$md -join "`n" | Set-Content -Path $mdPath -Encoding UTF8

Write-Host "functional consistency report generated:"
Write-Host "  $jsonPath"
Write-Host "  $mdPath"
