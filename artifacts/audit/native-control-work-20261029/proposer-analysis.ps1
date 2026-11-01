param(
    [Parameter(Mandatory = $true)][string]$ReportPath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [int]$Height = 52
)
$ErrorActionPreference = 'Stop'

function Read-Json([string]$Path) {
    Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}
function Validator-Id($PublicKey) {
    $domain = [Text.Encoding]::UTF8.GetBytes("novovm-round-bft/v2/mldsa65/validator-id" + [char]0)
    $bytes = [byte[]]($domain + [byte[]]$PublicKey)
    [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
}
function Project-Event($Event, [long]$Origin) {
    [ordered]@{
        event = $Event.event; height = $Event.height; round = $Event.round
        wall_unix_us = $Event.wall_unix_us
        offset_from_proposer_parent_ack_ms = ([double]($Event.wall_unix_us - $Origin) / 1000)
        elapsed_ns = $Event.elapsed_ns; duration_ns = $Event.duration_ns
        body = $Event.body; fragment = $Event.fragment
    }
}

$resolvedReport = (Resolve-Path -LiteralPath $ReportPath).Path
$directory = Split-Path -Parent $resolvedReport
$report = Read-Json $resolvedReport
$configs = @(0..3 | ForEach-Object { Read-Json (Join-Path $directory "node-$_.json") })
$progress = @(0..3 | ForEach-Object { Read-Json (Join-Path $directory "create-$_.progress.json") })
$members = @($configs[0].genesis.validators | ForEach-Object {
    [pscustomobject]@{ id = (Validator-Id $_.public_key); weight = $_.weight }
} | Sort-Object id)
$mapping = @()
for ($i = 0; $i -lt 4; $i++) {
    # Public configuration only. Never read signing_key_file or transport_key_file.
    $address = $configs[$i].udp_addr
    $peerPublic = @($configs | ForEach-Object { $_.peers } | Where-Object {
        $_.PSObject.Properties['address'].Value -eq $address
    })
    $ids = @($peerPublic | ForEach-Object { Validator-Id $_.public_key } | Select-Object -Unique)
    if ($ids.Count -ne 1) { throw "Ambiguous public validator mapping for node $i" }
    $mapping += [pscustomobject]@{ node = $i; udp_addr = $address; validator_id = $ids[0] }
}
$acks = @(0..3 | ForEach-Object {
    @($progress[$_].stage_profile.critical_path.events | Where-Object {
        $_.height -eq $Height -and $_.event -eq 'decision_durable_ack'
    }) | Select-Object -First 1
})
if ($acks.Count -ne 4 -or @($acks | Where-Object { $null -eq $_ }).Count) {
    throw 'All four target-height durable ACK records are required'
}
$bodies = @($acks.body | Select-Object -Unique)
$rounds = @($acks.round | Select-Object -Unique)
if ($bodies.Count -ne 1 -or $rounds.Count -ne 1) { throw 'Target ACK body/round mismatch' }
$body = $bodies[0]
$round = [long]$rounds[0]
# GenesisConfig::prepare uses activation_height=1; ValidatorSet::leader uses sorted IDs.
$leaderIndex = ([long]$Height - 1 + $round) % $members.Count
$leader = $members[$leaderIndex].id
$proposer = @($mapping | Where-Object { $_.validator_id -eq $leader })
if ($proposer.Count -ne 1) { throw 'Proposer does not map to exactly one public node configuration' }
$proposerNode = $proposer[0].node
$parentAck = @($progress[$proposerNode].stage_profile.critical_path.events | Where-Object {
    $_.height -eq ($Height - 1) -and $_.event -eq 'decision_durable_ack'
}) | Select-Object -First 1
if ($null -eq $parentAck) { throw 'Proposer parent-height ACK missing' }
$origin = [long]$parentAck.wall_unix_us
$nodes = @()
for ($i = 0; $i -lt 4; $i++) {
    $stage = $progress[$i].stage_profile
    $critical = @($stage.critical_path.events)
    $matched = @($critical | Where-Object { $_.height -eq $Height -and $_.body -eq $body })
    $execution = @($matched | Where-Object { $_.event -eq 'candidate_execution_done' }) | Select-Object -First 1
    $proposal = @($matched | Where-Object { $_.event -eq 'proposal_received' }) | Select-Object -First 1
    $localParent = @($critical | Where-Object {
        $_.height -eq ($Height - 1) -and $_.event -eq 'decision_durable_ack'
    }) | Select-Object -First 1
    $identity = @($stage.control_work.events | Where-Object {
        $_.height -eq $Height -and $_.body -eq $body -and $_.event -eq 'body_identity'
    } | Sort-Object wall_unix_us) | Select-Object -First 1
    $cutoff = if ($null -ne $proposal) { [long]$proposal.wall_unix_us } else { [long]$acks[$i].wall_unix_us }
    $received = @($stage.native_delivery.events | Where-Object {
        $_.height -eq $Height -and $_.event -eq 'received' -and $_.wall_unix_us -le $cutoff
    })
    $decodes = @($stage.control_work.events | Where-Object {
        $_.height -eq $Height -and $_.event -eq 'native_decode'
    })
    $unbound = @($critical | Where-Object {
        $_.height -eq $Height -and $_.event -in @('signature_batch_done', 'authority_admission_done')
    })
    $nodes += [ordered]@{
        node = $i; validator_id = $mapping[$i].validator_id; is_target_proposer = ($i -eq $proposerNode)
        critical_dropped = $stage.critical_path.dropped
        control_evicted = $stage.control_work.evicted
        delivery_evicted = $stage.native_delivery.evicted
        parent_ack = $(if ($null -ne $localParent) { Project-Event $localParent $origin })
        same_body_identity_end = $(if ($null -ne $identity) { Project-Event $identity $origin })
        same_body_critical = @($matched | ForEach-Object { Project-Event $_ $origin })
        execution_to_proposal_ms = $(if ($null -ne $execution -and $null -ne $proposal) {
            [double]($proposal.wall_unix_us - $execution.wall_unix_us) / 1000
        })
        identity_end_to_execution_ms = $(if ($null -ne $identity -and $null -ne $execution) {
            [double]($execution.wall_unix_us - $identity.wall_unix_us) / 1000
        })
        context_only_admission_activity_NOT_same_body_proof = @($unbound | ForEach-Object { Project-Event $_ $origin })
        pre_proposal_received_context_objects_NOT_body_identified = @($received | Select-Object event, height, wall_unix_us, peer, fragment, purposes)
        context_decode_records_NOT_body_identified = @($decodes | ForEach-Object { Project-Event $_ $origin })
    }
}
$result = [ordered]@{
    report = $resolvedReport; binary_sha256 = $report.binary_sha256
    transactions = $report.transactions; successful_tps = $report.successful_tps
    target_height = $Height; target_round = $round; target_body_id = $body
    proposer_node = $proposerNode; proposer_validator_id = $leader
    proposer_derivation = 'SHA256(validator-id domain || public key); sort IDs; (height - activation_height 1 + round) modulo member count'
    public_node_mapping = $mapping
    wall_origin_unix_us = $origin
    nodes = $nodes
    source_rules = @(
        'round_bft/wire.rs: Validator::new and ValidatorSet::leader define ID and sorted-ID round robin',
        'service/config.rs: GenesisConfig::prepare constructs ValidatorSet with activation_height=1',
        'consensus/bodies.rs: follower Ready candidate waits for verified proposal demand before registration',
        'consensus/progress.rs::drive_consensus: proposer without an existing valid certificate selects a registered candidate with local_round == current round',
        'native_lifecycle/successor.rs: proactive AssetBody fanout covers selected successor; current-height prepared follower results do not grant remote execution or proposal authority'
    )
    interpretation_limits = @(
        'body ID matches bind identity/execute/proposal/ACK observations to the same candidate; context-only signature/authority events do not identify the batch',
        'received events contain fragment ID and peer but not message variant or candidate body ID; they cannot establish that the target body arrived early',
        'native_decode records are thresholded and bounded; absence is not proof of no arrival or no decode',
        'wall clock supports cross-process alignment only; per-node monotonic durations remain the ordering authority',
        'late proposer body preparation is observed; the upstream cause (gossip delivery, pending nonce availability, capture or scheduling) is not established by these records'
    )
}
$fullOutput = [IO.Path]::GetFullPath($OutputPath)
$parentOutput = Split-Path -Parent $fullOutput
if (-not (Test-Path -LiteralPath $parentOutput -PathType Container)) { throw 'Output parent must already exist' }
$result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $fullOutput -Encoding utf8
Write-Output $fullOutput
