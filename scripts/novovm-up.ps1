<#
DEPRECATED COMPATIBILITY SHELL

This script is NOT part of the NOVOVM mainline entry path.
Mainline entry is `novovmctl`.

This script exists only for compatibility forwarding.

Do NOT add:
- policy logic
- rollout logic
- lifecycle logic
- daemon logic
- any business logic

Only parameter forwarding and exit-code passthrough are allowed.
#>
[CmdletBinding()]
param(
    [ValidateSet("dev", "prod")]
    [string]$Profile = "dev",
    [ValidateSet("full", "l1", "l2", "l3")]
    [string]$RoleProfile = "full",
    [switch]$NoGateway,
    [switch]$SkipBuild,
    [switch]$BuildBeforeRun,
    [switch]$Daemon,
    [switch]$UseNodeWatchMode,
    [switch]$LeanIo,
    [ValidateRange(1, 3600)]
    [int]$RestartDelaySeconds = 3,
    [ValidateRange(0, 1000000)]
    [int]$MaxRestarts = 0,
    [ValidateSet("none", "backup", "restore", "migrate")]
    [string]$UaStoreAction = "none",
    [ValidateSet("secure", "fast")]
    [string]$OverlayRouteMode = "",
    [string]$OverlayRouteRuntimeFile = "config/runtime/lifecycle/overlay.route.runtime.json",
    [string]$OverlayRouteRuntimeProfile = "",
    [string]$OverlayRouteRelayCandidates = "",
    [string]$OverlayRouteRelayCandidatesByRegion = "",
    [string]$OverlayRouteRelayCandidatesByRole = "",
    [string]$OverlayRouteRelayDirectoryFile = "",
    [ValidateRange(0, 1)]
    [double]$OverlayRouteRelayHealthMin = 0,
    [string]$OverlayRouteRelayPenaltyStateFile = "artifacts/runtime/lifecycle/overlay.relay.penalty.state.json",
    [string]$OverlayRouteRelayPenaltyDelta = "",
    [ValidateRange(0, 1)]
    [double]$OverlayRouteRelayPenaltyRecoverPerRun = 0.05,
    [switch]$EnableAutoProfile,
    [string]$AutoProfileStateFile = "artifacts/runtime/lifecycle/overlay.auto-profile.state.json",
    [string]$AutoProfileProfiles = "prod-cn,prod-eu,prod-us",
    [ValidateRange(1, 86400)]
    [int]$AutoProfileMinHoldSeconds = 180,
    [ValidateRange(0, 1)]
    [double]$AutoProfileSwitchMargin = 0.08,
    [ValidateRange(1, 86400)]
    [int]$AutoProfileSwitchbackCooldownSeconds = 300,
    [ValidateRange(1, 3600)]
    [int]$AutoProfileRecheckSeconds = 30,
    [string]$AutoProfileBinaryPath = "",
    [string]$UaSnapshot = "",
    [string]$GatewayStoreFrom = "",
    [string]$PluginStoreFrom = "",
    [string]$PluginAuditFrom = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [switch]$AllowPublicGatewayBind,
    [string]$SpoolDir = "artifacts/ingress/spool",
    [ValidateRange(10, 60000)]
    [int]$PollMs = 200,
    [ValidateRange(50, 60000)]
    [int]$SupervisorPollMs = 1000,
    [ValidateRange(1, 1000000)]
    [int]$NodeWatchBatchMaxFiles = 1024,
    [switch]$EnableReconcileDaemon,
    [string]$ReconcileSenderAddress = "",
    [string]$ReconcileRpcEndpoint = "http://127.0.0.1:9899",
    [ValidateRange(1, 86400)]
    [int]$ReconcileIntervalSeconds = 15,
    [ValidateRange(1, 3600)]
    [int]$ReconcileRestartDelaySeconds = 3,
    [ValidateRange(0, 1000)]
    [int]$ReconcileReplayMaxPerPayout = 3,
    [ValidateRange(0, 86400)]
    [int]$ReconcileReplayCooldownSec = 30,
    [string]$ReconcileRuntimeFile = "config/runtime/lifecycle/reconcile.runtime.json",
    [string]$ReconcileRuntimeProfile = "",
    [ValidateRange(0, 86400)]
    [int]$IdleExitSeconds = 0,
    [ValidateRange(0, 4294967295)]
    [uint32]$GatewayMaxRequests = 0,
    [string]$GatewayBinaryPath = "",
    [string]$NodeBinaryPath = "",
    [switch]$DryRun,
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# COMPATIBILITY SHELL ONLY
# This script must not contain policy logic, rollout logic, risk evaluation,
# failover logic, or runtime decision logic on the mainline path.
# Mainline execution must go through `novovmctl up`.
# PowerShell here is limited to parameter forwarding and exit-code passthrough.

$script:NovovmCtlUpSupportedParams = @(
    "Profile",
    "RoleProfile",
    "OverlayRouteMode",
    "OverlayRouteRuntimeFile",
    "OverlayRouteRuntimeProfile",
    "OverlayRouteRelayDirectoryFile",
    "EnableAutoProfile",
    "AutoProfileStateFile",
    "AutoProfileProfiles",
    "AutoProfileMinHoldSeconds",
    "AutoProfileSwitchMargin",
    "AutoProfileSwitchbackCooldownSeconds",
    "AutoProfileRecheckSeconds",
    "AutoProfileBinaryPath",
    "NodeBinaryPath",
    "DryRun"
)

function Resolve-NovovmCtlBinary {
    $candidates = @(
        "target/release/novovmctl.exe",
        "target/release/novovmctl",
        "target/debug/novovmctl.exe",
        "target/debug/novovmctl",
        "crates/novovmctl/target/release/novovmctl.exe",
        "crates/novovmctl/target/release/novovmctl",
        "crates/novovmctl/target/debug/novovmctl.exe",
        "crates/novovmctl/target/debug/novovmctl"
    )

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    throw "novovmctl not found in default candidate paths"
}

function Invoke-NovovmCtlUpBridge {
    $unsupported = @($PSBoundParameters.Keys | Where-Object { $script:NovovmCtlUpSupportedParams -notcontains $_ })
    if ($unsupported.Count -gt 0) {
        throw ("novovm-up.ps1 compatibility shell only supports novovmctl-backed parameters. Unsupported legacy parameters: " + ($unsupported -join ", "))
    }

    $novovmctl = Resolve-NovovmCtlBinary
    $argsList = New-Object System.Collections.Generic.List[string]
    $argsList.Add("up")
    $argsList.Add("--profile")
    $argsList.Add($Profile)
    $argsList.Add("--role-profile")
    $argsList.Add($RoleProfile)

    if (-not [string]::IsNullOrWhiteSpace($OverlayRouteRuntimeFile)) {
        $argsList.Add("--overlay-route-runtime-file")
        $argsList.Add($OverlayRouteRuntimeFile)
    }
    if (-not [string]::IsNullOrWhiteSpace($OverlayRouteRuntimeProfile)) {
        $argsList.Add("--overlay-route-runtime-profile")
        $argsList.Add($OverlayRouteRuntimeProfile)
    }
    if (-not [string]::IsNullOrWhiteSpace($OverlayRouteMode)) {
        $argsList.Add("--overlay-route-mode")
        $argsList.Add($OverlayRouteMode)
    }
    if (-not [string]::IsNullOrWhiteSpace($OverlayRouteRelayDirectoryFile)) {
        $argsList.Add("--overlay-route-relay-directory-file")
        $argsList.Add($OverlayRouteRelayDirectoryFile)
    }
    if (-not [string]::IsNullOrWhiteSpace($AutoProfileStateFile)) {
        $argsList.Add("--auto-profile-state-file")
        $argsList.Add($AutoProfileStateFile)
    }
    if (-not [string]::IsNullOrWhiteSpace($AutoProfileProfiles)) {
        $argsList.Add("--auto-profile-profiles")
        $argsList.Add($AutoProfileProfiles)
    }
    if ($AutoProfileMinHoldSeconds -gt 0) {
        $argsList.Add("--auto-profile-min-hold-seconds")
        $argsList.Add([string]$AutoProfileMinHoldSeconds)
    }
    if ($AutoProfileSwitchMargin -ge 0) {
        $argsList.Add("--auto-profile-switch-margin")
        $argsList.Add([string]$AutoProfileSwitchMargin)
    }
    if ($AutoProfileSwitchbackCooldownSeconds -gt 0) {
        $argsList.Add("--auto-profile-switchback-cooldown-seconds")
        $argsList.Add([string]$AutoProfileSwitchbackCooldownSeconds)
    }
    if ($AutoProfileRecheckSeconds -gt 0) {
        $argsList.Add("--auto-profile-recheck-seconds")
        $argsList.Add([string]$AutoProfileRecheckSeconds)
    }
    if (-not [string]::IsNullOrWhiteSpace($AutoProfileBinaryPath)) {
        $argsList.Add("--policy-cli-binary-file")
        $argsList.Add($AutoProfileBinaryPath)
    }
    if (-not [string]::IsNullOrWhiteSpace($NodeBinaryPath)) {
        $argsList.Add("--node-binary-file")
        $argsList.Add($NodeBinaryPath)
    }

    $argsList.Add("--auto-profile-enabled")
    if ($EnableAutoProfile) {
        $argsList.Add("true")
    } else {
        $argsList.Add("false")
    }

    if ($DryRun) {
        $argsList.Add("--dry-run")
    }

    Write-Host ("[compat-shell] forwarding to novovmctl: {0} {1}" -f $novovmctl, ($argsList -join " "))
    & $novovmctl @argsList
    exit $LASTEXITCODE
}

Invoke-NovovmCtlUpBridge

function Set-EnvIfEmpty {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$Value
    )
    $item = Get-Item -Path ("Env:" + $Name) -ErrorAction SilentlyContinue
    if ($null -eq $item -or [string]::IsNullOrWhiteSpace($item.Value)) {
        Set-Item -Path ("Env:" + $Name) -Value $Value
    }
}

function Set-EnvForce {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$Value
    )
    Set-Item -Path ("Env:" + $Name) -Value $Value
}

function Set-EnvIntAtLeast {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [long]$Min,
        [Parameter(Mandatory = $true)]
        [long]$DefaultValue
    )
    $item = Get-Item -Path ("Env:" + $Name) -ErrorAction SilentlyContinue
    $value = $DefaultValue
    if ($null -ne $item -and -not [string]::IsNullOrWhiteSpace($item.Value)) {
        $parsed = 0L
        if ([long]::TryParse($item.Value, [ref]$parsed)) {
            $value = $parsed
        }
    }
    if ($value -lt $Min) {
        $value = $Min
    }
    Set-Item -Path ("Env:" + $Name) -Value ([string]$value)
}

function Convert-ToRelayCandidateList {
    param(
        [Parameter(Mandatory = $false)]
        $Value
    )
    $out = @()
    if ($null -eq $Value) {
        return $out
    }
    if ($Value -is [System.Array]) {
        foreach ($item in $Value) {
            if ($null -ne $item) {
                $text = ([string]$item).Trim()
                if (-not [string]::IsNullOrWhiteSpace($text)) {
                    $out += $text
                }
            }
        }
        return $out
    }
    return ([string]$Value).Split(@(",", ";"), [System.StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { $_.Trim() } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
}

function Get-ObjectPropertyValueCI {
    param(
        [Parameter(Mandatory = $false)]
        $Object,
        [Parameter(Mandatory = $true)]
        [string]$Name
    )
    if ($null -eq $Object) {
        return $null
    }
    if ($Object -is [System.Collections.IDictionary]) {
        foreach ($key in $Object.Keys) {
            if ([string]::Equals([string]$key, $Name, [System.StringComparison]::OrdinalIgnoreCase)) {
                return $Object[$key]
            }
        }
        return $null
    }
    foreach ($prop in $Object.PSObject.Properties) {
        if ([string]::Equals([string]$prop.Name, $Name, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $prop.Value
        }
    }
    return $null
}

function Resolve-TemplateRelayCandidates {
    param(
        [Parameter(Mandatory = $false)]
        $TemplateObject,
        [Parameter(Mandatory = $false)]
        [string]$Region,
        [Parameter(Mandatory = $false)]
        [string]$RoleProfile
    )
    $regionMap = Get-ObjectPropertyValueCI -Object $TemplateObject -Name "relay_candidates_by_region"
    if ($null -ne $regionMap -and -not [string]::IsNullOrWhiteSpace($Region)) {
        $regionCandidatesRaw = Get-ObjectPropertyValueCI -Object $regionMap -Name $Region
        if ($null -eq $regionCandidatesRaw) {
            $regionCandidatesRaw = Get-ObjectPropertyValueCI -Object $regionMap -Name "default"
        }
        $regionCandidates = Convert-ToRelayCandidateList -Value $regionCandidatesRaw
        if ($regionCandidates.Count -gt 0) {
            return $regionCandidates
        }
    }

    $roleMap = Get-ObjectPropertyValueCI -Object $TemplateObject -Name "relay_candidates_by_role"
    if ($null -ne $roleMap -and -not [string]::IsNullOrWhiteSpace($RoleProfile)) {
        $roleCandidatesRaw = Get-ObjectPropertyValueCI -Object $roleMap -Name $RoleProfile
        if ($null -eq $roleCandidatesRaw) {
            $roleCandidatesRaw = Get-ObjectPropertyValueCI -Object $roleMap -Name "default"
        }
        $roleCandidates = Convert-ToRelayCandidateList -Value $roleCandidatesRaw
        if ($roleCandidates.Count -gt 0) {
            return $roleCandidates
        }
    }

    $flatCandidatesRaw = Get-ObjectPropertyValueCI -Object $TemplateObject -Name "relay_candidates"
    return Convert-ToRelayCandidateList -Value $flatCandidatesRaw
}

function Convert-JsonTextToObjectOrNull {
    param(
        [Parameter(Mandatory = $false)]
        [string]$Raw
    )
    if ([string]::IsNullOrWhiteSpace($Raw)) {
        return $null
    }
    try {
        return ($Raw | ConvertFrom-Json -ErrorAction Stop)
    } catch {
        return $null
    }
}

function Resolve-RelayCandidatesFromDirectory {
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRootPath,
        [Parameter(Mandatory = $false)]
        [string]$DirectoryFile,
        [Parameter(Mandatory = $false)]
        [string]$Region,
        [Parameter(Mandatory = $false)]
        [string]$RoleProfileValue,
        [Parameter(Mandatory = $false)]
        [double]$HealthMin,
        [Parameter(Mandatory = $false)]
        [int]$MaxCount,
        [Parameter(Mandatory = $false)]
        [System.Collections.IDictionary]$PenaltyMap
    )
    if ([string]::IsNullOrWhiteSpace($DirectoryFile)) {
        return @()
    }
    $resolved = $DirectoryFile
    if (-not [System.IO.Path]::IsPathRooted($resolved)) {
        $resolved = Join-Path $RepoRootPath $resolved
    }
    if (-not (Test-Path -LiteralPath $resolved)) {
        return @()
    }
    $raw = Get-Content -LiteralPath $resolved -Raw -ErrorAction SilentlyContinue
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return @()
    }
    $cfg = Convert-JsonTextToObjectOrNull -Raw $raw
    if ($null -eq $cfg) {
        return @()
    }
    $relays = Get-ObjectPropertyValueCI -Object $cfg -Name "relays"
    if ($null -eq $relays -or -not ($relays -is [System.Array])) {
        return @()
    }
    $effectiveRegion = "global"
    if (-not [string]::IsNullOrWhiteSpace($Region)) {
        $effectiveRegion = $Region.Trim()
    }
    $effectiveRole = ""
    if (-not [string]::IsNullOrWhiteSpace($RoleProfileValue)) {
        $effectiveRole = $RoleProfileValue.Trim()
    }
    $effectiveHealthMin = [Math]::Min(1, [Math]::Max(0, [double]$HealthMin))
    $limit = [Math]::Max(1, $MaxCount)
    $picked = @()
    foreach ($relay in $relays) {
        if ($null -eq $relay) {
            continue
        }
        $idRaw = Get-ObjectPropertyValueCI -Object $relay -Name "id"
        if ($null -eq $idRaw) {
            continue
        }
        $relayId = ([string]$idRaw).Trim()
        if ([string]::IsNullOrWhiteSpace($relayId)) {
            continue
        }
        $enabledRaw = Get-ObjectPropertyValueCI -Object $relay -Name "enabled"
        if ($null -ne $enabledRaw -and -not [bool]$enabledRaw) {
            continue
        }
        $relayRegionRaw = Get-ObjectPropertyValueCI -Object $relay -Name "region"
        if ($null -ne $relayRegionRaw -and -not [string]::IsNullOrWhiteSpace([string]$relayRegionRaw)) {
            $relayRegion = ([string]$relayRegionRaw).Trim()
            if (
                -not [string]::Equals($relayRegion, "global", [System.StringComparison]::OrdinalIgnoreCase) -and
                -not [string]::Equals($relayRegion, $effectiveRegion, [System.StringComparison]::OrdinalIgnoreCase)
            ) {
                continue
            }
        }
        $rolesRaw = Get-ObjectPropertyValueCI -Object $relay -Name "roles"
        if ($null -ne $rolesRaw -and -not [string]::IsNullOrWhiteSpace($effectiveRole)) {
            $roleList = Convert-ToRelayCandidateList -Value $rolesRaw
            if ($roleList.Count -gt 0) {
                $roleMatched = $false
                foreach ($roleItem in $roleList) {
                    if (
                        [string]::Equals($roleItem, $effectiveRole, [System.StringComparison]::OrdinalIgnoreCase) -or
                        [string]::Equals($roleItem, "default", [System.StringComparison]::OrdinalIgnoreCase)
                    ) {
                        $roleMatched = $true
                        break
                    }
                }
                if (-not $roleMatched) {
                    continue
                }
            }
        }
        $score = 1.0
        $healthRaw = Get-ObjectPropertyValueCI -Object $relay -Name "health"
        if ($null -ne $healthRaw) {
            $parsed = 0.0
            if ([double]::TryParse([string]$healthRaw, [ref]$parsed)) {
                $score = $parsed
            }
        }
        $penalty = 0.0
        if ($null -ne $PenaltyMap -and $PenaltyMap.Contains($relayId)) {
            $penaltyParsed = 0.0
            if ([double]::TryParse([string]$PenaltyMap[$relayId], [ref]$penaltyParsed)) {
                $penalty = [Math]::Min(1, [Math]::Max(0, $penaltyParsed))
            }
        }
        $score = [Math]::Max(0, ($score - $penalty))
        if ($score -lt $effectiveHealthMin) {
            continue
        }
        $picked += [pscustomobject]@{
            id = $relayId
            score = $score
        }
    }
    if ($picked.Count -eq 0) {
        return @()
    }
    $sorted = $picked | Sort-Object -Property @{ Expression = "score"; Descending = $true }, @{ Expression = "id"; Descending = $false }
    return @($sorted | Select-Object -First $limit | ForEach-Object { [string]$_.id })
}

function Load-RelayPenaltyState {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StatePath
    )
    $result = @{}
    if (-not (Test-Path -LiteralPath $StatePath)) {
        return $result
    }
    $raw = Get-Content -LiteralPath $StatePath -Raw -ErrorAction SilentlyContinue
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $result
    }
    $obj = Convert-JsonTextToObjectOrNull -Raw $raw
    if ($null -eq $obj) {
        return $result
    }
    if ($obj -is [System.Collections.IDictionary]) {
        foreach ($key in $obj.Keys) {
            $value = 0.0
            if ([double]::TryParse([string]$obj[$key], [ref]$value)) {
                $result[[string]$key] = [Math]::Min(1, [Math]::Max(0, $value))
            }
        }
        return $result
    }
    foreach ($prop in $obj.PSObject.Properties) {
        $value = 0.0
        if ([double]::TryParse([string]$prop.Value, [ref]$value)) {
            $result[[string]$prop.Name] = [Math]::Min(1, [Math]::Max(0, $value))
        }
    }
    return $result
}

function Save-RelayPenaltyState {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StatePath,
        [Parameter(Mandatory = $true)]
        [System.Collections.IDictionary]$PenaltyMap
    )
    $parent = Split-Path -Parent $StatePath
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $obj = [ordered]@{}
    foreach ($entry in $PenaltyMap.GetEnumerator()) {
        $value = 0.0
        if ([double]::TryParse([string]$entry.Value, [ref]$value)) {
            $normalized = [Math]::Min(1, [Math]::Max(0, $value))
            if ($normalized -gt 0) {
                $obj[[string]$entry.Key] = [Math]::Round($normalized, 6)
            }
        }
    }
    ($obj | ConvertTo-Json -Depth 4) | Set-Content -LiteralPath $StatePath -Encoding UTF8
}

function Resolve-OverlayRouteMode {
    param(
        [string]$ProfileValue,
        [string]$CliMode
    )
    $raw = $CliMode
    if ([string]::IsNullOrWhiteSpace($raw)) {
        $envMode = Get-Item -Path "Env:NOVOVM_OVERLAY_ROUTE_MODE" -ErrorAction SilentlyContinue
        if ($null -ne $envMode -and -not [string]::IsNullOrWhiteSpace($envMode.Value)) {
            $raw = $envMode.Value
        }
    }
    if ([string]::IsNullOrWhiteSpace($raw)) {
        if ($ProfileValue -eq "prod") {
            return "secure"
        }
        return "fast"
    }
    $mode = $raw.Trim().ToLowerInvariant()
    if ($mode -ne "secure" -and $mode -ne "fast") {
        throw ("invalid overlay route mode: " + $raw + " (valid: secure|fast)")
    }
    return $mode
}

function Apply-OverlayRouteModeEnv {
    param(
        [string]$Mode
    )
    Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_MODE" -Value $Mode
    Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_ROUTE_REGION" -Value "global"
    if ($Mode -eq "secure") {
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_STRATEGY" -Value "multi_hop"
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP" -Value "1"
        Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_ROUTE_HOP_COUNT" -Value "3"
        Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_ROUTE_MIN_HOPS" -Value "2"
        Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS" -Value "30"
        Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS" -Value "8"
        Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE" -Value "3"
        Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS" -Value "60"
        Set-EnvIntAtLeast -Name "NOVOVM_OVERLAY_ROUTE_HOP_COUNT" -Min 3 -DefaultValue 3
        Set-EnvIntAtLeast -Name "NOVOVM_OVERLAY_ROUTE_MIN_HOPS" -Min 2 -DefaultValue 2
        Set-EnvIntAtLeast -Name "NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS" -Min 1 -DefaultValue 30
        Set-EnvIntAtLeast -Name "NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS" -Min 1 -DefaultValue 8
        Set-EnvIntAtLeast -Name "NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE" -Min 1 -DefaultValue 3
        Set-EnvIntAtLeast -Name "NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS" -Min 1 -DefaultValue 60
        return
    }
    Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_STRATEGY" -Value "direct"
    Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP" -Value "0"
    Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_HOP_COUNT" -Value "1"
    Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_MIN_HOPS" -Value "1"
    Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS" -Value "300"
    Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS" -Value "1"
    Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE" -Value "1"
    Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS" -Value "300"
}

function Test-IsLoopbackBind {
    param(
        [string]$BindAddress
    )
    if ([string]::IsNullOrWhiteSpace($BindAddress)) {
        return $false
    }
    $raw = $BindAddress.Trim()
    $host = $raw
    $portIndex = $raw.LastIndexOf(":")
    if ($portIndex -gt 0) {
        $host = $raw.Substring(0, $portIndex)
    }
    $host = $host.Trim()
    if ($host.StartsWith("[") -and $host.EndsWith("]")) {
        $host = $host.Substring(1, $host.Length - 2)
    }
    $hostLower = $host.ToLowerInvariant()
    if ($hostLower -eq "127.0.0.1" -or $hostLower -eq "localhost" -or $hostLower -eq "::1") {
        return $true
    }
    return $false
}

function Resolve-ReconcileRuntimeTemplate {
    param(
        [string]$RepoRootPath,
        [string]$TemplateFilePath,
        [string]$TemplateProfile
    )
    if ([string]::IsNullOrWhiteSpace($TemplateFilePath)) {
        return $null
    }
    $resolved = $TemplateFilePath
    if (-not [System.IO.Path]::IsPathRooted($resolved)) {
        $resolved = Join-Path $RepoRootPath $resolved
    }
    if (-not (Test-Path -LiteralPath $resolved)) {
        return $null
    }
    $raw = Get-Content -LiteralPath $resolved -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $null
    }
    $cfg = $raw | ConvertFrom-Json -ErrorAction Stop
    $selected = $cfg
    $profileUsed = ""
    if ($null -ne $cfg.profiles) {
        $wanted = $TemplateProfile
        if ([string]::IsNullOrWhiteSpace($wanted)) {
            $wanted = "default"
        }
        $p = $cfg.profiles.PSObject.Properties[$wanted]
        if ($null -eq $p) {
            $p = $cfg.profiles.PSObject.Properties["default"]
            if ($null -ne $p) {
                $profileUsed = "default"
            }
        } else {
            $profileUsed = $wanted
        }
        if ($null -eq $p) {
            return $null
        }
        $selected = $p.Value
    } else {
        if ([string]::IsNullOrWhiteSpace($TemplateProfile)) {
            $profileUsed = "inline"
        } else {
            $profileUsed = $TemplateProfile
        }
    }
    return [pscustomobject]@{
        path = $resolved
        profile = $profileUsed
        value = $selected
    }
}

function Invoke-OverlayAutoProfileSelector {
    param(
        [string]$RepoRootPath,
        [string]$RuntimeFilePath,
        [string]$CurrentProfile,
        [string]$StateFilePath,
        [string]$ProfilesValue,
        [int]$MinHoldSeconds,
        [double]$SwitchMargin,
        [int]$SwitchbackCooldownSeconds,
        [int]$RecheckSeconds,
        [string]$BinaryPath
    )
    if ([string]::IsNullOrWhiteSpace($RuntimeFilePath)) {
        throw "EnableAutoProfile requires OverlayRouteRuntimeFile"
    }
    $runtimeResolved = $RuntimeFilePath
    if (-not [System.IO.Path]::IsPathRooted($runtimeResolved)) {
        $runtimeResolved = Join-Path $RepoRootPath $runtimeResolved
    }
    if (-not (Test-Path -LiteralPath $runtimeResolved)) {
        throw ("overlay auto profile runtime file not found: " + $runtimeResolved)
    }

    $stateResolved = $StateFilePath
    if ([string]::IsNullOrWhiteSpace($stateResolved)) {
        $stateResolved = "artifacts/runtime/lifecycle/overlay.auto-profile.state.json"
    }
    if (-not [System.IO.Path]::IsPathRooted($stateResolved)) {
        $stateResolved = Join-Path $RepoRootPath $stateResolved
    }
    $stateParent = Split-Path -Parent $stateResolved
    if (-not [string]::IsNullOrWhiteSpace($stateParent)) {
        New-Item -ItemType Directory -Force -Path $stateParent | Out-Null
    }

    $selectorArgs = @(
        "--runtime-file", $runtimeResolved,
        "--state-file", $stateResolved,
        "--current-profile", $CurrentProfile,
        "--profiles", $ProfilesValue,
        "--min-hold-seconds", ([string]$MinHoldSeconds),
        "--switch-margin", ([string]$SwitchMargin),
        "--switchback-cooldown-seconds", ([string]$SwitchbackCooldownSeconds),
        "--recheck-seconds", ([string]$RecheckSeconds)
    )

    $resolvedBinaryPath = $BinaryPath
    if (-not [string]::IsNullOrWhiteSpace($resolvedBinaryPath) -and -not [System.IO.Path]::IsPathRooted($resolvedBinaryPath)) {
        $resolvedBinaryPath = Join-Path $RepoRootPath $resolvedBinaryPath
    }
    $useUnifiedCli = $false
    if ([string]::IsNullOrWhiteSpace($resolvedBinaryPath)) {
        $runningOnWindows = ([System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT)
        $rolloutPolicyCandidates = @(
            (Join-Path $RepoRootPath ("target/release/novovm-rollout-policy" + $(if ($runningOnWindows) { ".exe" } else { "" }))),
            (Join-Path $RepoRootPath "target/release/novovm-rollout-policy"),
            (Join-Path $RepoRootPath ("target/debug/novovm-rollout-policy" + $(if ($runningOnWindows) { ".exe" } else { "" }))),
            (Join-Path $RepoRootPath "target/debug/novovm-rollout-policy")
        )
        foreach ($candidate in @($rolloutPolicyCandidates)) {
            if (-not [string]::IsNullOrWhiteSpace($candidate) -and (Test-Path -LiteralPath $candidate)) {
                $resolvedBinaryPath = $candidate
                $useUnifiedCli = $true
                break
            }
        }
        if ([string]::IsNullOrWhiteSpace($resolvedBinaryPath)) {
            $cmd = Get-Command -Name "novovm-rollout-policy" -ErrorAction SilentlyContinue
            if ($null -ne $cmd) {
                $cmdPath = [string]$cmd.Source
                if ([string]::IsNullOrWhiteSpace($cmdPath)) {
                    $cmdPath = [string]$cmd.Path
                }
                if (-not [string]::IsNullOrWhiteSpace($cmdPath)) {
                    $resolvedBinaryPath = $cmdPath
                    $useUnifiedCli = $true
                }
            }
        }
    } elseif ([System.IO.Path]::GetFileNameWithoutExtension($resolvedBinaryPath) -eq 'novovm-rollout-policy') {
        $useUnifiedCli = $true
    }

    $stdout = @()
    $invokedBy = ""
    $exitCode = 0
    if (-not [string]::IsNullOrWhiteSpace($resolvedBinaryPath) -and ((-not $useUnifiedCli) -or (Test-Path -LiteralPath $resolvedBinaryPath))) {
        if ($useUnifiedCli) {
            $stdout = & $resolvedBinaryPath "overlay-auto-profile" @selectorArgs
            $invokedBy = ($resolvedBinaryPath + " overlay-auto-profile")
        } else {
            $stdout = & $resolvedBinaryPath @selectorArgs
            $invokedBy = $resolvedBinaryPath
        }
        $exitCode = $LASTEXITCODE
    } else {
        $manifestPath = Join-Path $RepoRootPath "crates/novovm-rollout-policy/Cargo.toml"
        if (-not (Test-Path -LiteralPath $manifestPath)) {
            throw ("overlay auto profile manifest not found: " + $manifestPath)
        }
        $stdout = & cargo run --quiet --manifest-path $manifestPath --bin novovm-rollout-policy -- overlay-auto-profile @selectorArgs
        $exitCode = $LASTEXITCODE
        $invokedBy = "cargo run --manifest-path crates/novovm-rollout-policy/Cargo.toml --bin novovm-rollout-policy -- overlay-auto-profile"
    }
    if ($exitCode -ne 0) {
        throw ("overlay auto profile selector failed: exit_code=" + $exitCode)
    }

    $stdoutText = [string]::Join([Environment]::NewLine, @($stdout)).Trim()
    if ([string]::IsNullOrWhiteSpace($stdoutText)) {
        throw "overlay auto profile selector returned empty output"
    }
    $decision = $stdoutText | ConvertFrom-Json -ErrorAction Stop
    return [pscustomobject]@{
        decision = $decision
        runtime_file = $runtimeResolved
        state_file = $stateResolved
        invoked_by = $invokedBy
    }
}

function Invoke-UaStoreAction {
    param(
        [string]$RepoRootPath,
        [string]$Action,
        [string]$Snapshot,
        [string]$GatewayFrom,
        [string]$PluginFrom,
        [string]$AuditFrom,
        [switch]$BypassForce
    )
    $uaScript = Join-Path $RepoRootPath "scripts/novovm-ua-prod-store.ps1"
    if (-not (Test-Path -LiteralPath $uaScript)) {
        throw ("Missing UA store script: " + $uaScript)
    }
    $args = @{
        Action = $Action
        RepoRoot = $RepoRootPath
    }
    if ($Snapshot) {
        $args["Snapshot"] = $Snapshot
    }
    if ($GatewayFrom) {
        $args["GatewayStoreFrom"] = $GatewayFrom
    }
    if ($PluginFrom) {
        $args["PluginStoreFrom"] = $PluginFrom
    }
    if ($AuditFrom) {
        $args["PluginAuditFrom"] = $AuditFrom
    }
    if ($BypassForce) {
        $args["Force"] = $true
    }
    & $uaScript @args
}

function Invoke-PipelineOnce {
    param(
        [string]$PipelineScriptPath,
        [switch]$NoGatewayMode,
        [switch]$SkipBuildMode,
        [switch]$UseNodeWatchModeFlag,
        [switch]$LeanIoFlag,
        [switch]$EnableReconcileDaemonFlag,
        [string]$ReconcileSenderAddressValue,
        [string]$ReconcileRpcEndpointValue,
        [int]$ReconcileIntervalSecondsValue,
        [int]$ReconcileRestartDelaySecondsValue,
        [int]$ReconcileReplayMaxPerPayoutValue,
        [int]$ReconcileReplayCooldownSecValue,
        [string]$BindValue,
        [string]$SpoolDirValue,
        [int]$PollMsValue,
        [int]$SupervisorPollMsValue,
        [int]$NodeWatchBatchMaxFilesValue,
        [int]$IdleExitSecondsValue,
        [uint32]$GatewayMaxRequestsValue,
        [string]$GatewayBinaryPathValue,
        [string]$NodeBinaryPathValue
    )
    $invokeArgs = @{}
    if ($NoGatewayMode) {
        $invokeArgs["SkipGatewayStart"] = $true
    }
    if ($SkipBuildMode) {
        $invokeArgs["SkipBuild"] = $true
    }
    if ($UseNodeWatchModeFlag) {
        $invokeArgs["UseNodeWatchMode"] = $true
    }
    if ($LeanIoFlag) {
        $invokeArgs["LeanIo"] = $true
    }
    if ($EnableReconcileDaemonFlag) {
        $invokeArgs["EnableReconcileDaemon"] = $true
        $invokeArgs["ReconcileSenderAddress"] = $ReconcileSenderAddressValue
        $invokeArgs["ReconcileRpcEndpoint"] = $ReconcileRpcEndpointValue
        $invokeArgs["ReconcileIntervalSeconds"] = $ReconcileIntervalSecondsValue
        $invokeArgs["ReconcileRestartDelaySeconds"] = $ReconcileRestartDelaySecondsValue
        $invokeArgs["ReconcileReplayMaxPerPayout"] = $ReconcileReplayMaxPerPayoutValue
        $invokeArgs["ReconcileReplayCooldownSec"] = $ReconcileReplayCooldownSecValue
    }
    $invokeArgs["GatewayBind"] = $BindValue
    $invokeArgs["SpoolDir"] = $SpoolDirValue
    $invokeArgs["PollMs"] = $PollMsValue
    $invokeArgs["SupervisorPollMs"] = $SupervisorPollMsValue
    $invokeArgs["NodeWatchBatchMaxFiles"] = $NodeWatchBatchMaxFilesValue
    $invokeArgs["IdleExitSeconds"] = $IdleExitSecondsValue
    $invokeArgs["GatewayMaxRequests"] = $GatewayMaxRequestsValue
    if (-not [string]::IsNullOrWhiteSpace($GatewayBinaryPathValue)) {
        $invokeArgs["GatewayBinaryPath"] = $GatewayBinaryPathValue
    }
    if (-not [string]::IsNullOrWhiteSpace($NodeBinaryPathValue)) {
        $invokeArgs["NodeBinaryPath"] = $NodeBinaryPathValue
    }
    & $PipelineScriptPath @invokeArgs
}

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

if ($UaStoreAction -ne "none") {
    Invoke-UaStoreAction -RepoRootPath $repoRoot -Action $UaStoreAction -Snapshot $UaSnapshot -GatewayFrom $GatewayStoreFrom -PluginFrom $PluginStoreFrom -AuditFrom $PluginAuditFrom -BypassForce:$Force
    exit 0
}

Set-EnvIfEmpty -Name "NOVOVM_NODE_MODE" -Value "full"
Set-EnvIfEmpty -Name "NOVOVM_EXEC_PATH" -Value "ffi_v2"
Set-EnvIfEmpty -Name "NOVOVM_HOST_ADMISSION" -Value "disabled"
if ($env:COMPUTERNAME) {
    Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_NODE_ID" -Value $env:COMPUTERNAME
} elseif ($env:HOSTNAME) {
    Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_NODE_ID" -Value $env:HOSTNAME
} else {
    Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_NODE_ID" -Value "local"
}
Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_SESSION_ID" -Value ("sess-" + [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
Set-EnvIfEmpty -Name "NOVOVM_L1L4_ANCHOR_LEDGER_ENABLED" -Value "0"
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_SPOOL_DIR" -Value $SpoolDir
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_UA_STORE_BACKEND" -Value "rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_UA_STORE_PATH" -Value "artifacts/gateway/unified-account-router.rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND" -Value "rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_ETH_TX_INDEX_PATH" -Value "artifacts/gateway/eth-tx-index.rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND" -Value "rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH" -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND" -Value "rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH" -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ALLOW_NON_PROD_PLUGIN_BACKEND" -Value "0"

if ($Profile -eq "prod") {
    Set-EnvForce -Name "NOVOVM_NODE_MODE" -Value "full"
    Set-EnvForce -Name "NOVOVM_EXEC_PATH" -Value "ffi_v2"
    Set-EnvForce -Name "NOVOVM_HOST_ADMISSION" -Value "disabled"
    Set-EnvForce -Name "NOVOVM_L1L4_ANCHOR_PATH" -Value "artifacts/l1/l1l4-anchor.jsonl"
    Set-EnvForce -Name "NOVOVM_L1L4_ANCHOR_LEDGER_ENABLED" -Value "1"
    Set-EnvIfEmpty -Name "NOVOVM_L1L4_ANCHOR_LEDGER_KEY_PREFIX" -Value "ledger:l1:l1l4_anchor:v1:"
    Set-EnvForce -Name "NOVOVM_GATEWAY_UA_STORE_BACKEND" -Value "rocksdb"
    Set-EnvForce -Name "NOVOVM_GATEWAY_UA_STORE_PATH" -Value "artifacts/gateway/unified-account-router.rocksdb"
    Set-EnvForce -Name "NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND" -Value "rocksdb"
    Set-EnvForce -Name "NOVOVM_GATEWAY_ETH_TX_INDEX_PATH" -Value "artifacts/gateway/eth-tx-index.rocksdb"
    Set-EnvForce -Name "NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND" -Value "rocksdb"
    Set-EnvForce -Name "NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH" -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.rocksdb"
    Set-EnvForce -Name "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND" -Value "rocksdb"
    Set-EnvForce -Name "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH" -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.rocksdb"
    Set-EnvForce -Name "NOVOVM_ALLOW_NON_PROD_UA_BACKEND" -Value "0"
    Set-EnvForce -Name "NOVOVM_ALLOW_NON_PROD_GATEWAY_BACKEND" -Value "0"
    Set-EnvForce -Name "NOVOVM_ALLOW_NON_PROD_PLUGIN_BACKEND" -Value "0"
    Set-EnvIfEmpty -Name "RUST_LOG" -Value "info"
} else {
    Set-EnvIfEmpty -Name "RUST_LOG" -Value "debug"
}

$overlayTemplateProfile = $OverlayRouteRuntimeProfile
if ([string]::IsNullOrWhiteSpace($overlayTemplateProfile)) {
    $overlayTemplateProfile = $Profile
}
if ($EnableAutoProfile) {
    $autoProfileResult = Invoke-OverlayAutoProfileSelector `
        -RepoRootPath $repoRoot `
        -RuntimeFilePath $OverlayRouteRuntimeFile `
        -CurrentProfile $overlayTemplateProfile `
        -StateFilePath $AutoProfileStateFile `
        -ProfilesValue $AutoProfileProfiles `
        -MinHoldSeconds $AutoProfileMinHoldSeconds `
        -SwitchMargin $AutoProfileSwitchMargin `
        -SwitchbackCooldownSeconds $AutoProfileSwitchbackCooldownSeconds `
        -RecheckSeconds $AutoProfileRecheckSeconds `
        -BinaryPath $AutoProfileBinaryPath
    $autoDecision = $autoProfileResult.decision
    if ($null -ne $autoDecision.selected_profile -and -not [string]::IsNullOrWhiteSpace([string]$autoDecision.selected_profile)) {
        $overlayTemplateProfile = [string]$autoDecision.selected_profile
    }
    Write-Host ("novovm_up_overlay_auto_profile: enabled=true selected={0} action={1} reason={2} score={3} previous={4} invoked_by={5}" -f `
            $overlayTemplateProfile, `
            [string]$autoDecision.action, `
            [string]$autoDecision.reason, `
            [string]$autoDecision.score, `
            [string]$autoDecision.previous_profile, `
            [string]$autoProfileResult.invoked_by)
}
$overlayRouteTemplate = Resolve-ReconcileRuntimeTemplate -RepoRootPath $repoRoot -TemplateFilePath $OverlayRouteRuntimeFile -TemplateProfile $overlayTemplateProfile
if ($null -ne $overlayRouteTemplate) {
    $ot = $overlayRouteTemplate.value
    if (-not $PSBoundParameters.ContainsKey("OverlayRouteMode") -and $null -ne $ot.mode -and -not [string]::IsNullOrWhiteSpace([string]$ot.mode)) {
        $OverlayRouteMode = [string]$ot.mode
    }
    if ($null -ne $ot.region -and -not [string]::IsNullOrWhiteSpace([string]$ot.region)) {
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_REGION" -Value ([string]$ot.region)
    }
    if ($null -ne $ot.relay_buckets) {
        $relayBuckets = [Math]::Max(1, [int]$ot.relay_buckets)
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_BUCKETS" -Value ([string]$relayBuckets)
    }
    if ($null -ne $ot.relay_set_size) {
        $relaySetSize = [Math]::Min(64, [Math]::Max(1, [int]$ot.relay_set_size))
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE" -Value ([string]$relaySetSize)
    }
    if ($null -ne $ot.relay_rotate_seconds) {
        $relayRotateSec = [Math]::Min(86400, [Math]::Max(1, [int]$ot.relay_rotate_seconds))
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_ROTATE_SECONDS" -Value ([string]$relayRotateSec)
    }
    if (-not $PSBoundParameters.ContainsKey("OverlayRouteRelayDirectoryFile") -and $null -ne $ot.relay_directory_file -and -not [string]::IsNullOrWhiteSpace([string]$ot.relay_directory_file)) {
        $OverlayRouteRelayDirectoryFile = [string]$ot.relay_directory_file
    }
    if (-not $PSBoundParameters.ContainsKey("OverlayRouteRelayHealthMin") -and $null -ne $ot.relay_health_min) {
        $relayHealthMinParsed = 0.0
        if ([double]::TryParse([string]$ot.relay_health_min, [ref]$relayHealthMinParsed)) {
            $OverlayRouteRelayHealthMin = [Math]::Min(1, [Math]::Max(0, $relayHealthMinParsed))
        }
    }
    if (-not $PSBoundParameters.ContainsKey("OverlayRouteRelayPenaltyStateFile") -and $null -ne $ot.relay_penalty_state_file -and -not [string]::IsNullOrWhiteSpace([string]$ot.relay_penalty_state_file)) {
        $OverlayRouteRelayPenaltyStateFile = [string]$ot.relay_penalty_state_file
    }
    if (-not $PSBoundParameters.ContainsKey("OverlayRouteRelayPenaltyRecoverPerRun") -and $null -ne $ot.relay_penalty_recover_per_run) {
        $relayRecoverParsed = 0.0
        if ([double]::TryParse([string]$ot.relay_penalty_recover_per_run, [ref]$relayRecoverParsed)) {
            $OverlayRouteRelayPenaltyRecoverPerRun = [Math]::Min(1, [Math]::Max(0, $relayRecoverParsed))
        }
    }
    $effectiveOverlayRegion = "global"
    $overlayRegionEnv = Get-Item -Path "Env:NOVOVM_OVERLAY_ROUTE_REGION" -ErrorAction SilentlyContinue
    if ($null -ne $overlayRegionEnv -and -not [string]::IsNullOrWhiteSpace($overlayRegionEnv.Value)) {
        $effectiveOverlayRegion = $overlayRegionEnv.Value.Trim()
    }
    $relayCandidates = Resolve-TemplateRelayCandidates -TemplateObject $ot -Region $effectiveOverlayRegion -RoleProfile $RoleProfile
    if ($relayCandidates.Count -gt 0) {
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES" -Value ($relayCandidates -join ",")
    }
    if ($null -ne $ot.hop_slot_seconds) {
        $hopSlotSec = [Math]::Min(86400, [Math]::Max(1, [int]$ot.hop_slot_seconds))
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_HOP_SLOT_SECONDS" -Value ([string]$hopSlotSec)
    }
    if ($null -ne $ot.hop_count) {
        $hopCount = [Math]::Max(1, [int]$ot.hop_count)
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_HOP_COUNT" -Value ([string]$hopCount)
    }
    if ($null -ne $ot.min_hops) {
        $minHops = [Math]::Max(1, [int]$ot.min_hops)
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_MIN_HOPS" -Value ([string]$minHops)
    }
    if ($null -ne $ot.strategy -and -not [string]::IsNullOrWhiteSpace([string]$ot.strategy)) {
        $strategyRaw = ([string]$ot.strategy).Trim().ToLowerInvariant()
        if ($strategyRaw -eq "direct" -or $strategyRaw -eq "multi_hop") {
            Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_STRATEGY" -Value $strategyRaw
        }
    }
    if ($null -ne $ot.enforce_multi_hop) {
        $enforceValue = [bool]$ot.enforce_multi_hop
        if ($enforceValue) {
            Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP" -Value "1"
        } else {
            Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_ENFORCE_MULTI_HOP" -Value "0"
        }
    }
    Write-Host ("novovm_up_overlay_route_template: file={0} profile={1}" -f $overlayRouteTemplate.path, $overlayRouteTemplate.profile)
}

$effectiveOverlayRouteMode = Resolve-OverlayRouteMode -ProfileValue $Profile -CliMode $OverlayRouteMode
Apply-OverlayRouteModeEnv -Mode $effectiveOverlayRouteMode

if ($PSBoundParameters.ContainsKey("OverlayRouteRelayDirectoryFile") -or -not [string]::IsNullOrWhiteSpace($OverlayRouteRelayDirectoryFile)) {
    $effectiveOverlayRegion = "global"
    $overlayRegionEnv = Get-Item -Path "Env:NOVOVM_OVERLAY_ROUTE_REGION" -ErrorAction SilentlyContinue
    if ($null -ne $overlayRegionEnv -and -not [string]::IsNullOrWhiteSpace($overlayRegionEnv.Value)) {
        $effectiveOverlayRegion = $overlayRegionEnv.Value.Trim()
    }
    $relaySetSizeRaw = 1
    $relaySetSizeEnv = Get-Item -Path "Env:NOVOVM_OVERLAY_ROUTE_RELAY_SET_SIZE" -ErrorAction SilentlyContinue
    if ($null -ne $relaySetSizeEnv -and -not [string]::IsNullOrWhiteSpace($relaySetSizeEnv.Value)) {
        $relaySetSizeParsed = 0
        if ([int]::TryParse($relaySetSizeEnv.Value, [ref]$relaySetSizeParsed)) {
            $relaySetSizeRaw = [Math]::Max(1, [Math]::Min(64, $relaySetSizeParsed))
        }
    }
    $penaltyStatePath = $OverlayRouteRelayPenaltyStateFile
    if (-not [System.IO.Path]::IsPathRooted($penaltyStatePath)) {
        $penaltyStatePath = Join-Path $repoRoot $penaltyStatePath
    }
    $penaltyMap = Load-RelayPenaltyState -StatePath $penaltyStatePath
    $recoverStep = [Math]::Min(1, [Math]::Max(0, [double]$OverlayRouteRelayPenaltyRecoverPerRun))
    if ($recoverStep -gt 0) {
        foreach ($key in @($penaltyMap.Keys)) {
            $rawValue = 0.0
            if ([double]::TryParse([string]$penaltyMap[$key], [ref]$rawValue)) {
                $next = [Math]::Max(0, ($rawValue - $recoverStep))
                $penaltyMap[$key] = [Math]::Min(1, [Math]::Max(0, $next))
            }
        }
    }
    if ($PSBoundParameters.ContainsKey("OverlayRouteRelayPenaltyDelta") -and -not [string]::IsNullOrWhiteSpace($OverlayRouteRelayPenaltyDelta)) {
        $deltaObj = Convert-JsonTextToObjectOrNull -Raw $OverlayRouteRelayPenaltyDelta
        if ($null -ne $deltaObj) {
            if ($deltaObj -is [System.Collections.IDictionary]) {
                foreach ($key in $deltaObj.Keys) {
                    $delta = 0.0
                    if ([double]::TryParse([string]$deltaObj[$key], [ref]$delta)) {
                        $base = 0.0
                        if ($penaltyMap.Contains([string]$key)) {
                            [double]::TryParse([string]$penaltyMap[[string]$key], [ref]$base) | Out-Null
                        }
                        $penaltyMap[[string]$key] = [Math]::Min(1, [Math]::Max(0, ($base + $delta)))
                    }
                }
            } else {
                foreach ($prop in $deltaObj.PSObject.Properties) {
                    $delta = 0.0
                    if ([double]::TryParse([string]$prop.Value, [ref]$delta)) {
                        $base = 0.0
                        if ($penaltyMap.Contains([string]$prop.Name)) {
                            [double]::TryParse([string]$penaltyMap[[string]$prop.Name], [ref]$base) | Out-Null
                        }
                        $penaltyMap[[string]$prop.Name] = [Math]::Min(1, [Math]::Max(0, ($base + $delta)))
                    }
                }
            }
        }
    }
    Save-RelayPenaltyState -StatePath $penaltyStatePath -PenaltyMap $penaltyMap
    $relayCandidatesFromDir = Resolve-RelayCandidatesFromDirectory `
        -RepoRootPath $repoRoot `
        -DirectoryFile $OverlayRouteRelayDirectoryFile `
        -Region $effectiveOverlayRegion `
        -RoleProfileValue $RoleProfile `
        -HealthMin $OverlayRouteRelayHealthMin `
        -MaxCount $relaySetSizeRaw `
        -PenaltyMap $penaltyMap
    if ($relayCandidatesFromDir.Count -gt 0) {
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES" -Value ($relayCandidatesFromDir -join ",")
    }
}

if ($PSBoundParameters.ContainsKey("OverlayRouteRelayCandidatesByRole") -and -not [string]::IsNullOrWhiteSpace($OverlayRouteRelayCandidatesByRole)) {
    $roleMapRaw = Convert-JsonTextToObjectOrNull -Raw $OverlayRouteRelayCandidatesByRole
    $roleCandidates = @()
    if ($null -ne $roleMapRaw) {
        $roleCandidatesRaw = Get-ObjectPropertyValueCI -Object $roleMapRaw -Name $RoleProfile
        if ($null -eq $roleCandidatesRaw) {
            $roleCandidatesRaw = Get-ObjectPropertyValueCI -Object $roleMapRaw -Name "default"
        }
        $roleCandidates = Convert-ToRelayCandidateList -Value $roleCandidatesRaw
    }
    if ($roleCandidates.Count -gt 0) {
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES" -Value ($roleCandidates -join ",")
    }
}

if ($PSBoundParameters.ContainsKey("OverlayRouteRelayCandidatesByRegion") -and -not [string]::IsNullOrWhiteSpace($OverlayRouteRelayCandidatesByRegion)) {
    $regionMapRaw = Convert-JsonTextToObjectOrNull -Raw $OverlayRouteRelayCandidatesByRegion
    $regionCandidates = @()
    if ($null -ne $regionMapRaw) {
        $effectiveOverlayRegion = "global"
        $overlayRegionEnv = Get-Item -Path "Env:NOVOVM_OVERLAY_ROUTE_REGION" -ErrorAction SilentlyContinue
        if ($null -ne $overlayRegionEnv -and -not [string]::IsNullOrWhiteSpace($overlayRegionEnv.Value)) {
            $effectiveOverlayRegion = $overlayRegionEnv.Value.Trim()
        }
        $regionCandidatesRaw = Get-ObjectPropertyValueCI -Object $regionMapRaw -Name $effectiveOverlayRegion
        if ($null -eq $regionCandidatesRaw) {
            $regionCandidatesRaw = Get-ObjectPropertyValueCI -Object $regionMapRaw -Name "default"
        }
        $regionCandidates = Convert-ToRelayCandidateList -Value $regionCandidatesRaw
    }
    if ($regionCandidates.Count -gt 0) {
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES" -Value ($regionCandidates -join ",")
    }
}

if ($PSBoundParameters.ContainsKey("OverlayRouteRelayCandidates") -and -not [string]::IsNullOrWhiteSpace($OverlayRouteRelayCandidates)) {
    $relayCandidatesCli = Convert-ToRelayCandidateList -Value $OverlayRouteRelayCandidates
    if ($relayCandidatesCli.Count -gt 0) {
        Set-EnvForce -Name "NOVOVM_OVERLAY_ROUTE_RELAY_CANDIDATES" -Value ($relayCandidatesCli -join ",")
    }
}

$pipelineScript = Join-Path $repoRoot "scripts/migration/run_gateway_node_pipeline.ps1"
if (-not (Test-Path -LiteralPath $pipelineScript)) {
    throw ("Missing pipeline script: " + $pipelineScript)
}

$templateProfile = $ReconcileRuntimeProfile
if ([string]::IsNullOrWhiteSpace($templateProfile)) {
    $templateProfile = $Profile
}
$reconcileTemplate = Resolve-ReconcileRuntimeTemplate -RepoRootPath $repoRoot -TemplateFilePath $ReconcileRuntimeFile -TemplateProfile $templateProfile
$templateEnableDefined = $false
$templateEnableValue = $false
if ($null -ne $reconcileTemplate) {
    $t = $reconcileTemplate.value
    if (-not $PSBoundParameters.ContainsKey("ReconcileSenderAddress") -and $null -ne $t.sender_address) {
        $ReconcileSenderAddress = [string]$t.sender_address
    }
    if (-not $PSBoundParameters.ContainsKey("ReconcileRpcEndpoint") -and $null -ne $t.rpc_endpoint) {
        $ReconcileRpcEndpoint = [string]$t.rpc_endpoint
    }
    if (-not $PSBoundParameters.ContainsKey("ReconcileIntervalSeconds") -and $null -ne $t.interval_seconds) {
        $ReconcileIntervalSeconds = [Math]::Max(1, [int]$t.interval_seconds)
    }
    if (-not $PSBoundParameters.ContainsKey("ReconcileRestartDelaySeconds") -and $null -ne $t.restart_delay_seconds) {
        $ReconcileRestartDelaySeconds = [Math]::Max(1, [int]$t.restart_delay_seconds)
    }
    if (-not $PSBoundParameters.ContainsKey("ReconcileReplayMaxPerPayout") -and $null -ne $t.replay_max_per_payout) {
        $ReconcileReplayMaxPerPayout = [Math]::Max(0, [int]$t.replay_max_per_payout)
    }
    if (-not $PSBoundParameters.ContainsKey("ReconcileReplayCooldownSec") -and $null -ne $t.replay_cooldown_sec) {
        $ReconcileReplayCooldownSec = [Math]::Max(0, [int]$t.replay_cooldown_sec)
    }
    if ($null -ne $t.enabled -and -not $PSBoundParameters.ContainsKey("EnableReconcileDaemon")) {
        $templateEnableDefined = $true
        $templateEnableValue = [bool]$t.enabled
    }
    $templateEnabledText = "n/a"
    if ($templateEnableDefined) {
        $templateEnabledText = [string]([bool]$templateEnableValue)
    }
    Write-Host ("novovm_up_reconcile_template: file={0} profile={1} enabled={2}" -f $reconcileTemplate.path, $reconcileTemplate.profile, $templateEnabledText)
}

$effectiveSkipBuild = $SkipBuild -or (($Profile -eq "prod") -and (-not $BuildBeforeRun))
$effectiveNoGateway = $NoGateway
$effectiveUseNodeWatchMode = $UseNodeWatchMode -or (($Profile -eq "prod") -and $Daemon)
$effectiveLeanIo = $LeanIo -or (($Profile -eq "prod") -and $Daemon)
$defaultAutoReconcileEnable = (($Profile -eq "prod") -and $Daemon -and (-not [string]::IsNullOrWhiteSpace($ReconcileSenderAddress)))
$effectiveEnableReconcileDaemon = $EnableReconcileDaemon -or $defaultAutoReconcileEnable
if ($templateEnableDefined -and -not $PSBoundParameters.ContainsKey("EnableReconcileDaemon")) {
    $effectiveEnableReconcileDaemon = [bool]$templateEnableValue
}

switch ($RoleProfile) {
    "l1" {
        $effectiveNoGateway = $true
        $effectiveUseNodeWatchMode = $true
        $effectiveEnableReconcileDaemon = $false
        if ($Profile -eq "prod") {
            $effectiveLeanIo = $true
        }
    }
    "l2" {
        $effectiveNoGateway = $true
        $effectiveUseNodeWatchMode = $true
        $effectiveEnableReconcileDaemon = $false
        if ($Profile -eq "prod") {
            $effectiveLeanIo = $true
        }
    }
    "l3" {
        $effectiveUseNodeWatchMode = $true
        if ($Profile -eq "prod") {
            $effectiveLeanIo = $true
        }
    }
    default {
    }
}

Set-EnvForce -Name "NOVOVM_NODE_ROLE_PROFILE" -Value $RoleProfile

switch ($RoleProfile) {
    "l1" { Set-EnvForce -Name "NOVOVM_NETWORK_LAYER_HINT" -Value "L1" }
    "l2" { Set-EnvForce -Name "NOVOVM_NETWORK_LAYER_HINT" -Value "L2" }
    "l3" { Set-EnvForce -Name "NOVOVM_NETWORK_LAYER_HINT" -Value "L3" }
    default { Set-EnvForce -Name "NOVOVM_NETWORK_LAYER_HINT" -Value "L1-L4" }
}

if ($effectiveEnableReconcileDaemon -and $effectiveNoGateway) {
    throw "EnableReconcileDaemon requires local gateway process in unified entrypoint mode (do not use -NoGateway)"
}
if ($effectiveEnableReconcileDaemon -and [string]::IsNullOrWhiteSpace($ReconcileSenderAddress)) {
    throw "EnableReconcileDaemon requires ReconcileSenderAddress (or reconcile template sender_address)"
}
if ($Profile -eq "prod" -and -not $effectiveNoGateway -and -not $AllowPublicGatewayBind) {
    if (-not (Test-IsLoopbackBind -BindAddress $GatewayBind)) {
        throw ("prod mode rejects public gateway bind by default: GatewayBind=" + $GatewayBind + " (use -AllowPublicGatewayBind to override explicitly)")
    }
}

Write-Host ("novovm_up_profile: profile={0} role={1} overlay_mode={2} no_gateway={3} daemon={4} use_node_watch_mode={5} lean_io={6} auto_profile={7}" -f $Profile, $RoleProfile, $effectiveOverlayRouteMode, [bool]$effectiveNoGateway, [bool]$Daemon, [bool]$effectiveUseNodeWatchMode, [bool]$effectiveLeanIo, [bool]$EnableAutoProfile)
Write-Host ("novovm_up_reconcile_embedded: enabled={0} sender={1} endpoint={2} interval_sec={3} replay_max={4} replay_cooldown_sec={5}" -f [bool]$effectiveEnableReconcileDaemon, $ReconcileSenderAddress, $ReconcileRpcEndpoint, $ReconcileIntervalSeconds, $ReconcileReplayMaxPerPayout, $ReconcileReplayCooldownSec)

if (-not $Daemon) {
    Invoke-PipelineOnce -PipelineScriptPath $pipelineScript -NoGatewayMode:$effectiveNoGateway -SkipBuildMode:$effectiveSkipBuild -UseNodeWatchModeFlag:$effectiveUseNodeWatchMode -LeanIoFlag:$effectiveLeanIo -EnableReconcileDaemonFlag:$effectiveEnableReconcileDaemon -ReconcileSenderAddressValue $ReconcileSenderAddress -ReconcileRpcEndpointValue $ReconcileRpcEndpoint -ReconcileIntervalSecondsValue $ReconcileIntervalSeconds -ReconcileRestartDelaySecondsValue $ReconcileRestartDelaySeconds -ReconcileReplayMaxPerPayoutValue $ReconcileReplayMaxPerPayout -ReconcileReplayCooldownSecValue $ReconcileReplayCooldownSec -BindValue $GatewayBind -SpoolDirValue $SpoolDir -PollMsValue $PollMs -SupervisorPollMsValue $SupervisorPollMs -NodeWatchBatchMaxFilesValue $NodeWatchBatchMaxFiles -IdleExitSecondsValue $IdleExitSeconds -GatewayMaxRequestsValue $GatewayMaxRequests -GatewayBinaryPathValue $GatewayBinaryPath -NodeBinaryPathValue $NodeBinaryPath
    exit 0
}

$restartCount = 0
while ($true) {
    Write-Host ("novovm_up_daemon_cycle_in: profile={0} role={1} no_gateway={2} skip_build={3} use_node_watch_mode={4} lean_io={5} restart_count={6}" -f $Profile, $RoleProfile, [bool]$effectiveNoGateway, [bool]$effectiveSkipBuild, [bool]$effectiveUseNodeWatchMode, [bool]$effectiveLeanIo, $restartCount)
    $ok = $true
    try {
        Invoke-PipelineOnce -PipelineScriptPath $pipelineScript -NoGatewayMode:$effectiveNoGateway -SkipBuildMode:$effectiveSkipBuild -UseNodeWatchModeFlag:$effectiveUseNodeWatchMode -LeanIoFlag:$effectiveLeanIo -EnableReconcileDaemonFlag:$effectiveEnableReconcileDaemon -ReconcileSenderAddressValue $ReconcileSenderAddress -ReconcileRpcEndpointValue $ReconcileRpcEndpoint -ReconcileIntervalSecondsValue $ReconcileIntervalSeconds -ReconcileRestartDelaySecondsValue $ReconcileRestartDelaySeconds -ReconcileReplayMaxPerPayoutValue $ReconcileReplayMaxPerPayout -ReconcileReplayCooldownSecValue $ReconcileReplayCooldownSec -BindValue $GatewayBind -SpoolDirValue $SpoolDir -PollMsValue $PollMs -SupervisorPollMsValue $SupervisorPollMs -NodeWatchBatchMaxFilesValue $NodeWatchBatchMaxFiles -IdleExitSecondsValue $IdleExitSeconds -GatewayMaxRequestsValue $GatewayMaxRequests -GatewayBinaryPathValue $GatewayBinaryPath -NodeBinaryPathValue $NodeBinaryPath
    } catch {
        $ok = $false
        Write-Host ("novovm_up_daemon_cycle_err: {0}" -f $_.Exception.Message)
    }

    if ($ok) {
        Write-Host "novovm_up_daemon_cycle_out: pipeline exited without exception"
    }

    $restartCount = $restartCount + 1
    if ($MaxRestarts -gt 0 -and $restartCount -ge $MaxRestarts) {
        Write-Host ("novovm_up_daemon_out: max_restarts_reached={0}" -f $MaxRestarts)
        break
    }
    Start-Sleep -Seconds $RestartDelaySeconds
}


