Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Convert-CompatArgToken {
    param([string]$Token)
    if ($Token -match '^-[A-Za-z][A-Za-z0-9]*$') {
        $name = $Token.Substring(1)
        $kebab = [System.Text.RegularExpressions.Regex]::Replace(
            $name,
            '([a-z0-9])([A-Z])',
            '$1-$2'
        ).ToLowerInvariant()
        return "--$kebab"
    }
    return $Token
}

function Resolve-NovovmctlBinary {
    param(
        [string]$RepoRoot,
        [string]$ExplicitCtlPath
    )
    $candidates = @()
    if (-not [string]::IsNullOrWhiteSpace($ExplicitCtlPath)) { $candidates += $ExplicitCtlPath }
    if ($env:NOVOVMCTL_BIN) { $candidates += $env:NOVOVMCTL_BIN }
    $candidates += @(
        (Join-Path $RepoRoot "target\release\novovmctl.exe"),
        (Join-Path $RepoRoot "target\release\novovmctl"),
        (Join-Path $RepoRoot "target\debug\novovmctl.exe"),
        (Join-Path $RepoRoot "target\debug\novovmctl")
    )
    foreach ($p in $candidates) {
        if ($p -and (Test-Path -LiteralPath $p)) { return (Resolve-Path $p).Path }
    }
    return $null
}

function Resolve-MainlinePreflightBinary {
    param(
        [string]$RepoRoot
    )
    $candidates = @(
        (Join-Path $RepoRoot "target\release\supervm-mainline-preflight.exe"),
        (Join-Path $RepoRoot "target\release\supervm-mainline-preflight"),
        (Join-Path $RepoRoot "target\debug\supervm-mainline-preflight.exe"),
        (Join-Path $RepoRoot "target\debug\supervm-mainline-preflight")
    )
    foreach ($p in $candidates) {
        if ($p -and (Test-Path -LiteralPath $p)) { return (Resolve-Path $p).Path }
    }
    return $null
}

function Invoke-MainlinePreflight {
    param([string]$RepoRoot)

    $preflight = Resolve-MainlinePreflightBinary -RepoRoot $RepoRoot
    if ($preflight) {
        & $preflight --repo-root $RepoRoot
        return $LASTEXITCODE
    }

    & cargo run -p novovm-node --bin supervm-mainline-preflight -- --repo-root $RepoRoot
    return $LASTEXITCODE
}

function Test-MainlineGateIsRequiredForSubcommand {
    param([string]$Subcommand)
    switch ($Subcommand.ToLowerInvariant()) {
        "up" { return $true }
        "daemon" { return $true }
        "rollout" { return $true }
        "rollout-control" { return $true }
        "lifecycle" { return $true }
        default { return $false }
    }
}

function Assert-MainlineGateStatus {
    param([string]$RepoRoot)

    function Throw-MainlineGateContractViolation {
        param(
            [Parameter(Mandatory = $true)][string]$Code,
            [Parameter(Mandatory = $true)][string]$Detail
        )
        throw "mainline gate contract failed [$Code]: $Detail"
    }
    $exitCode = Invoke-MainlinePreflight -RepoRoot $RepoRoot
    if ($exitCode -eq 0) { return }
    Throw-MainlineGateContractViolation -Code "preflight.failed" -Detail "rust preflight failed (exit=$exitCode)"
}

function Clear-ManualRouteEnvBlacklist {
    $keys = @(
        "NOVOVM_L3_POLICY_MODE",
        "NOVOVM_L3_PROFILE_STICKY_MARGIN",
        "NOVOVM_L3_PROFILE_RUNTIME_FEEDBACK_SCALE",
        "NOVOVM_L3_PROFILE_CANDIDATE_LIMIT",
        "NOVOVM_L3_PROFILE_MODE_POLICY",
        "NOVOVM_L3_PROFILE_MODE_POLICY_GOVERNANCE",
        "NOVOVM_L3_PROFILE_MODE_MIN",
        "NOVOVM_L3_PROFILE_MODE_MAX",
        "NOVOVM_L3_PROFILE_FAMILY",
        "NOVOVM_L3_PROFILE_FAMILY_GOVERNANCE",
        "NOVOVM_L3_PROFILE_FAMILY_MIN",
        "NOVOVM_L3_PROFILE_FAMILY_MAX",
        "NOVOVM_L3_POLICY_PROFILE_VERSION",
        "NOVOVM_L3_POLICY_PROFILE_VERSION_GOVERNANCE",
        "NOVOVM_L3_POLICY_PROFILE_DEFAULT",
        "NOVOVM_OVERLAY_ROUTE_MODE",
        "NOVOVM_OVERLAY_ROUTE_REGION",
        "NOVOVM_OVERLAY_ROUTE_STRATEGY",
        "NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP",
        "NOVOVM_OVERLAY_ROUTE_HOP_COUNT",
        "NOVOVM_OVERLAY_ROUTE_MIN_HOPS",
        "NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS",
        "NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS",
        "NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE",
        "NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS",
        "NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES",
        "NOVOVM_OVERLAY_ROUTE_FORCE_STRATEGY",
        "NOVOVM_OVERLAY_ROUTE_FORCE_RELAY_ID",
        "NOVOVM_OVERLAY_ROUTE_FORCE_HOP_COUNT",
        "NOVOVM_OVERLAY_ROUTE_ID"
    )
    foreach ($k in $keys) {
        Remove-Item -LiteralPath ("Env:" + $k) -ErrorAction SilentlyContinue
    }
}

function Invoke-NovovmctlForward {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot,
        [Parameter(Mandatory = $true)]
        [string]$Subcommand,
        [string[]]$BaseArgs = @(),
        [string[]]$IncomingArgs = @()
    )
    $explicitCtl = ""
    $normalized = New-Object System.Collections.Generic.List[string]
    for ($i = 0; $i -lt $IncomingArgs.Count; $i++) {
        $tok = $IncomingArgs[$i]
        if ($tok -match '^(-CtlBinaryFile|--ctl-binary-file)$') {
            if ($i + 1 -lt $IncomingArgs.Count) {
                $explicitCtl = $IncomingArgs[$i + 1]
                $i++
                continue
            }
        }
        $normalized.Add((Convert-CompatArgToken -Token $tok))
    }

    if (Test-MainlineGateIsRequiredForSubcommand -Subcommand $Subcommand) {
        Assert-MainlineGateStatus -RepoRoot $RepoRoot
    }

    Clear-ManualRouteEnvBlacklist
    $ctl = Resolve-NovovmctlBinary -RepoRoot $RepoRoot -ExplicitCtlPath $explicitCtl
    $argv = @($Subcommand) + $BaseArgs + @($normalized.ToArray())

    if ($ctl) {
        & $ctl @argv
        exit $LASTEXITCODE
    }
    & cargo run -p novovmctl -- @argv
    exit $LASTEXITCODE
}
