Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$scriptPath = Join-Path $repo 'scripts/novovm-adaptive-overlay-cross-machine-smoke.ps1'
$root = Join-Path $repo ('artifacts/audit/aggregate-test-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $root | Out-Null
$cases = @('direct', 'relay', 'multihop', 'queue', 'receiver-failed', 'sender-string-bool',
    'wrong-receiver', 'wrong-sender', 'wrong-scope', 'missing-report', 'missing-target',
    'duplicate-target', 'missing-relay', 'short-send', 'unexpected-queue', 'missing-sender', 'missing-queue-sender',
    'stale-sender', 'stale-receiver', 'wrong-case', 'legacy-report')
foreach ($case in $cases) {
    $config = Get-Content (Join-Path $repo 'configs/network-overlay/adaptive-cross-machine-4node.example.json') -Raw | ConvertFrom-Json
    $index = switch ($case) { 'relay' { 1 }; 'multihop' { 2 }; 'queue' { 3 }; 'missing-queue-sender' { 3 }; 'missing-relay' { 1 }; default { 0 } }
    $selection = $config.cases[$index]
    $out = Join-Path $root $case
    $reportDir = Join-Path $out $selection.name
    New-Item -ItemType Directory -Path $reportDir -Force | Out-Null
    $sender = [ordered]@{
        accepted = $true; scope = 'adaptive_overlay_node_process_gate_v0'; node_id = 'node-a'
        target_peer_id = 'node-b'; selected_path = $selection.expected_path
        sent_frame_count = 4; queued_count = 0; sent_bytes_total = 100
        diagnostic_run_id = 'test-attempt'; diagnostic_case = $selection.name
    }
    if ($case -eq 'queue') { $sender.sent_frame_count = 0; $sender.queued_count = 4; $sender.sent_bytes_total = 0 }
    if ($case -eq 'sender-string-bool') { $sender.accepted = 'false' }
    if ($case -eq 'wrong-sender') { $sender.node_id = 'someone-else' }
    if ($case -eq 'wrong-scope') { $sender.scope = 'unrelated' }
    if ($case -eq 'short-send') { $sender.sent_frame_count = 3 }
    if ($case -eq 'unexpected-queue') { $sender.queued_count = 1 }
    if ($case -eq 'stale-sender') { $sender.diagnostic_run_id = 'previous-attempt' }
    if ($case -eq 'legacy-report') { $sender.Remove('diagnostic_run_id') }
    if ($case -eq 'missing-target') { $selection.listener_node_ids = @() }
    if ($case -eq 'duplicate-target') { $selection.listener_node_ids = @('node-b', 'node-b') }
    if ($case -eq 'missing-relay') { $selection.listener_node_ids = @('node-b') }
    if ($case -notin @('missing-sender', 'missing-queue-sender')) {
        $sender | ConvertTo-Json | Set-Content (Join-Path $reportDir 'node-a.json')
    }
    foreach ($id in $selection.listener_node_ids) {
        if ($case -eq 'missing-report') { continue }
        $listener = [ordered]@{
            accepted = $case -ne 'receiver-failed'; scope = 'adaptive_overlay_node_process_gate_v0'
            node_id = if ($case -eq 'wrong-receiver') { 'someone-else' } else { $id }
            direct_frames_received = 4; relay_frames_forwarded = 4
            diagnostic_run_id = if ($case -eq 'stale-receiver') { 'previous-attempt' } else { 'test-attempt' }
            diagnostic_case = if ($case -eq 'wrong-case') { 'unrelated-case' } else { $selection.name }
        }
        $listener | ConvertTo-Json | Set-Content (Join-Path $reportDir "$id.json")
    }
    $configPath = Join-Path $out 'config.json'
    $config | ConvertTo-Json -Depth 10 | Set-Content $configPath
    & pwsh -NoProfile -File $scriptPath -Action aggregate -RunId test-attempt -RepoRoot $repo -ConfigPath $configPath -Case $selection.name -ReportRoot $out *> (Join-Path $out 'process.log')
    $code = $LASTEXITCODE
    $expected = $case -in @('direct', 'relay', 'multihop', 'queue')
    if (($code -eq 0) -ne $expected) { throw "unexpected exit $code for $case; inspect $out" }
    $report = Get-Content (Join-Path $reportDir 'aggregate.json') -Raw | ConvertFrom-Json
    if ($report.accepted -ne $expected) { throw "incorrect report for $case" }
    if (!$report.boundary.network_only -or $report.boundary.aoem_called) { throw 'boundary changed' }
}
Write-Output "PASS: $($cases.Count) aggregate cases; synthetic reports only, no network. Evidence: $root"
# The last case intentionally launches a child process that exits nonzero.
# GitHub's pwsh wrapper propagates LASTEXITCODE unless this completed suite
# explicitly exits successfully. Assertion failures above still terminate first.
exit 0
