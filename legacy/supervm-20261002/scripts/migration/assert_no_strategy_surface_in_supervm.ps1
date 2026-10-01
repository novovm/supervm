param(
    [string]$RepoRoot = ""
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

function Assert-NoPattern {
    param(
        [string[]]$Paths,
        [string]$Pattern,
        [string]$Label
    )
    foreach ($path in $Paths) {
        if (-not (Test-Path $path)) {
            continue
        }
        $matches = @(Select-String -Path $path -Pattern $Pattern -CaseSensitive -List)
        if ($matches.Count -gt 0) {
            $first = $matches[0]
            throw "strategy surface boundary violation: ${Label} found in $($first.Path):$($first.LineNumber)"
        }
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
Push-Location $RepoRoot
try {
    $scanRoots = @("crates", "scripts", "artifacts/migration")
    $extensions = @("*.rs", "*.ps1", "*.md")
    $files = New-Object System.Collections.Generic.List[string]
    foreach ($root in $scanRoots) {
        if (-not (Test-Path $root)) {
            continue
        }
        foreach ($ext in $extensions) {
            Get-ChildItem -Path $root -Recurse -File -Filter $ext |
                Where-Object { $_.FullName -notmatch "\\artifacts\\audit\\" } |
                ForEach-Object { $files.Add($_.FullName) }
        }
    }

    $checks = @(
        @{ Label = "external strategy acronym"; Pattern = "M[E]V" },
        @{ Label = "specific router family name"; Pattern = "U[n]iswap|u[n]iswap" },
        @{ Label = "strategy priority env"; Pattern = "S[W]AP_PRIORITY|s[w]ap_priority" },
        @{ Label = "strategy hit counters"; Pattern = "s[w]ap_hits|s[w]ap_v2|s[w]ap_v3|u[n]ique_swap|f[i]rst_swap|l[a]st_swap|r[e]cent_swap|t[o]tal_swap" },
        @{ Label = "strategy classifier names"; Pattern = "S[w]apKind|d[e]tect_swap|s[w]ap_classifier" },
        @{ Label = "removed observation scripts"; Pattern = "r[u]n_evm_u[n]iswap|w[a]tch_evm_u[n]iswap|e[v]m-u[n]iswap" },
        @{ Label = "removed autopilot scripts"; Pattern = "r[u]n_evm_mempool_autopilot|r[u]n_evm_full_lifecycle_autopilot" }
    )

    foreach ($check in $checks) {
        Assert-NoPattern -Paths $files.ToArray() -Pattern $check.Pattern -Label $check.Label
    }

    Write-Host "SUPERVM/NOVOVM strategy-surface guard passed"
} finally {
    Pop-Location
}
