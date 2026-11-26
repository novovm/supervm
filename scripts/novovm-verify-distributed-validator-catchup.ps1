#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $PlanFile,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string] $PlanSha256,
    [Parameter(Mandatory)][string] $EvidenceDirectory,
    [switch] $PlanOnly,
    [ValidatePattern('^[a-f0-9]{64}$')][string] $GenesisSha256
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
$roles = @('A1','A2','C1','D1')
$members = @{}
$roleWeights = [ordered]@{}
$idDomain = [Text.Encoding]::ASCII.GetBytes("novovm-round-bft/v2/mldsa65/validator-id`0")
Assert-Test ($plan.slots.Count -eq 4 -and $genesis.genesis.validators.Count -eq 4) 'Four pinned validator roles required'
for ($i = 0; $i -lt $roles.Count; $i++) {
    $slot = $plan.slots[$i]
    $validator = $genesis.genesis.validators[$i]
    $field = $slot.PSObject.Properties['weight']
    $expected = if ($null -eq $field) { 1 } else { $field.Value }
    Assert-Test ($slot.role -ceq $roles[$i] -and ($null -eq $field -or $expected -is [long])) 'Invalid plan role or weight'
    Assert-Test ($validator.weight -is [long] -and $validator.weight -in @(1,2,3) -and $validator.weight -eq $expected -and
        $validator.public_key.Count -eq 1952 -and (Hex-Bytes $slot.validator) -ceq (Hex-Bytes $validator.public_key) -and
        $validator.transport_public_key.Count -eq 1952 -and
        (Hex-Bytes $slot.transport) -ceq (Hex-Bytes $validator.transport_public_key)) 'Plan differs from genesis validator'
    $id = Hex-Bytes ([Security.Cryptography.SHA256]::HashData([byte[]]($idDomain + $validator.public_key)))
    Assert-Test (!$members.ContainsKey($id)) 'Duplicate validator key'
    $members[$id] = $slot.role
    $roleWeights[$slot.role] = $validator.weight
}
$layout = $roleWeights.Values -join ','
Assert-Test ($layout -in @('1,1,1,1','1,3,2,1')) 'Unsupported weight schedule'
$unequal = $layout -eq '1,3,2,1'
$totalWeight = [long]($roleWeights.Values | Measure-Object -Sum).Sum
$quorumWeight = [long]([Math]::Floor(2 * $totalWeight / 3) + 1)
$weakWeight = $roleWeights.A1 + $roleWeights.C1
$restoredWeight = $roleWeights.A2 + $roleWeights.C1
Assert-Test ($weakWeight -lt $quorumWeight -and (!$unequal -or $restoredWeight -ge $quorumWeight)) 'Invalid isolation schedule'
if ($GenesisSha256) { Assert-Test ($GenesisSha256 -ceq $genesisHash) 'Genesis file digest differs' }
if ($PlanOnly) {
    Assert-Test ([bool]$GenesisSha256) 'Supply the independent genesis file digest for plan review'
    [ordered]@{result='PLAN_ONLY'; plan_sha256=$PlanSha256; genesis_sha256=$genesisHash; genesis=$domain
        weights=$roleWeights; total_weight=$totalWeight; quorum_weight=$quorumWeight
        a1_c1_weight=$weakWeight; a2_c1_weight=$restoredWeight} | ConvertTo-Json
    return
}

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
function Assert-Archive($Record, $Status, [string[]]$ExpectedRoles = @()) {
    $signerRoles = @(foreach ($id in $Record.signers) {
        Assert-Test ($id -cmatch '^[a-f0-9]{64}$' -and $members.ContainsKey($id)) 'Unknown QC signer'
        $members[$id]
    })
    Assert-Test ($signerRoles.Count -gt 0 -and @($signerRoles | Sort-Object -Unique).Count -eq $signerRoles.Count) 'Duplicate or empty QC signers'
    $weight = ($signerRoles | ForEach-Object { $roleWeights[$_] } | Measure-Object -Sum).Sum
    if ($ExpectedRoles.Count) {
        Assert-Test ((($ExpectedRoles | Sort-Object) -join ',') -ceq (($signerRoles | Sort-Object) -join ',')) 'QC signers differ from the online pair'
    }
    Assert-Test ($Record.height -eq $Status.height -and $Record.block_hash -ceq (Hex-Bytes $Status.block_hash) -and
        $Record.state_root -ceq (Hex-Bytes $Status.state_root) -and
        $Record.receipt_batch_commitment -ceq (Hex-Bytes $Status.receipt_batch_commitment) -and
        $Record.quorum_weight -is [long] -and $Record.quorum_weight -eq $quorumWeight -and
        $Record.signed_weight -is [long] -and $Record.signed_weight -eq $weight -and
        $weight -ge $quorumWeight -and $Record.signatures -is [long] -and $signerRoles.Count -eq $Record.signatures -and
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
$survivor = if ($unequal) { 'C1' } else { 'A1' }
if ($unequal) { $aRestore = Checked-Run 'A1-restore' 'existing' 'A1' }
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
    $target = if ($_ -eq 41) { $survivor } else { 'A1' }
    $sent = Checked-Run "submit-$_" 'submit' $target
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
    if ($unequal) {
        Assert-Test ((Query-Started $start) -ge [Math]::Max($aLaunch.started_unix,$cLaunch.started_unix) -and
            $end.observed_unix -le $aLaunch.stopped_unix -and $end.observed_unix -le $cLaunch.stopped_unix -and
            $aLaunch.stopped_unix -lt $secondRestore.started_unix) 'Weak pair and restored pair overlapped or were not online'
    }
    foreach ($point in @($start,$end)) {
        Assert-Head $healthy.status $point.status
        Assert-Test ($point.nonce -eq 41 -and $point.receipts.Count -eq 2) "Missing minority markers: $role"
        for ($i = 0; $i -lt 2; $i++) {
            $receipt = $point.receipts[$i]
            Assert-Test ($receipt.hash -ceq $minoritySent.marker_hashes[$i] -and $receipt.state -cin @('Received','NotRecent')) "Minority published or rejected a marker: $role"
        }
    }
}
$offered = Checked-Run "$survivor-hold-start" 'query' $survivor
Assert-Test (@($offered.receipts | Where-Object state -CEQ 'Received').Count -eq 2) 'Surviving node did not retain both original marker inputs'
$restored = Checked-Run "$survivor-restored" 'query' $survivor
Confirm-FinalReceipts $restored $minoritySent
Assert-Test ($restored.observed_unix -gt $secondRestore.started_unix -and $restored.observed_unix -lt $lateRestore.started_unix -and
    $restored.status.height -gt $healthy.status.height) 'Restored voting weight did not finalize before D1 returned'
if ($unequal) {
    Assert-Test ((Query-Started $restored) -ge $secondRestore.started_unix -and
        $restored.observed_unix -le [Math]::Min($secondRestore.stopped_unix,$cLaunch.stopped_unix) -and
        $restored.observed_unix -lt $aRestore.started_unix) 'Restored pair did not finalize alone'
    foreach ($receipt in $restored.receipts) {
        Assert-Test ($receipt.state.Finalized.height -eq $restored.status.height) 'Wrong height for restored-pair QC'
    }
}

foreach ($role in $roles) {
    $final = Checked-Run "$role-final" 'query' $role
    $lastLaunch = switch ($role) {
        'A1' { if ($unequal) { $aRestore } else { $aLaunch } }
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
    if ($unequal) { Assert-Archive $persisted.archive $final.status @('C1','A2') }
    else { Assert-Archive $persisted.archive $final.status }
}
$result = [ordered] @{
    result = 'PASS'
    plan_sha256 = $PlanSha256
    chain_id = $plan.chain_id
    final_height = $restored.status.height
    block_hash = Hex-Bytes $restored.status.block_hash
    state_root = Hex-Bytes $restored.status.state_root
    quorum_weight = $quorumWeight
    durable_roles = $roles
    retained_outbox_sha256 = $old.outbox_sha256
}
if ($unequal) {
    $result.total_weight = $totalWeight
    $result.insufficient_pair = @{ roles=@('A1','C1'); online_weight=$weakWeight; height=$healthy.status.height }
    $result.finalizing_pair = @{ roles=@('A2','C1'); signed_weight=$restoredWeight; height=$restored.status.height }
}
$result | ConvertTo-Json -Depth 4
