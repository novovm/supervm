param([Parameter(Mandatory)][string]$Report,[Parameter(Mandatory)][string]$Name,
    [string]$OutputDirectory = $PSScriptRoot)
$ErrorActionPreference = 'Stop'
$path = (Resolve-Path -LiteralPath $Report).Path
$raw = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
$rows = if ($null -ne $raw.completed) { @() } elseif ($raw.rounds) { @($raw.rounds) } else { @($raw) }
$rounds = foreach ($row in $rows) {
    $r = if ($row.report) { $row.report } else { $row }
    [ordered]@{
        label=$row.label;signature_backend=$r.signature_backend;transactions=$r.transactions;
        successful_tps=$r.successful_tps;elapsed_seconds=$r.elapsed_seconds;latency_us=$r.latency_us;
        passed=$r.passed;cold_recovery_pass=$r.cold_recovery_pass;full_fee_state_audited=$r.full_fee_state_audited;
        input_hash=$r.signed_workload_sha256;signing_domain=$r.signing_domain;genesis_commitment=$r.genesis_commitment;initial_root=$r.initial_root;
        binary_sha256=$r.binary_sha256;aoem_sha256=$r.aoem_sha256;business_backend=$r.backend;
        counts=$r.counts;load_strategy=$r.load_strategy;signature_generation_timed=$r.signature_generation_timed;
        timing=$r.timing;canonical_body_budget=$r.canonical_body_budget;wire_message_budget=$r.wire_message_budget;
        grant_expires_at_height=$r.grant_expires_at_height;static_intent_window=$r.static_intent_window;
        cold_counts=$r.cold_counts;recovered=@($r.recovered)
        diagnostic_counters=$(if ($r.final_local_progress) {
            [ordered]@{
                status=$r.final_local_progress.status;scope=$r.final_local_progress.scope;
                nodes=@(foreach ($n in $r.final_local_progress.nodes) {
                    [ordered]@{
                        node_index=$n.node_index;matched_final_head=$n.matched_final_head;
                        phase_scope=$n.snapshot.stage_profile.scope;
                        phases=$n.snapshot.stage_profile.phases;
                        send_polls=$n.snapshot.native_send_polls;
                        receive_totals=$n.snapshot.native_receive_totals
                    }
                })
            }
        })
    }
}
$evidenceRoot = (Resolve-Path -LiteralPath $OutputDirectory).Path
$zip = Join-Path $evidenceRoot "$Name.raw.zip"
if (Test-Path -LiteralPath $zip) {
    $previous = Get-Content -Raw -LiteralPath (Join-Path $evidenceRoot "$Name.json") | ConvertFrom-Json
    if ($previous.source_sha256 -ne (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -or
        $previous.archive_sha256 -ne (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()) {
        throw 'Existing evidence differs; use a new name.'
    }
} else {
    Compress-Archive -LiteralPath $path -DestinationPath $zip
}
[ordered]@{
    source=$path;source_sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant();
    archive=(Split-Path -Leaf $zip);archive_sha256=(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant();
    same_signed_corpus_and_initial_state=$raw.same_signed_corpus_and_initial_state;
    fault_recovery=$(if ($null -ne $raw.completed) {
        $raw | Select-Object completed,scope,real_processes,real_machines,fixed_quorum,
            binary_sha256,library_sha256,missing_validator,offline_heights,late_start_after_ms,
            catchup_crash,final_heads,final_states,durable_fee_oracle,
            same_config_and_signing_outbox_preserved,cold_reopen_verified,elapsed_ms
    });
    scope='public reports only; raw report compressed unchanged; per-round database and signing journal retained locally; no private-key files';
    rounds=@($rounds)
} | ConvertTo-Json -Depth 25 | Set-Content -LiteralPath (Join-Path $evidenceRoot "$Name.json") -Encoding utf8NoBOM
Get-Item -LiteralPath $zip | Select-Object Name,Length
