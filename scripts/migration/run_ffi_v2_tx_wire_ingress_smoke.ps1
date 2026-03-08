param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 2000000)]
    [int]$Txs = 1000,
    [ValidateRange(1, 2000000)]
    [int]$Accounts = 128,
    [ValidateSet("release", "debug")]
    [string]$BuildProfile = "release",
    [ValidateRange(30, 1800)]
    [int]$TimeoutSec = 180
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

$dateTag = Get-Date -Format "yyyy-MM-dd"
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\ffi-v2-tx-wire-smoke-$dateTag"
}
if (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-Cargo {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs
    )
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")
    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()
    if ($proc.ExitCode -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed in $WorkDir`n$stdout`n$stderr"
    }
}

$nodeDir = Join-Path $RepoRoot "crates\novovm-node"
if ($BuildProfile -eq "release") {
    Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("build", "--quiet", "--release", "--bin", "novovm-node")
} else {
    Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("build", "--quiet", "--bin", "novovm-node")
}

$exePath = if ($BuildProfile -eq "release") {
    Join-Path $nodeDir "cargo-target\release\novovm-node.exe"
} else {
    Join-Path $nodeDir "cargo-target\debug\novovm-node.exe"
}
if (-not (Test-Path $exePath)) {
    $fallback = Join-Path $RepoRoot ("target\{0}\novovm-node.exe" -f $BuildProfile)
    if (-not (Test-Path $fallback)) {
        throw "novovm-node executable not found: $exePath"
    }
    $exePath = $fallback
}

$txWirePath = Join-Path $OutputDir "ingress.txwire.bin"
Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("run", "--quiet", "--bin", "novovm-txgen", "--", "--out", $txWirePath, "--txs", "$Txs", "--accounts", "$Accounts")
if (-not (Test-Path $txWirePath)) {
    throw "tx wire ingress file not generated: $txWirePath"
}
$txWirePath = [System.IO.Path]::GetFullPath($txWirePath)

$stdoutPath = Join-Path $OutputDir "node.stdout.log"
$stderrPath = Join-Path $OutputDir "node.stderr.log"
if (Test-Path $stdoutPath) { Remove-Item $stdoutPath -Force }
if (Test-Path $stderrPath) { Remove-Item $stderrPath -Force }

$envMap = @{
    NOVOVM_EXEC_PATH = "ffi_v2"
    NOVOVM_TX_WIRE_FILE = $txWirePath
    NOVOVM_ENABLE_HOST_ADMISSION = "0"
}
$previousEnv = @{}
foreach ($entry in $envMap.GetEnumerator()) {
    $previousEnv[$entry.Key] = [Environment]::GetEnvironmentVariable($entry.Key, "Process")
    Set-Item -Path ("Env:{0}" -f $entry.Key) -Value $entry.Value
}
$exitCode = $null
$sw = [System.Diagnostics.Stopwatch]::StartNew()
try {
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $exePath
    $psi.WorkingDirectory = (Split-Path $exePath -Parent)
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $proc = [System.Diagnostics.Process]::Start($psi)
    $timedOut = -not $proc.WaitForExit($TimeoutSec * 1000)
    $sw.Stop()
    if ($timedOut) {
        try { $proc.Kill() } catch {}
        throw "novovm-node timed out after $TimeoutSec sec"
    }
    $stdoutText = $proc.StandardOutput.ReadToEnd()
    $stderrText = $proc.StandardError.ReadToEnd()
    $proc.Refresh()
    $exitCode = $proc.ExitCode
    $stdoutText | Set-Content -Path $stdoutPath -Encoding UTF8
    $stderrText | Set-Content -Path $stderrPath -Encoding UTF8
} finally {
    foreach ($key in $previousEnv.Keys) {
        $prior = $previousEnv[$key]
        if ($null -eq $prior -or $prior -eq "") {
            Remove-Item -Path ("Env:{0}" -f $key) -ErrorAction SilentlyContinue
        } else {
            Set-Item -Path ("Env:{0}" -f $key) -Value $prior
        }
    }
}

$stdoutText = if (Test-Path $stdoutPath) { Get-Content $stdoutPath -Raw } else { "" }
$modeLine = ($stdoutText -split "`r?`n" | Where-Object { $_ -match "^mode=ffi_v2 variant=" } | Select-Object -Last 1)
$ingressLine = ($stdoutText -split "`r?`n" | Where-Object { $_ -match "^tx_ingress_source:" } | Select-Object -Last 1)
$contractLine = ($stdoutText -split "`r?`n" | Where-Object { $_ -match "^d1_ingress_contract:" } | Select-Object -Last 1)
$modeMatch = [regex]::Match(
    [string]$modeLine,
    "^mode=ffi_v2 variant=(?<variant>\w+) dll=(?<dll>.+?) rc=(?<rc>\d+)\((?<rc_name>[^)]+)\) submitted=(?<submitted>\d+) processed=(?<processed>\d+) success=(?<success>\d+) writes=(?<writes>\d+) elapsed_us=(?<elapsed>\d+)$"
)
$ingressMatch = [regex]::Match(
    [string]$ingressLine,
    "^tx_ingress_source: mode=(?<mode>\w+) txs=(?<txs>\d+) host_admission=(?<host_admission>true|false)$"
)
$contractMatch = [regex]::Match(
    [string]$contractLine,
    "^d1_ingress_contract: mode=(?<mode>\S+) source=(?<source>\S+) codec=(?<codec>\S+) aoem_ingress_path=(?<path>\S+)$"
)
$exitCode = if ($null -ne $exitCode) { $exitCode } else { 1 }

$pass = (
    $exitCode -eq 0 `
    -and $modeMatch.Success `
    -and $ingressMatch.Success `
    -and $contractMatch.Success `
    -and $ingressMatch.Groups["mode"].Value -eq "tx_wire" `
    -and $contractMatch.Groups["source"].Value -eq "tx_wire"
)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    repo_root = $RepoRoot
    output_dir = $OutputDir
    tx_wire_file = $txWirePath
    txs = $Txs
    accounts = $Accounts
    elapsed_ms = [Math]::Round($sw.Elapsed.TotalMilliseconds, 2)
    exit_code = $exitCode
    mode_line = $modeLine
    ingress_line = $ingressLine
    contract_line = $contractLine
    stdout = $stdoutPath
    stderr = $stderrPath
}

$summaryJsonPath = Join-Path $OutputDir "ffi-v2-tx-wire-smoke-summary.json"
$summaryMdPath = Join-Path $OutputDir "ffi-v2-tx-wire-smoke-summary.md"
$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryJsonPath -Encoding UTF8

$md = @()
$md += "# FFI V2 TX Wire Ingress Smoke ($dateTag)"
$md += ""
$md += "- pass: $pass"
$md += "- tx_wire_file: $txWirePath"
$md += "- txs: $Txs"
$md += "- accounts: $Accounts"
$md += "- elapsed_ms: $([Math]::Round($sw.Elapsed.TotalMilliseconds, 2))"
$md += "- exit_code: $exitCode"
$md += "- ingress_line: $ingressLine"
$md += "- contract_line: $contractLine"
$md += "- mode_line: $modeLine"
$md += "- stdout: $stdoutPath"
$md += "- stderr: $stderrPath"
$md | Set-Content -Path $summaryMdPath -Encoding UTF8

Write-Host "ffi v2 tx wire ingress smoke generated:"
Write-Host "  summary_json: $summaryJsonPath"
Write-Host "  summary_md:   $summaryMdPath"
Write-Host "  overall_pass: $pass"

if (-not $pass) {
    throw "ffi_v2 tx wire ingress smoke failed"
}
