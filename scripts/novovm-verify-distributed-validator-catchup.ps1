#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $PlanFile,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string] $PlanSha256,
    [Parameter(Mandatory)][string] $EvidenceDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
function Assert-Test([bool] $Pass, [string] $Message) {
    if (-not $Pass) { throw $Message }
}
function Read-Json([string] $Name) {
    Get-Content -LiteralPath (Join-Path $EvidenceDirectory "$Name.json") -Raw | ConvertFrom-Json
}
function Hex-Bytes($Value) {
    [Convert]::ToHexString([byte[]] $Value).ToLowerInvariant()
}
function Json($Value) { ConvertTo-Json -InputObject $Value -Depth 15 -Compress }
function Query-Started($Row) {
    $start = $Row.PSObject.Properties['query_started_unix']
    if ($null -eq $start) { return $Row.observed_unix }
    $start.Value
}

Assert-Test ((Get-FileHash -LiteralPath $PlanFile).Hash.ToLowerInvariant() -ceq $PlanSha256) 'Plan changed'
$plan = Get-Content -LiteralPath $PlanFile -Raw | ConvertFrom-Json
$genesis = Read-Json 'genesis'
Assert-Test ($genesis.plan_sha256 -ceq $PlanSha256 -and $genesis.genesis.chain_id -eq $plan.chain_id) 'Genesis is for another plan'
$genesisHash = (Get-FileHash -LiteralPath (Join-Path $EvidenceDirectory 'genesis.json')).Hash.ToLowerInvariant()
$domain = Hex-Bytes $genesis.genesis.genesis_config_commitment
$protocol = Hex-Bytes $genesis.genesis.protocol_commitment

function Checked-Run([string] $Name, [string] $Action, [string] $Role) {
    $row = Read-Json $Name
    Assert-Test ($row.plan_sha256 -ceq $PlanSha256 -and $row.run_id -ceq $plan.run_id -and
        $row.chain_id -eq $plan.chain_id -and $row.genesis -ceq $domain -and $row.protocol -ceq $protocol -and
        $row.action -ceq $Action -and $row.role -ceq $Role) "Wrong run, action, or role in $Name"
    if ($Action -ne 'archive') {
        Assert-Test ($row.observed_unix -ge $plan.start_unix -and $row.observed_unix -lt $plan.end_unix) "Out of window: $Name"
    }
    if ($Action -ceq 'query') {
        $started = Query-Started $row
        Assert-Test ($started -is [long] -and $row.observed_unix -is [long] -and
            $started -ge $plan.start_unix -and $started -le $row.observed_unix) "Invalid query timestamps: $Name"
    }
    $row
}
function Assert-Head($Left, $Right) {
    foreach ($field in @('height','block_hash','state_root','receipt_batch_commitment','state_version')) {
        Assert-Test ((Json $Left.$field) -ceq (Json $Right.$field)) "Head mismatch: $field"
    }
}
function Assert-Archive($Record, $Status) {
    Assert-Test ($Record.height -eq $Status.height -and $Record.block_hash -ceq (Hex-Bytes $Status.block_hash) -and
        $Record.state_root -ceq (Hex-Bytes $Status.state_root) -and
        $Record.receipt_batch_commitment -ceq (Hex-Bytes $Status.receipt_batch_commitment) -and
        $Record.quorum_weight -eq 3 -and $Record.signed_weight -in @(3,4) -and
        $Record.signatures -eq $Record.signed_weight -and
        @($Record.signers | Sort-Object -Unique).Count -eq $Record.signatures -and
        $Record.outbox_sha256 -cmatch '^[a-f0-9]{64}$') 'Missing durable head or its verified Precommit QC'
}
function Confirm-FinalReceipts($Row, $Submission) {
    Assert-Test ($Row.nonce -eq $Submission.nonce -and $Row.receipts.Count -eq 2) 'Wrong receipt request'
    for ($i = 0; $i -lt 2; $i++) {
        $receipt = $Row.receipts[$i]
        Assert-Test ($receipt.hash -ceq $Submission.marker_hashes[$i] -and
            $null -ne $receipt.state.PSObject.Properties['Finalized'] -and $receipt.state.Finalized.success -eq $true -and
            $receipt.state.Finalized.nonce_after -eq ($Row.nonce + 1) -and
            $receipt.state.Finalized.height -le $receipt.indexed_height) 'Missing successful original marker finality'
    }
}

$roles = @('A1','A2','C1','D1')
foreach ($role in $roles) {
    $ready = Read-Json "ready-$role"
    $slot = @($plan.slots | Where-Object role -CEQ $role)[0]
    Assert-Test ($ready.role -ceq $role -and $ready.plan_sha256 -ceq $PlanSha256 -and
        $ready.run_id -ceq $plan.run_id -and $ready.chain_id -eq $plan.chain_id -and
        $ready.address -ceq $slot.address -and $ready.genesis_sha256 -ceq $genesisHash -and
        $ready.node_sha256 -ceq $plan.node_sha256 -and $ready.aoem_sha256 -ceq $plan.aoem_sha256 -and
        $ready.harness_sha256 -ceq $plan.harness_sha256 -and $ready.ctl_sha256 -ceq $plan.ctl_sha256) "Mismatched readiness for $role"
}

$aLaunch = Checked-Run 'A1-create' 'create' 'A1'
$cLaunch = Checked-Run 'C1-create' 'create' 'C1'
$lateFirst = Checked-Run 'D1-create' 'create' 'D1'
$lateCatchup = Checked-Run 'D1-catchup' 'existing' 'D1'
$secondFirst = Checked-Run 'A2-create' 'create' 'A2'
$secondRestore = Checked-Run 'A2-restore' 'existing' 'A2'
$lateRestore = Checked-Run 'D1-restore' 'existing' 'D1'
$before = Checked-Run 'D1-baseline' 'query' 'D1'
$healthy = Checked-Run 'A1-advanced' 'query' 'A1'
$caught = Checked-Run 'D1-caught' 'query' 'D1'
Assert-Test ($before.status.height -ge 1 -and $before.observed_unix -le $lateFirst.stopped_unix -and
    $lateFirst.stopped_unix -le $healthy.observed_unix -and $healthy.observed_unix -le $lateCatchup.started_unix -and
    $caught.observed_unix -ge $lateCatchup.started_unix -and $caught.observed_unix -le $lateCatchup.stopped_unix -and
    $healthy.status.height -ge ($before.status.height + 40)) 'D1 did not miss and automatically catch up on 40 durable heights'
Assert-Head $healthy.status $caught.status
$old = (Checked-Run 'D1-outbox-before' 'archive' 'D1').archive
$preserved = (Checked-Run 'D1-outbox-after' 'archive' 'D1').archive
Assert-Archive $old $before.status
Assert-Archive $preserved $before.status
Assert-Test ($old.record_sha256 -ceq $preserved.record_sha256 -and
    $old.outbox_sha256 -ceq $preserved.outbox_sha256) 'The original durable signing record/outbox changed'

$submissions = @(0..41 | ForEach-Object {
    $sent = Checked-Run "submit-$_" 'submit' 'A1'
    Assert-Test ($sent.operator -ceq 'A1' -and $sent.nonce -eq $_ -and $sent.transport_completed -eq $true -and
        $sent.marker_hashes.Count -eq 2 -and $sent.marker_hashes[0] -cne $sent.marker_hashes[1] -and
        @($sent.marker_hashes | Where-Object { $_ -cnotmatch '^[a-f0-9]{64}$' }).Count -eq 0) "Invalid original submission $_"
    if ($_ -in 1..40) {
        Assert-Test ($sent.observed_unix -ge $lateFirst.stopped_unix -and
            $sent.observed_unix -le $healthy.observed_unix) "Submission $_ did not run with D1 absent"
    }
    $sent
})
for ($i = 1; $i -lt $submissions.Count; $i++) {
    Assert-Test ($submissions[$i].observed_unix -ge $submissions[$i - 1].observed_unix) 'Submissions are out of order'
}
Confirm-FinalReceipts $before $submissions[0]
Confirm-FinalReceipts $healthy $submissions[40]
Confirm-FinalReceipts $caught $submissions[40]
Assert-Test ((Json $healthy.receipts) -ceq (Json $caught.receipts)) 'D1 caught up to a different receipt projection'

$minoritySent = $submissions[41]
Assert-Test ($minoritySent.observed_unix -ge [Math]::Max($lateCatchup.stopped_unix, $secondFirst.stopped_unix) -and
    $minoritySent.observed_unix -lt $secondRestore.started_unix) 'Minority marker did not run with only two validators'
foreach ($role in @('A1','C1')) {
    $start = Checked-Run "$role-hold-start" 'query' $role
    $end = Checked-Run "$role-hold-end" 'query' $role
    Assert-Test ((Query-Started $start) -ge $minoritySent.observed_unix -and
        $end.observed_unix -ge ($start.observed_unix + 12) -and
        $end.observed_unix -lt $secondRestore.started_unix -and $lateRestore.started_unix -ge $secondRestore.started_unix) "Minority hold is too short: $role"
    foreach ($point in @($start,$end)) {
        Assert-Head $healthy.status $point.status
        Assert-Test ($point.nonce -eq 41 -and $point.receipts.Count -eq 2) "Missing minority markers: $role"
        for ($i = 0; $i -lt 2; $i++) {
            $receipt = $point.receipts[$i]
            Assert-Test ($receipt.hash -ceq $minoritySent.marker_hashes[$i] -and $receipt.state -cin @('Received','NotRecent')) "Minority published or rejected a marker: $role"
        }
    }
}
$offered = Checked-Run 'A1-hold-start' 'query' 'A1'
Assert-Test (@($offered.receipts | Where-Object state -CEQ 'Received').Count -eq 2) 'A1 did not retain both marker inputs during the minority hold'
$restored = Checked-Run 'A1-restored' 'query' 'A1'
Confirm-FinalReceipts $restored $minoritySent
Assert-Test ($restored.observed_unix -gt $secondRestore.started_unix -and $restored.observed_unix -lt $lateRestore.started_unix -and
    $restored.status.height -gt $healthy.status.height) 'The third validator did not restore finality before D1 returned'

foreach ($role in $roles) {
    $final = Checked-Run "$role-final" 'query' $role
    $lastLaunch = switch ($role) {
        'A1' { $aLaunch }
        'A2' { $secondRestore }
        'C1' { $cLaunch }
        'D1' { $lateRestore }
    }
    Assert-Test ($final.observed_unix -ge $lastLaunch.started_unix -and
        $final.observed_unix -le $lastLaunch.stopped_unix) "Final query fell outside the owned launch: $role"
    Assert-Head $restored.status $final.status
    Confirm-FinalReceipts $final $minoritySent
    Assert-Test ((Json $restored.receipts) -ceq (Json $final.receipts)) "Final receipts differ on $role"
    $persisted = Checked-Run "$role-persisted" 'archive' $role
    Assert-Test ($persisted.observed_unix -ge $lastLaunch.stopped_unix) "Final archive predates stop: $role"
    Assert-Archive $persisted.archive $final.status
}
[ordered] @{
    result = 'PASS'
    plan_sha256 = $PlanSha256
    chain_id = $plan.chain_id
    final_height = $restored.status.height
    block_hash = Hex-Bytes $restored.status.block_hash
    state_root = Hex-Bytes $restored.status.state_root
    quorum_weight = 3
    durable_roles = $roles
    retained_outbox_sha256 = $old.outbox_sha256
} | ConvertTo-Json
