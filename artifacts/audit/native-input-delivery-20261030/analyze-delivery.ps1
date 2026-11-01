param(
    [Parameter(Mandatory)][string]$ReportPath,
    [Parameter(Mandatory)][string]$OutputPath,
    [ValidateRange(1, 40)][int]$Top = 16
)
$ErrorActionPreference = 'Stop'

function Hex($Value) {
    if ($null -eq $Value) { return $null }
    if ($Value -is [string]) { return ($Value -replace '^0x', '').ToLowerInvariant() }
    [Convert]::ToHexString([byte[]]$Value).ToLowerInvariant()
}
function Scope($Event) { "$(Hex $Event.detail.session)/$($Event.detail.sequence)" }
function Delta($Last, $First) {
    if ($null -ne $Last -and $null -ne $First) { ([double]$Last - [double]$First) / 1000 }
}
function Local-Us($Event) { if ($null -ne $Event) { [double]$Event.elapsed_ns / 1000 } }
function Evidence-Us($Event, $Value) {
    if ($null -ne $Event.detail.observer_elapsed_us -and $null -ne $Value) {
        (Local-Us $Event) - [double]$Event.detail.observer_elapsed_us + [double]$Value
    }
}
function Wall-Us($Event, $Value) {
    if ($null -ne $Event.wall_unix_us -and $null -ne $Event.detail.observer_elapsed_us -and $null -ne $Value) {
        [double]$Event.wall_unix_us - [double]$Event.detail.observer_elapsed_us + [double]$Value
    }
}
function Show($Event) {
    if ($null -eq $Event) { return $null }
    [ordered]@{ event = $Event.event; height = $Event.height; elapsed_ms = $Event.elapsed_ns / 1e6
        wall_unix_us_ALIGNMENT_ONLY = $Event.wall_unix_us; detail = $Event.detail }
}

$resolved = (Resolve-Path -LiteralPath $ReportPath).Path
$directory = Split-Path -Parent $resolved
$report = Get-Content -LiteralPath $resolved -Raw | ConvertFrom-Json
$nodes = @(); $configs = @(); $issues = [Collections.Generic.List[string]]::new()
for ($i = 0; $i -lt 4; $i++) {
    $embedded = @($report.final_local_progress.nodes | Where-Object node_index -eq $i)
    $snapshot = if ($embedded.Count -eq 1) { $embedded[0].snapshot } else { $null }
    $origin = 'report.final_local_progress'
    if ($null -eq $snapshot.stage_profile) {
        $path = Join-Path $directory "create-$i.progress.json"
        $origin = $path
        $snapshot = if (Test-Path -LiteralPath $path -PathType Leaf) {
            Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
        } else { $null }
    }
    $events = @($snapshot.stage_profile.asset_ingress.events | Sort-Object elapsed_ns)
    $nodes += [pscustomobject]@{ index = $i; snapshot = $snapshot; origin = $origin; events = $events
        local_peer = $null; routes = @{} }
    $path = Join-Path $directory "node-$i.json"
    # Only public endpoints/keys are selected below; never export the configs.
    $configs += $(if (Test-Path -LiteralPath $path -PathType Leaf) {
        Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    } else { $null })
    if (!$events.Count) { $issues.Add("node$i has no retained asset events") }
}
for ($i = 0; $i -lt 4; $i++) {
    if ($null -eq $configs[$i]) { continue }
    $localKeys = @($configs | Where-Object { $null -ne $_ } | ForEach-Object { $_.peers } |
        Where-Object address -eq $configs[$i].udp_addr | ForEach-Object { Hex $_.public_key } | Select-Object -Unique)
    if ($localKeys.Count -eq 1) {
        $member = @($configs[$i].genesis.validators | Where-Object { (Hex $_.public_key) -eq $localKeys[0] })
        if ($member.Count -eq 1 -and $member[0].transport_public_key.Count -eq 32) {
            $nodes[$i].local_peer = 'novovm-ed25519:' + (Hex $member[0].transport_public_key)
        }
    }
    for ($p = 0; $p -lt $configs[$i].peers.Count; $p++) {
        $matches = @(0..3 | Where-Object { $null -ne $configs[$_] -and $configs[$_].udp_addr -eq $configs[$i].peers[$p].address })
        if ($matches.Count -eq 1) { $nodes[$i].routes[$p] = $matches[0] }
    }
}

$cases = [Collections.Generic.List[object]]::new()
foreach ($node in $nodes) {
    foreach ($gossip in @($node.events | Where-Object event -eq 'gossip_batch_prepared')) {
        $scope = Scope $gossip
        $scoped = @($node.events | Where-Object { $null -ne $_.detail.session -and (Scope $_) -eq $scope })
        $prepared = @($scoped | Where-Object { $_.event -eq 'asset_prepared' -and $_.detail.kind -eq 'asset_transactions' } | Select-Object -First 1)[0]
        $reservation = @($scoped | Where-Object event -eq 'input_fanout_reserved' | Select-Object -First 1)[0]
        $wait = @($scoped | Where-Object event -eq 'input_fanout_wait' | Select-Object -First 1)[0]
        $offer = @($scoped | Where-Object event -eq 'gossip_offer_reserved' | Select-Object -First 1)[0]
        $fragment = Hex $prepared.detail.fragment
        $peerStates = if ($null -ne $reservation) { @($reservation.detail.peers) } else { @($wait.detail.peers) }
        if (!$peerStates.Count) { $peerStates = @([pscustomobject]@{ peer = $null; requested = $true; reserved = $null }) }
        foreach ($peerState in $peerStates | Where-Object requested -eq $true) {
            $p = $peerState.peer
            $target = if ($null -ne $p -and $node.routes.ContainsKey([int]$p)) { $node.routes[[int]$p] } else { $null }
            $outbound = @($node.events | Where-Object {
                $fragment -and (Hex $_.detail.fragment) -eq $fragment -and $_.detail.peer_index -eq $p
            })
            $admit = @($outbound | Where-Object event -eq 'input_transport_admitted' | Select-Object -First 1)[0]
            $returned = @($outbound | Where-Object event -eq 'input_transport_returned' | Select-Object -Last 1)[0]
            $sendEvent = if ($null -ne $returned) { $returned } else { $admit }
            $tag = Hex $sendEvent.detail.tag
            $receive = @(); $decoded = @()
            if ($null -ne $target -and $fragment) {
                $receive = @($nodes[$target].events | Where-Object {
                    $_.event -eq 'input_transport_complete' -and (Hex $_.detail.fragment) -eq $fragment -and
                    ($null -eq $node.local_peer -or $_.detail.peer -eq $node.local_peer) -and
                    (!$tag -or (Hex $_.detail.tag) -eq $tag)
                })
                $decoded = @($nodes[$target].events | Where-Object {
                    $_.event -eq 'asset_received' -and (Hex $_.detail.fragment) -eq $fragment -and (Scope $_) -eq $scope
                })
            }
            $receiveEvent = if ($receive.Count -eq 1) { $receive[0] } else { $null }
            $decodedEvent = if ($decoded.Count -eq 1) { $decoded[0] } else { $null }
            $s = $sendEvent.detail.evidence; $r = $receiveEvent.detail.evidence
            $attemptEvents = @($outbound | Where-Object { $_.event -in @('input_transport_admitted', 'input_transport_returned') -and
                (Hex $_.detail.tag) -eq $tag })
            $attemptCounts = @($attemptEvents | ForEach-Object { $_.detail.evidence.send_attempts_observed } | Where-Object { $null -ne $_ })
            $attempts = if ($attemptCounts.Count) { [uint64]($attemptCounts | Measure-Object -Maximum).Maximum } else { $null }
            $retryMixed = $null -ne $attempts -and $attempts -gt 1
            $attemptQualification = if ($retryMixed) { 'retry_mixed/unqualified' } elseif ($null -eq $attempts) {
                'unknown/attempt_count_not_observed'
            } else { 'single_observed_send_attempt/receiver_generation_not_observed' }
            $flags = [Collections.Generic.List[string]]::new()
            if ($retryMixed) { $flags.Add('RETRY_MIXED_UNQUALIFIED_NO_SINGLE_ATTEMPT_PHASE_JOIN') }
            elseif ($null -eq $attempts -and $null -ne $sendEvent) { $flags.Add('SEND_ATTEMPT_COUNT_NOT_OBSERVED') }
            if ($null -eq $reservation) { $flags.Add('RESERVATION_NOT_OBSERVED') }
            if ($peerState.reserved -eq $false) { $flags.Add('PEER_OMITTED_FROM_THIS_OBJECT') }
            if ($null -eq $target) { $flags.Add('PEER_TO_NODE_UNKNOWN') }
            if ($null -eq $admit) { $flags.Add('SEND_ADMISSION_NOT_OBSERVED') }
            if ($null -eq $returned) { $flags.Add('SEND_RETURN_NOT_OBSERVED') }
            if ($receive.Count -ne 1) { $flags.Add("RECEIVE_HANDOFF_COUNT_$($receive.Count)") }
            if ($decoded.Count -ne 1) { $flags.Add("DECODED_INPUT_COUNT_$($decoded.Count)") }
            if ($node.snapshot.stage_profile.asset_ingress.evicted -gt 0) { $flags.Add('SENDER_ASSET_EVENTS_EVICTED') }
            foreach ($entry in @($sendEvent, $receiveEvent) | Where-Object { $null -ne $_ }) {
                if ($null -eq $entry.detail.evidence) { $flags.Add('TRANSFER_RECORD_MISSING') }
                if ($entry.detail.retention.omitted_records -gt 0 -or $entry.detail.retention.evicted_records -gt 0 -or $entry.detail.retention.expired_records -gt 0) {
                    $flags.Add('OBSERVER_HAS_RETENTION_LOSS_CHECK_RAW_RECORD_COMPLETENESS')
                }
            }
            if ($null -ne $target -and $nodes[$target].snapshot.stage_profile.asset_ingress.evicted -gt 0) { $flags.Add('RECEIVER_ASSET_EVENTS_EVICTED') }
            $times = [ordered]@{
                gossip_to_prepare_ms = Delta (Local-Us $prepared) (Local-Us $gossip)
                gossip_to_reservation_ms = Delta (Local-Us $reservation) (Local-Us $gossip)
                reservation_to_admit_ms = Delta (Local-Us $admit) (Local-Us $reservation)
                admit_to_first_initial_ms = Delta $s.send.first_initial_us $s.send_admitted_us
                initial_span_ms = Delta $s.send.initial_complete_us $s.send.first_initial_us
                send_admit_to_terminal_ms = Delta $s.send_terminal_us $s.send_admitted_us
                first_send_to_first_receive_WALL_APPROX_ms = Delta (Wall-Us $receiveEvent $r.first_fragment_us) (Wall-Us $sendEvent $s.send.first_initial_us)
                receive_fragments_span_ms = Delta $r.fragments_complete_us $r.first_fragment_us
                hash_first_to_verified_ms = Delta $r.verified_us $r.first_hash_us
                final_fragment_to_verified_ms = Delta $r.verified_us $r.fragments_complete_us
                verified_to_handoff_APPROX_ms = Delta (Local-Us $receiveEvent) (Evidence-Us $receiveEvent $r.verified_us)
                handoff_to_decoded_ms = Delta (Local-Us $decodedEvent) (Local-Us $receiveEvent)
                prepare_to_decoded_WALL_APPROX_ms = Delta $decodedEvent.wall_unix_us $prepared.wall_unix_us
            }
            $unqualifiedTimes = $null
            if ($retryMixed) {
                $unqualifiedTimes = $times
                $times = [ordered]@{}
                foreach ($name in $unqualifiedTimes.Keys) {
                    $times[$name] = if ($name -in @('gossip_to_prepare_ms', 'gossip_to_reservation_ms')) { $unqualifiedTimes[$name] } else { $null }
                }
            }
            $cases.Add([pscustomobject]@{ sender_node = $node.index; peer_index = $p; receiver_node = $target
                session = Hex $gossip.detail.session; sequence = $gossip.detail.sequence
                ordered_ids = Hex $gossip.detail.ordered_ids; rows = $gossip.detail.rows
                repair_peer = $gossip.detail.repair_peer; fragment = $fragment; tag = $tag
                reserved = $peerState.reserved; terminal = $s.send_terminal_state; timing = $times
                send_attempts_observed = $attempts; attempt_qualification = $attemptQualification
                retry_mixed_unqualified_spans = $unqualifiedTimes
                packets = [ordered]@{ initial_written = $s.send.initial_packets; repair_written = $s.send.repair_packets
                    probe_written = $s.send.probe_packets; acknowledged_best = $s.acknowledged_best
                    received_unique = $r.fragments; expected = $r.total_fragments; accepted_repair = $r.repair_fragments }
                offer_elapsed_us = Local-Us $offer; reservation_elapsed_us = Local-Us $reservation
                repair_mask_before = $offer.detail.repair_mask_before; prefer_repair_before = $offer.detail.prefer_repair_before
                evidence_flags = @($flags | Select-Object -Unique)
                detail = [ordered]@{ gossip = Show $gossip; prepared = Show $prepared; wait = Show $wait
                    reservation = Show $reservation; offer = Show $offer; admitted = Show $admit
                    returned = Show $returned; received = Show $receiveEvent; decoded = Show $decodedEvent
                    retry_mixed_send_events = @(if ($retryMixed) { $attemptEvents | ForEach-Object { Show $_ } })
                    retry_mixed_receive_events = @(if ($retryMixed) { $receive | ForEach-Object { Show $_ } }) } })
        }
    }
}
$ranked = @($cases | Sort-Object { @($_.timing.Values | Where-Object { $null -ne $_ }) | Measure-Object -Maximum | Select-Object -ExpandProperty Maximum } -Descending | Select-Object -First $Top)
$groups = @($ranked | Where-Object ordered_ids | ForEach-Object { "$($_.rows):$($_.ordered_ids)" } | Select-Object -Unique)
$related = @($cases | Where-Object { "$($_.rows):$($_.ordered_ids)" -in $groups } | Select-Object -First ($Top * 12))
function Brief($Case) {
    [ordered]@{ sender_node = $Case.sender_node; peer_index = $Case.peer_index; receiver_node = $Case.receiver_node
        session = $Case.session; sequence = $Case.sequence; ordered_ids = $Case.ordered_ids; rows = $Case.rows
        repair_peer = $Case.repair_peer; fragment = $Case.fragment; tag = $Case.tag; reserved = $Case.reserved
        terminal = $Case.terminal; timing = $Case.timing; packets = $Case.packets; evidence_flags = $Case.evidence_flags
        send_attempts_observed = $Case.send_attempts_observed; attempt_qualification = $Case.attempt_qualification
        retry_mixed_unqualified_spans = $Case.retry_mixed_unqualified_spans
        offer_elapsed_us = $Case.offer_elapsed_us; repair_mask_before = $Case.repair_mask_before
        prefer_repair_before = $Case.prefer_repair_before }
}
# A mask bit proves some retained repair work, not which nonce. An older omitted
# exact group is associated only when a later same-peer repair reserves it and no
# earlier reservation for that exact group was observed.
$freshOverRepair = [Collections.Generic.List[object]]::new()
foreach ($fresh in @($cases | Where-Object { $null -eq $_.repair_peer -and $_.reserved -eq $true -and
    $null -ne $_.peer_index -and $null -ne $_.offer_elapsed_us -and
    ([uint64]$_.repair_mask_before -band ([uint64]1 -shl [int]$_.peer_index)) -ne 0 })) {
    $older = @($cases | Where-Object { $_.sender_node -eq $fresh.sender_node -and $_.peer_index -eq $fresh.peer_index -and
        $_.reserved -eq $false -and $null -ne $_.offer_elapsed_us -and $_.offer_elapsed_us -lt $fresh.offer_elapsed_us } |
        Sort-Object offer_elapsed_us)
    $matched = $null; $repair = $null
    foreach ($omitted in $older) {
        $next = @($cases | Where-Object { $_.sender_node -eq $fresh.sender_node -and $_.peer_index -eq $fresh.peer_index -and
            $_.ordered_ids -eq $omitted.ordered_ids -and $_.rows -eq $omitted.rows -and $_.reserved -eq $true -and
            $_.offer_elapsed_us -gt $omitted.offer_elapsed_us } | Sort-Object offer_elapsed_us | Select-Object -First 1)[0]
        if ($null -ne $next -and $next.offer_elapsed_us -gt $fresh.offer_elapsed_us -and $null -ne $next.repair_peer) {
            $matched = $omitted; $repair = $next; break
        }
    }
    $freshOverRepair.Add([pscustomobject]@{ sender_node = $fresh.sender_node; peer_index = $fresh.peer_index
        fresh = Brief $fresh; old_omitted = $(if ($null -ne $matched) { Brief $matched })
        later_exact_group_repair = $(if ($null -ne $repair) { Brief $repair })
        omitted_to_fresh_reservation_ms = Delta $fresh.offer_elapsed_us $matched.offer_elapsed_us
        fresh_to_old_repair_reservation_ms = Delta $repair.offer_elapsed_us $fresh.offer_elapsed_us
        provenance = $(if ($null -ne $matched) { 'fresh reserves peer with pending mask; earlier exact group omitted; next exact group reservation is later repair' }
            else { 'fresh reserves peer with pending mask; older exact group not established by retained events' })
        nonce_dependency = 'NOT_INFERRED'; fresh_offer = $fresh.detail.offer
        old_omission = $matched.detail.reservation; repair_offer = $repair.detail.offer })
}
$rankedFresh = @($freshOverRepair | Sort-Object fresh_to_old_repair_reservation_ms -Descending | Select-Object -First $Top)
$result = [ordered]@{ source = $resolved; source_sha256 = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash.ToLowerInvariant()
    binary_sha256 = $report.binary_sha256; passed = $report.passed; transactions = $report.transactions
    successful_tps_DIAGNOSTIC_RUN = $report.successful_tps; scope = 'same original input objects; no inferred packet loss or new performance claim'
    candidate_peer_cases = $cases.Count; top_count = $ranked.Count
    retry_mixed_cases_count = @($cases | Where-Object attempt_qualification -eq 'retry_mixed/unqualified').Count
    retry_mixed_cases = @($cases | Where-Object attempt_qualification -eq 'retry_mixed/unqualified' | Select-Object -First $Top |
        ForEach-Object { [ordered]@{ summary = Brief $_; events = $_.detail } })
    top_waits = @($ranked | ForEach-Object { Brief $_ })
    same_ordered_group_deliveries = @($related | ForEach-Object { Brief $_ })
    related_truncated = @($cases | Where-Object { "$($_.rows):$($_.ordered_ids)" -in $groups }).Count -gt $related.Count
    fresh_reservations_with_pending_repair_peer_count = $freshOverRepair.Count
    fresh_reservations_bypassing_preferred_repair_count = @($freshOverRepair | Where-Object { $_.fresh.prefer_repair_before -eq $true }).Count
    fresh_reservations_during_fair_alternation_count = @($freshOverRepair | Where-Object { $_.fresh.prefer_repair_before -eq $false }).Count
    fresh_before_older_exact_repair_count = @($freshOverRepair | Where-Object { $null -ne $_.old_omitted }).Count
    top_fresh_before_repair = $rankedFresh
    top_exact_evidence = @($ranked | ForEach-Object { [ordered]@{ sender_node = $_.sender_node; peer_index = $_.peer_index; sequence = $_.sequence; fragment = $_.fragment; events = $_.detail } })
    coverage = @($nodes | ForEach-Object { [ordered]@{ node = $_.index; snapshot = $_.origin; local_peer_PUBLIC = $_.local_peer
        asset_enabled = $_.snapshot.stage_profile.asset_ingress.enabled; events = $_.events.Count
        asset_evicted = $_.snapshot.stage_profile.asset_ingress.evicted
        transfer_events = @($_.events | Where-Object { $_.event -like 'input_transport_*' }).Count
        last_transfer_retention = @($_.events | Where-Object { $null -ne $_.detail.retention } | Select-Object -Last 1)[0].detail.retention
        first_event_ms = $(if ($_.events.Count) { $_.events[0].elapsed_ns / 1e6 })
        last_event_ms = $(if ($_.events.Count) { $_.events[-1].elapsed_ns / 1e6 }) } })
    issues = $issues.ToArray(); limits = @(
        'Missing events are UNKNOWN unless retained coverage proves observation; completed records can be evicted after export',
        'Ordered-ID digest plus row count identifies intents, not signature bytes; source_ref is never used across processes',
        'Sender and receiver tags bind authenticated session, stream and object; new repair scope means a different fragment',
        'Carrier write success is not remote receipt; accepted repair fragments exclude duplicates and cannot measure all retransmission traffic',
        'Hashing may overlap assembly; phase spans are not additive CPU/kernel time',
        'Observer clock is paired inside event closure after event timestamp: local alignment approximate; cross-node wall is not ordering authority',
        'Missing send_attempts_observed remains UNKNOWN, never zero; repeated same-tag sends are retry_mixed/unqualified and excluded from single-attempt phase timings',
        'Fresh reservation with prefer_repair_before=true bypasses a preferred repair; false is the existing fresh/repair fairness alternation, not this defect',
        'Receiver TTL reset has no generation counter: one tag does not prove one receiver assembly lifetime; mixed retry spans and retained events are not exact single-attempt durations',
        'Top selection ranks observed spans only and can omit uncompleted objects with no end timestamp; inspect evidence flags and same-group omissions'
    ) }
$destination = [IO.Path]::GetFullPath($OutputPath)
if (!(Test-Path -LiteralPath (Split-Path -Parent $destination) -PathType Container)) { throw 'Output parent must exist.' }
$result | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $destination -Encoding utf8NoBOM
Write-Output $destination
