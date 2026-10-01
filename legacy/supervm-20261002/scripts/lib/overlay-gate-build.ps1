# Shared by network-only LAN diagnostics; does not launch nodes or touch state.
function Resolve-OverlayGateBinary {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [switch]$SkipBuild
    )
    $root = [System.IO.Path]::GetFullPath($RepoRoot)
    $target = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')
    if ([string]::IsNullOrWhiteSpace($target)) {
        $target = Join-Path $root 'target'
    } elseif (-not [System.IO.Path]::IsPathRooted($target)) {
        $target = Join-Path $root $target
    }
    $target = [System.IO.Path]::GetFullPath($target)
    # Pass the resolved destination explicitly so Cargo config cannot select a
    # different directory from the executable lookup below.
    if (-not $SkipBuild) {
        Push-Location $root
        try {
            & cargo build -q -p novovm-node --bin supervm-network-overlay-gate --target-dir $target
            if ($LASTEXITCODE -ne 0) {
                throw "overlay gate build failed (exit=$LASTEXITCODE); refusing stale executable"
            }
        } finally {
            Pop-Location
        }
    }
    $name = if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
        'supervm-network-overlay-gate.exe'
    } else {
        'supervm-network-overlay-gate'
    }
    $binary = Join-Path (Join-Path $target 'debug') $name
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
        throw "missing gate binary: $binary"
    }
    return $binary
}
