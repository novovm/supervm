param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 3600)]
    [int]$TimeoutSecondsPerCase = 300
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\unifiedaccount"
}

function Invoke-CargoCommand {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs,
        [int]$TimeoutSeconds = 0
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $startedAt = [DateTime]::UtcNow
    $proc = [System.Diagnostics.Process]::Start($psi)
    $timedOut = $false
    if ($TimeoutSeconds -gt 0) {
        if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
            $timedOut = $true
            try { $proc.Kill() } catch {}
        }
    } else {
        $proc.WaitForExit()
    }
    $finishedAt = [DateTime]::UtcNow
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $durationMs = [int]($finishedAt - $startedAt).TotalMilliseconds

    return [ordered]@{
        args = $CargoArgs
        exit_code = [int]$proc.ExitCode
        timed_out = $timedOut
        started_at_utc = $startedAt.ToString("o")
        finished_at_utc = $finishedAt.ToString("o")
        duration_ms = $durationMs
        stdout = $stdout
        stderr = $stderr
    }
}

$nodeCrateDir = Join-Path $RepoRoot "crates\novovm-node"
if (-not (Test-Path (Join-Path $nodeCrateDir "Cargo.toml"))) {
    throw "missing novovm-node Cargo.toml: $nodeCrateDir"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$warmup = Invoke-CargoCommand -WorkDir $nodeCrateDir -CargoArgs @("test", "unified_account_gate_ua_g", "--no-run")
if ($warmup.exit_code -ne 0 -or $warmup.timed_out) {
    $warmupOut = ($warmup.stdout + $warmup.stderr).Trim()
    throw "cargo warmup failed before UA gate execution`n$warmupOut"
}

$cases = @(
    [ordered]@{
        case_id = "UA-G01"; gate = "ua_mapping_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g01_mapping_bind_success";
        evidence_rel_path = "ua_mapping/UA-G01.json";
        expected_output = "binding succeeds and emits binding_added"
    },
    [ordered]@{
        case_id = "UA-G02"; gate = "ua_mapping_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g02_mapping_conflict_rejected";
        evidence_rel_path = "ua_mapping/UA-G02.json";
        expected_output = "conflict binding rejected with binding_conflict_rejected"
    },
    [ordered]@{
        case_id = "UA-G03"; gate = "ua_mapping_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g03_mapping_cooldown_rejects_rebind";
        evidence_rel_path = "ua_mapping/UA-G03.json";
        expected_output = "rebind rejected during cooldown window"
    },
    [ordered]@{
        case_id = "UA-G04"; gate = "ua_signature_domain_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g04_signature_domain_mismatch_rejected";
        evidence_rel_path = "ua_signature/UA-G04.json";
        expected_output = "cross-domain signature rejected with domain_mismatch_rejected"
    },
    [ordered]@{
        case_id = "UA-G05"; gate = "ua_signature_domain_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g05_signature_domain_eip712_wrong_chain_rejected";
        evidence_rel_path = "ua_signature/UA-G05.json";
        expected_output = "typed_data fails under wrong chain_id domain"
    },
    [ordered]@{
        case_id = "UA-G06"; gate = "ua_nonce_replay_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g06_nonce_replay_rejected";
        evidence_rel_path = "ua_nonce/UA-G06.json";
        expected_output = "replayed nonce is rejected on second submission"
    },
    [ordered]@{
        case_id = "UA-G07"; gate = "ua_nonce_replay_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g07_nonce_reverse_order_rejected";
        evidence_rel_path = "ua_nonce/UA-G07.json";
        expected_output = "non-monotonic nonce is rejected"
    },
    [ordered]@{
        case_id = "UA-G08"; gate = "ua_permission_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g08_permission_delegate_cannot_update_policy";
        evidence_rel_path = "ua_permission/UA-G08.json";
        expected_output = "delegate policy update attempt is rejected"
    },
    [ordered]@{
        case_id = "UA-G09"; gate = "ua_permission_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g09_permission_expired_session_key_rejected";
        evidence_rel_path = "ua_permission/UA-G09.json";
        expected_output = "expired session key transaction is rejected"
    },
    [ordered]@{
        case_id = "UA-G10"; gate = "ua_persona_boundary_signal"; failure_level = "BlockRelease";
        test_name = "unified_account_gate_ua_g10_boundary_eth_cross_chain_atomic_rejected";
        evidence_rel_path = "ua_boundary/UA-G10.json";
        expected_output = "eth_* cross-chain atomic request is rejected"
    },
    [ordered]@{
        case_id = "UA-G11"; gate = "ua_persona_boundary_signal"; failure_level = "BlockRelease";
        test_name = "unified_account_gate_ua_g11_boundary_web30_single_chain_passes_without_eth_pollution";
        evidence_rel_path = "ua_boundary/UA-G11.json";
        expected_output = "web30_* single-chain route passes without eth_* nonce pollution"
    },
    [ordered]@{
        case_id = "UA-G12"; gate = "ua_type4_policy_signal"; failure_level = "BlockRelease";
        test_name = "unified_account_gate_ua_g12_type4_supported_mode_passes";
        evidence_rel_path = "ua_type4/UA-G12.json";
        expected_output = "type4 request passes in supported policy mode"
    },
    [ordered]@{
        case_id = "UA-G13"; gate = "ua_type4_policy_signal"; failure_level = "BlockRelease";
        test_name = "unified_account_gate_ua_g13_type4_reject_mode_returns_fixed_error";
        evidence_rel_path = "ua_type4/UA-G13.json";
        expected_output = "type4 reject mode returns fixed rejection"
    },
    [ordered]@{
        case_id = "UA-G14"; gate = "ua_type4_policy_signal"; failure_level = "BlockRelease";
        test_name = "unified_account_gate_ua_g14_type4_with_session_key_rejected_by_policy";
        evidence_rel_path = "ua_type4/UA-G14.json";
        expected_output = "type4 with session key is rejected by disabled mix policy"
    },
    [ordered]@{
        case_id = "UA-G15"; gate = "ua_uniqueness_conflict_signal"; failure_level = "BlockMerge";
        test_name = "unified_account_gate_ua_g15_uniqueness_conflict_signal_blocks_second_owner";
        evidence_rel_path = "ua_uniqueness/UA-G15.json";
        expected_output = "same persona conflict is blocked"
    },
    [ordered]@{
        case_id = "UA-G16"; gate = "ua_recovery_revocation_signal"; failure_level = "Warn";
        test_name = "unified_account_gate_ua_g16_recovery_rotate_then_revoke_emits_events";
        evidence_rel_path = "ua_recovery/UA-G16.json";
        expected_output = "key rotation plus revocation emits auditable state events"
    }
)

$results = @()
foreach ($case in $cases) {
    $evidencePath = Join-Path $OutputDir ($case.evidence_rel_path -replace "/", "\")
    $caseDir = Split-Path $evidencePath -Parent
    New-Item -ItemType Directory -Force -Path $caseDir | Out-Null

    $stdoutPath = Join-Path $caseDir "$($case.case_id).stdout.log"
    $stderrPath = Join-Path $caseDir "$($case.case_id).stderr.log"
    $run = Invoke-CargoCommand -WorkDir $nodeCrateDir -CargoArgs @(
        "test",
        $case.test_name,
        "--",
        "--nocapture",
        "--exact"
    ) -TimeoutSeconds $TimeoutSecondsPerCase

    $run.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
    $run.stderr | Set-Content -Path $stderrPath -Encoding UTF8

    $pass = [bool]((-not $run.timed_out) -and $run.exit_code -eq 0)
    $errorReason = ""
    if ($run.timed_out) {
        $errorReason = "timeout"
    } elseif ($run.exit_code -ne 0) {
        $errorReason = "cargo_test_failed"
    }

    $caseResult = [ordered]@{
        generated_at_utc = [DateTime]::UtcNow.ToString("o")
        case_id = $case.case_id
        gate = $case.gate
        failure_level = $case.failure_level
        pass = $pass
        error_reason = $errorReason
        expected_output = $case.expected_output
        test_name = $case.test_name
        test_command = "cargo test $($case.test_name) -- --nocapture --exact"
        exit_code = [int]$run.exit_code
        timed_out = [bool]$run.timed_out
        started_at_utc = $run.started_at_utc
        finished_at_utc = $run.finished_at_utc
        duration_ms = [int]$run.duration_ms
        stdout = $stdoutPath
        stderr = $stderrPath
    }

    $caseResult | ConvertTo-Json -Depth 8 | Set-Content -Path $evidencePath -Encoding UTF8
    $results += $caseResult
}

$allGates = @($results | ForEach-Object { $_.gate } | Sort-Object -Unique)
$signalRows = @()
foreach ($gate in $allGates) {
    $gateCases = @($results | Where-Object { $_.gate -eq $gate })
    $gatePass = [bool](@($gateCases | Where-Object { -not $_.pass }).Count -eq 0)
    $signalRows += [ordered]@{
        gate = $gate
        pass = $gatePass
        case_count = $gateCases.Count
        case_ids = @($gateCases | ForEach-Object { $_.case_id })
    }
}

$blockMergeCases = @($results | Where-Object { $_.failure_level -eq "BlockMerge" })
$blockReleaseCases = @($results | Where-Object { $_.failure_level -eq "BlockRelease" })
$warnCases = @($results | Where-Object { $_.failure_level -eq "Warn" })
$blockMergePass = [bool](@($blockMergeCases | Where-Object { -not $_.pass }).Count -eq 0)
$blockReleasePass = [bool](@($blockReleaseCases | Where-Object { -not $_.pass }).Count -eq 0)
$warnPass = [bool](@($warnCases | Where-Object { -not $_.pass }).Count -eq 0)
$overallPass = [bool]($blockMergePass -and $blockReleasePass)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $overallPass
    block_merge_pass = $blockMergePass
    block_release_pass = $blockReleasePass
    warn_pass = $warnPass
    total_cases = $results.Count
    passed_cases = @($results | Where-Object { $_.pass }).Count
    failed_cases = @($results | Where-Object { -not $_.pass }).Count
    output_dir = $OutputDir
    ua_signals = $signalRows
    cases = $results
}

$summaryJson = Join-Path $OutputDir "unified-account-gate-summary.json"
$summaryMd = Join-Path $OutputDir "unified-account-gate-summary.md"
$summary | ConvertTo-Json -Depth 12 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Unified Account Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- block_merge_pass: $($summary.block_merge_pass)"
    "- block_release_pass: $($summary.block_release_pass)"
    "- warn_pass: $($summary.warn_pass)"
    "- total_cases: $($summary.total_cases)"
    "- passed_cases: $($summary.passed_cases)"
    "- failed_cases: $($summary.failed_cases)"
    "- summary_json: $summaryJson"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "unified account gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  block_merge_pass: $($summary.block_merge_pass)"
Write-Host "  block_release_pass: $($summary.block_release_pass)"
Write-Host "  warn_pass: $($summary.warn_pass)"
Write-Host "  passed_cases: $($summary.passed_cases)/$($summary.total_cases)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"

if (-not $summary.pass) {
    throw "unified account gate FAILED"
}

Write-Host "unified account gate PASS"
