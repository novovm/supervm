param(
    [Parameter(Mandatory = $true)][string]$ReportPath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [ValidateRange(1, [int]::MaxValue)][int]$Height = 52
)
$ErrorActionPreference = 'Stop'
$issues = [Collections.Generic.List[object]]::new()

function Hash-Hex($Value) {
    if ($null -eq $Value) { return $null }
    if ($Value -is [string]) { return ($Value -replace '^0x', '').ToLowerInvariant() }
    [Convert]::ToHexString([byte[]]$Value).ToLowerInvariant()
}
function Validator-Id($PublicKey) {
    $domain = [Text.Encoding]::UTF8.GetBytes("novovm-round-bft/v2/mldsa65/validator-id" + [char]0)
    Hash-Hex ([Security.Cryptography.SHA256]::HashData([byte[]]($domain + [byte[]]$PublicKey)))
}
function Note-Issue([string]$Code, $Node, [string]$Message) {
    $issues.Add([ordered]@{ code = $Code; node = $Node; message = $Message })
}
function Show-Event($Event, [int]$Node) {
    if ($null -eq $Event) { return $null }
    $detail = if ($null -ne $Event.detail) { $Event.detail } else { [pscustomobject]@{} }
    $entry = [ordered]@{ event = $Event.event; height = $Event.height; round = $Event.round; elapsed_ms = $Event.elapsed_ns / 1e6 }
    $entry['from_proposer_parent_ack_ms'] = if ($Node -eq $proposerNode -and $null -ne $parentAck) {
        ($Event.elapsed_ns - $parentAck.elapsed_ns) / 1e6
    } else { $null }
    $entry['wall_alignment_ms_NOT_ordering_authority'] = if ($null -ne $parentAck) {
        ($Event.wall_unix_us - $parentAck.wall_unix_us) / 1000
    } else { $null }
    foreach ($key in @('kind', 'source_ref', 'rows', 'first_index', 'last_index', 'multiple_sources',
        'remaining', 'accepted_results', 'rejected_results', 'parent_height', 'parent_root', 'ok',
        'reason', 'selected', 'window_len', 'pending', 'boundary', 'awaiting_admission', 'budget_full',
        'window_first_tx', 'first_unknown_nonce', 'pool_wait', 'index', 'tx', 'nonce', 'parent_nonce', 'signer', 'repair_peer',
        'local', 'successor', 'parent_state_root', 'parent_block_hash', 'parent_state_version', 'broadcast')) {
        if ($null -ne $detail.PSObject.Properties[$key]) { $entry[$key] = $detail.$key }
    }
    if ($null -ne $detail.indices) { $entry['indices_count'] = @($detail.indices).Count }
    foreach ($key in @('body', 'fragment', 'ordered_ids', 'session')) {
        $value = if ($null -ne $detail.$key) { $detail.$key } else { $Event.$key }
        if ($null -ne $value) { $entry[$key] = Hash-Hex $value }
    }
    if ($null -ne $detail.sequence) { $entry['sequence'] = $detail.sequence }
    $entry
}

# Only these events construct a NEW batch Arc. Prepared/identified/row events
# observe an existing Arc and must not reset its lifetime.
$creationEvents = @('asset_received', 'gossip_batch_prepared', 'selection_body_prepared')
function Source-Life($Node, $Reference, [long]$Anchor) {
    if ($null -eq $Reference) { return $null }
    $same = @($Node.events | Where-Object {
        $null -ne $_.detail.source_ref -and [string]$_.detail.source_ref -eq [string]$Reference
    })
    $created = @($same | Where-Object {
        $_.event -in $creationEvents -and $_.elapsed_ns -le $Anchor
    } | Select-Object -Last 1)
    if ($created.Count -ne 1) { return $null }
    $start = [long]$created[0].elapsed_ns
    $next = @($same | Where-Object {
        $_.event -in $creationEvents -and $_.elapsed_ns -gt $start
    } | Select-Object -First 1)
    $end = if ($next.Count) { [long]$next[0].elapsed_ns } else { [long]::MaxValue }
    [pscustomobject]@{ reference = [string]$Reference; created = $created[0]; next = $next
        start = $start; end = $end
        events = @($same | Where-Object { $_.elapsed_ns -ge $start -and $_.elapsed_ns -lt $end }) }
}

function Body-Join($Node, [string]$Body) {
    foreach ($identified in @($Node.events | Where-Object {
        $_.event -eq 'body_identified' -and $_.height -eq $Height -and
        (Hash-Hex $_.detail.body) -eq $Body -and $_.elapsed_ns -le $Node.ack.elapsed_ns
    } | Sort-Object @{ Expression = { $_.detail.local }; Descending = $true }, elapsed_ns)) {
        $life = Source-Life $Node $identified.detail.source_ref $identified.elapsed_ns
        if ($null -eq $life) { continue }
        $selection = @($life.events | Where-Object {
            $_.event -eq 'selection_body_prepared' -and $_.elapsed_ns -le $identified.elapsed_ns
        } | Select-Object -Last 1)
        if ($selection.Count -eq 1) {
            return [pscustomobject]@{ identified = $identified; selection = $selection[0]; life = $life }
        }
    }
    $null
}

function Source-Summary($Node, $Received) {
    $life = Source-Life $Node $Received.detail.source_ref $Received.elapsed_ns
    if ($null -eq $life -or $life.created.event -ne 'asset_received' -or
        (Hash-Hex $life.created.detail.fragment) -ne (Hash-Hex $Received.detail.fragment)) {
        Note-Issue 'source_lifetime_unknown' $Node.index 'Received source generation could not be bounded; no pointer-only phase join was made.'
        return [ordered]@{ node = $Node.index; status = 'UNKNOWN'; received = (Show-Event $Received $Node.index) }
    }
    $phases = @($life.events | Where-Object { $_.event -like 'signature_*' -or $_.event -like 'authority_*' })
    $expected = @('signature_queued', 'signature_submit_accepted', 'signature_compute_begin',
        'signature_compute_reply_ready', 'signature_control_observed', 'authority_queued',
        'authority_capture_begin', 'authority_state_ready', 'authority_drained')
    $missing = @($expected | Where-Object { $_ -notin $phases.event })
    if ($missing.Count) { Note-Issue 'source_phases_incomplete' $Node.index ($missing -join ', ') }
    if (@($phases | Where-Object { $_.detail.multiple_sources -eq $true }).Count) {
        Note-Issue 'multiple_sources' $Node.index 'One phase includes more than one Arc; its first source_ref cannot attribute all rows.'
    }
    $drained = @($phases | Where-Object event -eq 'authority_drained')
    $rowSamples = @($life.events | Where-Object {
        $_.event -in @('row_seen', 'row_pool_insert') -or
        ($_.event -eq 'selection_row' -and $_.height -eq $Height)
    })
    $cutoff = if ($null -ne $Node.bodyJoin) { $Node.bodyJoin.identified.elapsed_ns } else { $Node.ack.elapsed_ns }
    $gates = @($Node.events | Where-Object {
        $_.event -eq 'selection_gate' -and $_.height -eq $Height -and
        $_.elapsed_ns -ge $Received.elapsed_ns -and $_.elapsed_ns -le $cutoff
    })
    $localParent = @($Node.stage.critical_path.events | Where-Object {
        $_.event -eq 'decision_durable_ack' -and $_.height -eq ($Height - 1)
    } | Select-Object -First 1)
    [ordered]@{
        node = $Node.index; status = $(if ($missing.Count) { 'INCOMPLETE' } else { 'OBSERVED_PHASES' })
        source_ref_LOCAL_ONLY = $life.reference; received = (Show-Event $Received $Node.index)
        received_before_local_target_height_ack = $(if ($null -ne $Node.ack) { $Received.elapsed_ns -le $Node.ack.elapsed_ns })
        received_before_local_parent_ack = $(if ($localParent.Count) { $Received.elapsed_ns -le $localParent[0].elapsed_ns })
        received_before_local_selection = $(if ($null -ne $Node.bodyJoin) { $Received.elapsed_ns -le $Node.bodyJoin.selection.elapsed_ns })
        next_same_address_creation = $(if ($life.next.Count) { Show-Event $life.next[0] $Node.index })
        missing_phases = $missing
        phases = @($phases | ForEach-Object { Show-Event $_ $Node.index })
        authority_drained_batches = @($drained | ForEach-Object {
            [ordered]@{ rows_after_signature_prefilter = $_.detail.rows
                accepted_results_INCLUDING_exact_duplicates = $_.detail.accepted_results
                rejected_results = $_.detail.rejected_results; remaining = $_.detail.remaining }
        })
        sampled_rows_ONLY_hash_first_byte_zero = @($rowSamples | ForEach-Object { Show-Event $_ $Node.index })
        selection_gates_CONTEXT_NOT_batch_membership = @($gates | ForEach-Object { Show-Event $_ $Node.index })
    }
}

$resolvedReport = (Resolve-Path -LiteralPath $ReportPath).Path
$directory = Split-Path -Parent $resolvedReport
$report = Get-Content -LiteralPath $resolvedReport -Raw | ConvertFrom-Json
$configs = @(); $nodes = @(); $mapping = @()
for ($i = 0; $i -lt 4; $i++) {
    $configPath = Join-Path $directory "node-$i.json"
    $configs += $(if (Test-Path -LiteralPath $configPath) {
        Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    } else { $null })
    $embedded = @($report.final_local_progress.nodes | Where-Object node_index -eq $i)
    $snapshot = if ($embedded.Count -eq 1) { $embedded[0].snapshot } else { $null }
    $snapshotSource = 'report.final_local_progress'
    if ($null -eq $snapshot.stage_profile) {
        $progressPath = Join-Path $directory "create-$i.progress.json"
        $snapshotSource = $progressPath
        $snapshot = if (Test-Path -LiteralPath $progressPath) {
            Get-Content -LiteralPath $progressPath -Raw | ConvertFrom-Json
        } else { $null }
    }
    $stage = $snapshot.stage_profile
    $ack = @($stage.critical_path.events | Where-Object {
        $_.event -eq 'decision_durable_ack' -and $_.height -eq $Height
    } | Sort-Object elapsed_ns | Select-Object -First 1)
    $nodes += [pscustomobject]@{ index = $i; stage = $stage; snapshot_source = $snapshotSource
        events = @($stage.asset_ingress.events | Where-Object { $null -ne $_ } | Sort-Object elapsed_ns)
        ack = $(if ($ack.Count) { $ack[0] }); bodyJoin = $null }
    if (!$ack.Count) { Note-Issue 'ack_missing' $i 'Target-height durable ACK is unavailable.' }
    if ($stage.asset_ingress.enabled -ne $true) { Note-Issue 'trace_disabled_or_missing' $i 'asset_ingress is not enabled in the selected snapshot.' }
    if ($stage.asset_ingress.evicted -gt 0) { Note-Issue 'trace_evicted' $i "asset_ingress evicted=$($stage.asset_ingress.evicted); absence is not proof of no event." }
}

$body = $null; $round = $null; $proposerNode = $null; $parentAck = $null
$ackBodies = @($nodes | Where-Object { $null -ne $_.ack } | ForEach-Object { Hash-Hex $_.ack.body } | Select-Object -Unique)
$ackRounds = @($nodes | Where-Object { $null -ne $_.ack } | ForEach-Object { $_.ack.round } | Select-Object -Unique)
if ($ackBodies.Count -eq 1 -and $ackBodies[0] -and $ackRounds.Count -eq 1) {
    $body = $ackBodies[0]; $round = [long]$ackRounds[0]
} else { Note-Issue 'ack_body_or_round_mismatch' $null 'No unambiguous final body/round; source association was not attempted.' }
try {
    if (@($configs | Where-Object { $null -eq $_ }).Count -or $configs.Count -ne 4) { throw 'Four public node configs are required.' }
    $members = @($configs[0].genesis.validators | ForEach-Object { Validator-Id $_.public_key } | Sort-Object)
    for ($i = 0; $i -lt 4; $i++) {
        $address = $configs[$i].udp_addr
        $ids = @($configs | ForEach-Object { $_.peers } | Where-Object address -eq $address |
            ForEach-Object { Validator-Id $_.public_key } | Select-Object -Unique)
        if ($ids.Count -ne 1) { throw "Ambiguous public key mapping for node $i" }
        $mapping += [ordered]@{ node = $i; udp_addr = $address; validator_id = $ids[0] }
    }
    if ($null -ne $body) {
        $leader = $members[([long]$Height - 1 + $round) % $members.Count]
        $leaderMapping = @($mapping | Where-Object validator_id -eq $leader)
        if ($leaderMapping.Count -ne 1) { throw 'Proposer mapping is not unique.' }
        $proposerNode = [int]$leaderMapping[0].node
        $parentAck = @($nodes[$proposerNode].stage.critical_path.events | Where-Object {
            $_.event -eq 'decision_durable_ack' -and $_.height -eq ($Height - 1)
        } | Sort-Object elapsed_ns | Select-Object -First 1) | Select-Object -First 1
    }
} catch { Note-Issue 'public_mapping_unknown' $null $_.Exception.Message }

$selected = $null; $senders = @(); $bodyNodes = @()
if ($null -ne $body) {
    foreach ($node in $nodes) {
        $node.bodyJoin = Body-Join $node $body
        $localParent = @($node.stage.critical_path.events | Where-Object {
            $_.event -eq 'decision_durable_ack' -and $_.height -eq ($Height - 1)
        } | Select-Object -First 1)
        $gateStart = if ($localParent.Count) { $localParent[0].elapsed_ns } else { 0 }
        $gateEnd = if ($null -ne $node.bodyJoin) { $node.bodyJoin.identified.elapsed_ns } else { $node.ack.elapsed_ns }
        if ($null -ne $node.bodyJoin) { $gateStart = [Math]::Min($gateStart, $node.bodyJoin.selection.elapsed_ns) }
        $bodyNodes += [ordered]@{ node = $node.index
            status = $(if ($null -eq $node.bodyJoin) { 'UNKNOWN_LOCAL_SELECTION' } else { 'SAME_LIVE_ARC' })
            parent_ack = $(if ($localParent.Count) { Show-Event $localParent[0] $node.index })
            body_identified = (Show-Event $node.bodyJoin.identified $node.index)
            selection = (Show-Event $node.bodyJoin.selection $node.index)
            same_body_critical = @($node.stage.critical_path.events | Where-Object {
                $_.height -eq $Height -and (Hash-Hex $_.body) -eq $body
            } | ForEach-Object { Show-Event $_ $node.index })
            sampled_pool_inserts_CONTEXT_NOT_batch_membership = @($node.events | Where-Object {
                $_.event -eq 'row_pool_insert' -and $_.detail.parent_height -eq ($Height - 1) -and
                $_.elapsed_ns -ge $gateStart -and $_.elapsed_ns -le $gateEnd
            } | Select-Object -First 32 | ForEach-Object { Show-Event $_ $node.index })
            selection_gates_CONTEXT_NOT_batch_membership = @($node.events | Where-Object {
                $_.event -eq 'selection_gate' -and $_.height -eq $Height -and
                $_.elapsed_ns -ge $gateStart -and $_.elapsed_ns -le $gateEnd
            } | ForEach-Object { Show-Event $_ $node.index }) }
    }
    if ($null -ne $proposerNode) { $selected = $nodes[$proposerNode].bodyJoin }
    if ($null -eq $selected) { Note-Issue 'proposer_selection_unknown' $proposerNode 'Final body could not be joined to a selection in the same local Arc generation.' }
}
if ($null -ne $selected) {
    $digest = Hash-Hex $selected.selection.detail.ordered_ids
    $rows = $selected.selection.detail.rows
    foreach ($sender in $nodes) {
        foreach ($gossip in @($sender.events | Where-Object {
            $_.event -eq 'gossip_batch_prepared' -and $_.detail.rows -eq $rows -and
            (Hash-Hex $_.detail.ordered_ids) -eq $digest -and $null -ne $digest
        })) {
            $life = Source-Life $sender $gossip.detail.source_ref $gossip.elapsed_ns
            $prepared = @($life.events | Where-Object {
                $_.event -eq 'asset_prepared' -and $_.detail.kind -eq 'asset_transactions' -and
                $_.elapsed_ns -ge $gossip.elapsed_ns -and $_.detail.rows -eq $rows -and
                $_.detail.sequence -eq $gossip.detail.sequence -and
                (Hash-Hex $_.detail.session) -eq (Hash-Hex $gossip.detail.session)
            } | Sort-Object elapsed_ns)
            if (!$prepared.Count) { Note-Issue 'prepared_fragment_unknown' $sender.index 'Matching gossip digest has no asset_prepared in its bounded Arc generation.' }
            $fragments = @($prepared | ForEach-Object { Hash-Hex $_.detail.fragment } | Select-Object -Unique)
            if ($fragments.Count -gt 1) { Note-Issue 'source_fragment_mismatch' $sender.index 'One live source/scope produced multiple fragments; all are retained as alternatives.' }
            $receivers = @()
            foreach ($fragment in $fragments) {
                foreach ($receiver in $nodes) {
                    foreach ($received in @($receiver.events | Where-Object {
                        $_.event -eq 'asset_received' -and $_.detail.kind -eq 'asset_transactions' -and
                        (Hash-Hex $_.detail.fragment) -eq $fragment
                    })) {
                        if ($received.detail.rows -ne $rows -or $received.detail.sequence -ne $gossip.detail.sequence -or
                            (Hash-Hex $received.detail.session) -ne (Hash-Hex $gossip.detail.session)) {
                            Note-Issue 'received_scope_or_rows_mismatch' $receiver.index 'Same fragment has inconsistent source metadata; phase attribution was refused.'
                            continue
                        }
                        $receivers += Source-Summary $receiver $received
                    }
                }
            }
            $senders += [ordered]@{ node = $sender.index; gossip = (Show-Event $gossip $sender.index)
                first_prepared = $(if ($prepared.Count) { Show-Event $prepared[0] $sender.index })
                prepared_observations = $prepared.Count; fragments = $fragments; received_sources = $receivers }
        }
    }
    if (!$senders.Count) { Note-Issue 'full_ordered_digest_unmatched' $null 'No gossip has identical count AND ordered-ID digest; regrouping/order changes or eviction remain possible.' }
    if ($null -ne $proposerNode -and !@($senders | ForEach-Object { $_.received_sources } | Where-Object node -eq $proposerNode).Count) {
        Note-Issue 'proposer_received_source_unknown' $proposerNode 'No validator-gossip received source was joined for the proposer; a local client origin is possible, not a missing-delivery finding.'
    }
}

$result = [ordered]@{
    report = $resolvedReport; binary_sha256 = $report.binary_sha256; passed = $report.passed
    transactions = $report.transactions; successful_tps = $report.successful_tps
    target_height = $Height; target_round = $round; final_body_id = $body; proposer_node = $proposerNode
    public_node_mapping = $mapping
    proposer_parent_ack = (Show-Event $parentAck $(if ($null -ne $proposerNode) { $proposerNode } else { -1 }))
    same_body_local_selections = $bodyNodes; matching_ordered_input_batches = $senders
    coverage = @($nodes | ForEach-Object { [ordered]@{ node = $_.index; snapshot = $_.snapshot_source
        asset_enabled = $_.stage.asset_ingress.enabled; asset_events = $_.events.Count
        asset_evicted = $_.stage.asset_ingress.evicted; critical_dropped = $_.stage.critical_path.dropped
        control_evicted = $_.stage.control_work.evicted; delivery_evicted = $_.stage.native_delivery.evicted
        first_asset_ms = $(if ($_.events.Count) { $_.events[0].elapsed_ns / 1e6 })
        last_asset_ms = $(if ($_.events.Count) { $_.events[-1].elapsed_ns / 1e6 }) } })
    issues = $issues.ToArray()
    interpretation_limits = @(
        'source_ref is a process-local Arc address; joins stop at the next creation at that address, never across processes',
        'full ordered transaction-ID digest plus count identifies this ordered intent group, NOT signatures; real verification/economics are separate evidence',
        'row_seen/row_pool_insert/selection_row sample only hash[0]==0; missing samples do not prove absent rows',
        'phase events remain individual observations: multiple work chunks/parent retries must not be collapsed into one first-to-last compute duration',
        'compute_begin may precede submit_accepted because submission unparks the worker before recording; ok means invocation success, not all rows valid',
        'authority trace rows are after signature/static prefilter; accepted_results includes exact duplicates, and drained includes pool-backpressure time',
        'selection gates describe local context/state changes, not proof that every pending row belongs to this input',
        'wall alignment assists cross-process comparison only; elapsed_ms is the per-process ordering authority; eviction/unknown are not zero waits'
    )
}
$fullOutput = [IO.Path]::GetFullPath($OutputPath)
if (!(Test-Path -LiteralPath (Split-Path -Parent $fullOutput) -PathType Container)) { throw 'Output parent must already exist.' }
$result | ConvertTo-Json -Depth 18 | Set-Content -LiteralPath $fullOutput -Encoding utf8NoBOM
Write-Output $fullOutput
