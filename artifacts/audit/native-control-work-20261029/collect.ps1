param([Parameter(Mandatory)][string]$Report,[Parameter(Mandatory)][string]$Name)
$ErrorActionPreference = 'Stop'
$path = (Resolve-Path -LiteralPath $Report).Path
$raw = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
$rows = if ($raw.rounds) { @($raw.rounds) } else { @($raw) }
$rounds = foreach ($row in $rows) {
    $r = if ($row.report) { $row.report } else { $row }
    $economics = $r.authority | ConvertTo-Json -Depth 20 -Compress
    $economicsHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($economics))).ToLowerInvariant()
    [ordered]@{
        label=$row.label;signature_backend=$r.signature_backend;transactions=$r.transactions;
        successful_tps=$r.successful_tps;elapsed_seconds=$r.elapsed_seconds;latency_us=$r.latency_us;
        passed=$r.passed;cold_recovery_pass=$r.cold_recovery_pass;full_fee_state_audited=$r.full_fee_state_audited;
        input_hash=$r.signed_workload_sha256;signing_domain=$r.signing_domain;genesis_commitment=$r.genesis_commitment;initial_root=$r.initial_root;
        binary_sha256=$r.binary_sha256;aoem_sha256=$r.aoem_sha256;business_backend=$r.backend;
        counts=$r.counts;load_strategy=$r.load_strategy;signature_generation_timed=$r.signature_generation_timed;
        timing=$r.timing;canonical_body_budget=$r.canonical_body_budget;wire_message_budget=$r.wire_message_budget;
        grant_expires_at_height=$r.grant_expires_at_height;static_intent_window=$r.static_intent_window;
        authority_with_schedule_metadata_sha256=$economicsHash;cold_counts=$r.cold_counts;
        heads=@($r.final_local_progress.nodes | ForEach-Object { [ordered]@{node=$_.node_index;matched=$_.matched_final_head;head=$_.snapshot.head} })
    }
}
$zip = Join-Path $PSScriptRoot "$Name.raw.zip"
Compress-Archive -LiteralPath $path -DestinationPath $zip
[ordered]@{
    source=$path;source_sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant();
    archive=(Split-Path -Leaf $zip);archive_sha256=(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant();
    same_signed_corpus_and_initial_state=$raw.same_signed_corpus_and_initial_state;
    scope='public reports only; full raw report compressed unchanged; per-round database and signing journal retained locally; no private-key files';
    rounds=@($rounds)
} | ConvertTo-Json -Depth 25 | Set-Content -LiteralPath (Join-Path $PSScriptRoot "$Name.json") -Encoding utf8NoBOM
Get-Item -LiteralPath $zip | Select-Object Name,Length
