param(
    [string]$RepoRoot = "D:\WorksArea\SUPERVM",
    [string]$OutputDir = "D:\WorksArea\SUPERVM\artifacts\migration\functional",
    [int]$Rounds = 200,
    [int]$Points = 1024,
    [int]$KeySpace = 251,
    [double]$Rw = 0.5,
    [int]$Seed = 123
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

$aoemRoot = Join-Path $RepoRoot "aoem"
$nodeDir = Join-Path $RepoRoot "crates\novovm-node"
$bindingsDir = Join-Path $RepoRoot "crates\aoem-bindings"

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$nodeFfiText = Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run") -EnvVars @{
    NOVOVM_EXEC_PATH = "ffi_v2"
    NOVOVM_AOEM_VARIANT = "core"
}
$nodeLegacyText = Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run") -EnvVars @{
    NOVOVM_EXEC_PATH = "legacy"
    NOVOVM_AOEM_VARIANT = "core"
}

$nodeFfi = Parse-NodeReportLine -Text $nodeFfiText
$nodeLegacy = Parse-NodeReportLine -Text $nodeLegacyText

$nodeCompatPass = (
    $nodeFfi.rc -eq 0 -and
    $nodeLegacy.rc -eq 0 -and
    $nodeFfi.processed -eq $nodeLegacy.processed -and
    $nodeFfi.success -eq $nodeLegacy.success -and
    $nodeFfi.writes -eq $nodeLegacy.writes
)

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

$result = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    node_mode_consistency = [ordered]@{
        compared = @("ffi_v2", "legacy_compat")
        pass = $nodeCompatPass
        ffi_v2 = $nodeFfi
        legacy_compat = $nodeLegacy
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
    overall_pass = ($nodeCompatPass -and $crossVariantPass)
    notes = @(
        "state_root API is not exposed in current AOEM FFI V2 skeleton; this check uses deterministic execution digest as temporary proxy"
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
    "- variant_digest_consistency.pass: $($result.variant_digest_consistency.pass)"
    ""
    "## Node Mode Consistency"
    ""
    "| mode | rc | processed | success | writes |"
    "|---|---:|---:|---:|---:|"
    "| ffi_v2 | $($nodeFfi.rc) | $($nodeFfi.processed) | $($nodeFfi.success) | $($nodeFfi.writes) |"
    "| legacy_compat | $($nodeLegacy.rc) | $($nodeLegacy.processed) | $($nodeLegacy.success) | $($nodeLegacy.writes) |"
    ""
    "## Variant Digest Consistency"
    ""
    "| variant | digest | total_processed | total_success | total_writes |"
    "|---|---|---:|---:|---:|"
)

foreach ($item in $digests) {
    $md += "| $($item.variant) | $($item.digest) | $($item.total_processed) | $($item.total_success) | $($item.total_writes) |"
}

$md += ""
$md += "## Notes"
$md += ""
foreach ($n in $result.notes) {
    $md += "- $n"
}

$md -join "`n" | Set-Content -Path $mdPath -Encoding UTF8

Write-Host "functional consistency report generated:"
Write-Host "  $jsonPath"
Write-Host "  $mdPath"
