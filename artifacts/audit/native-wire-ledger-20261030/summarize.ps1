param([Parameter(Mandatory)][string]$Report,[Parameter(Mandatory)][string]$Output)
$ErrorActionPreference = 'Stop'
$reportPath = (Resolve-Path -LiteralPath $Report).Path
$r = Get-Content -Raw -LiteralPath $reportPath | ConvertFrom-Json
if ($r.final_local_progress.status -ne 'COMPLETE') {
    # Preserve the exact status rather than claiming complete attribution.
    Write-Host "Progress capture status: $($r.final_local_progress.status)"
}
$rows = @(foreach ($n in $r.final_local_progress.nodes) {
    foreach ($s in $n.snapshot.native_io_costs.traffic.streams) {
        $name = if ($s.stream -eq 0) { 'RAW' } else {
            [Text.Encoding]::ASCII.GetString([byte[]]([BitConverter]::GetBytes([uint64]$s.stream))[7..0])
        }
        $events = [ordered]@{}
        foreach ($p in $s.events.PSObject.Properties) {
            $events[$p.Name] = [pscustomobject][ordered]@{
                packets=($p.Value | Measure-Object packets -Sum).Sum
                bytes=($p.Value | Measure-Object bytes -Sum).Sum
                by_kind=$p.Value
            }
        }
        [ordered]@{node=$n.node_index;stream=$name;events=$events}
    }
})
$totals = @(foreach ($g in ($rows | Group-Object { $_.stream })) {
    $events = [ordered]@{}
    foreach ($event in $g.Group[0].events.Keys) {
        $values = @($g.Group | ForEach-Object { $_.events[$event] })
        $events[$event] = [ordered]@{
            packets=($values | Measure-Object packets -Sum).Sum
            bytes=($values | Measure-Object bytes -Sum).Sum
            bytes_per_final_transaction=(($values | Measure-Object bytes -Sum).Sum / $r.transactions)
        }
    }
    [ordered]@{stream=$g.Name;events=$events}
})
$bodies = @(foreach ($n in $r.final_local_progress.nodes) {
    foreach ($b in $n.snapshot.native_delivery_traffic.bodies) {
        [ordered]@{node=$n.node_index;peer=$b.peer;height=$b.height;payload=($b.payload -join ',');
            fragment=($b.fragment -join ',');purposes=$b.purposes;state=$b.state;
            object_bytes=$b.object_bytes;wire=$b.wire}
    }
})
$repeated = @($bodies | Group-Object { "$($_.node):$($_.peer):$($_.payload)" } |
    Where-Object Count -gt 1 | ForEach-Object { [ordered]@{attempts=$_.Count;records=$_.Group} })
$clientDelta = @(foreach ($s in $r.client_io_after.traffic.streams) {
    $previous = $r.client_io_before.traffic.streams | Where-Object stream -eq $s.stream
    $name = if ($s.stream -eq 0) { 'RAW' } else {
        [Text.Encoding]::ASCII.GetString([byte[]]([BitConverter]::GetBytes([uint64]$s.stream))[7..0])
    }
    $events = [ordered]@{}
    foreach ($p in $s.events.PSObject.Properties) {
        $byKind = @(foreach ($kind in 0..3) {
            $old = if ($previous) { $previous.events.($p.Name)[$kind] } else { $null }
            [pscustomobject]@{packets=[long]$p.Value[$kind].packets-[long]$old.packets;
                bytes=[long]$p.Value[$kind].bytes-[long]$old.bytes}
        })
        $events[$p.Name] = [ordered]@{packets=($byKind | Measure-Object packets -Sum).Sum;
            bytes=($byKind | Measure-Object bytes -Sum).Sum;by_kind=$byKind}
    }
    [ordered]@{stream=$name;events=$events}
})
[ordered]@{
    report=$reportPath;report_sha256=(Get-FileHash -LiteralPath $reportPath -Algorithm SHA256).Hash
    scope='Node lifetime snapshots after load and audit queries; not exact simultaneous load-boundary deltas. Raw UDP payload excludes IP/UDP headers. Client is excluded unless client_io fields are present. Repeated equal body attempts are not proof of unnecessary delivery.'
    node_hash=$r.binary_sha256;aoem_hash=$r.aoem_sha256;input_hash=$r.signed_workload_sha256
    transactions=$r.transactions;tps=$r.successful_tps;latency_us=$r.latency_us;passed=$r.passed
    wire_frame_rows=$r.wire_frame_rows;heights=@($r.before.height)
    capture_status=$r.final_local_progress.status;nodes=$rows;totals=$totals
    client_before=$r.client_io_before;client_after=$r.client_io_after
    client_delta=$clientDelta
    client_delta_scope='Same client poll thread immediately before/after the timed drive; all four client sessions share this thread. Do not combine as a synchronized window with node lifetime snapshots.'
    per_node_costs=@(foreach ($n in $r.final_local_progress.nodes) {
        [ordered]@{node=$n.node_index;phases=@(foreach ($p in $n.snapshot.native_io_costs.phases.PSObject.Properties) {
            [ordered]@{phase=$p.Name;calls=$p.Value.calls;samples=$p.Value.samples;
                sampled_mean_ns=if ($p.Value.samples) { $p.Value.sampled_ns/$p.Value.samples } else { $null }}
        })}
    })
    body_totals=@($r.final_local_progress.nodes | ForEach-Object { [ordered]@{
        node=$_.node_index;omitted=$_.snapshot.native_delivery_traffic.omitted_bodies;
        totals=$_.snapshot.native_delivery_traffic.totals} })
    repeated_body_attempts=$repeated
} | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $Output -Encoding utf8NoBOM
