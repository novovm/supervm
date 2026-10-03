Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../lib/overlay-gate-build.ps1')
$root = Join-Path ([System.IO.Path]::GetTempPath()) ('overlay-build-test-' + [guid]::NewGuid())
$oldTarget = $env:CARGO_TARGET_DIR
$oldLocation = (Get-Location).Path
$script:buildExit = 0
$script:buildArgs = @()
function cargo {
    $script:buildArgs = $args
    $global:LASTEXITCODE = $script:buildExit
}
function Assert-True([bool]$Value, [string]$Message) {
    if (-not $Value) { throw $Message }
}
function Assert-Rejected([scriptblock]$Action, [string]$Expected) {
    try { & $Action | Out-Null } catch {
        if ($_.Exception.Message.Contains($Expected)) { return }
        throw
    }
    throw "expected rejection: $Expected"
}
try {
    foreach ($scriptName in @('novovm-overlay-cross-machine-process-gate.ps1', 'novovm-adaptive-overlay-cross-machine-smoke.ps1')) {
        $parseErrors = $null
        $tokens = $null
        [Management.Automation.Language.Parser]::ParseFile(
            (Join-Path $PSScriptRoot "../$scriptName"), [ref]$tokens, [ref]$parseErrors) | Out-Null
        Assert-True ($parseErrors.Count -eq 0) "parse failed: $scriptName"
    }
    New-Item -ItemType Directory -Path $root | Out-Null
    $name = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
        'supervm-network-overlay-gate.exe'
    } else { 'supervm-network-overlay-gate' }
    foreach ($target in @('', 'relative target', (Join-Path $root 'absolute target'))) {
        $env:CARGO_TARGET_DIR = $target
        $expected = if ($target -eq '') { Join-Path $root 'target' }
            elseif ([IO.Path]::IsPathRooted($target)) { $target }
            else { Join-Path $root $target }
        $debug = Join-Path $expected 'debug'
        New-Item -ItemType Directory -Force -Path $debug | Out-Null
        $binary = Join-Path $debug $name
        [IO.File]::WriteAllText($binary, 'old executable sentinel')
        Assert-True ((Resolve-OverlayGateBinary -RepoRoot $root -SkipBuild) -eq $binary) 'wrong skip-build path'
        $script:buildExit = 101
        Assert-Rejected { Resolve-OverlayGateBinary -RepoRoot $root } 'refusing stale executable'
        Assert-True ((Get-Location).Path -eq $oldLocation) 'build failure changed location'
        Assert-True ([IO.File]::ReadAllText($binary) -eq 'old executable sentinel') 'old binary changed'
        $script:buildExit = 0
        Assert-True ((Resolve-OverlayGateBinary -RepoRoot $root) -eq $binary) 'wrong built path'
        Assert-True ($script:buildArgs[-2] -eq '--target-dir' -and $script:buildArgs[-1] -eq $expected) 'cargo target differs from lookup'
    }
    $env:CARGO_TARGET_DIR = 'missing'
    Assert-Rejected { Resolve-OverlayGateBinary -RepoRoot $root -SkipBuild } 'missing gate binary'
    'PASS: default/relative/absolute destinations, stale-build refusal, missing binary and cwd restoration'
} finally {
    $env:CARGO_TARGET_DIR = $oldTarget
    # Keep isolated evidence; never delete a recursively computed directory.
    Write-Output "Test artifacts: $root"
}
