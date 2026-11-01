param([Parameter(Mandatory)][string]$Report,[Parameter(Mandatory)][string]$Output)
$ErrorActionPreference = 'Stop'
function Measure-UnionNs($Intervals) {
    $ordered = @($Intervals | Sort-Object start_ns,end_ns)
    if (!$ordered.Count) { return 0L }
    $start = [long]$ordered[0].start_ns
    $end = [long]$ordered[0].end_ns
    $total = 0L
    foreach ($v in ($ordered | Select-Object -Skip 1)) {
        if ([long]$v.start_ns -gt $end) {
            $total += $end-$start
            $start = [long]$v.start_ns
        }
        $end = [Math]::Max($end,[long]$v.end_ns)
    }
    return $total+$end-$start
}
$r = Get-Content -Raw -LiteralPath $Report | ConvertFrom-Json
$nodes = foreach ($n in $r.final_local_progress.nodes) {
    $p = $n.snapshot.stage_profile
    $events = @($p.control_work.events) + @($p.critical_path.events | Where-Object duration_ns)
    $events = @($events | Group-Object { "$($_.event):$($_.height):$($_.elapsed_ns):$($_.duration_ns):$($_.body):$($_.fragment)" } | ForEach-Object { $_.Group[0] })
    $acks = @($p.critical_path.events | Where-Object event -eq decision_durable_ack | Sort-Object height)
    $slow = for ($i=1; $i -lt $acks.Count; $i++) {
        [pscustomobject]@{height=$acks[$i].height;start_ns=$acks[$i-1].elapsed_ns;end_ns=$acks[$i].elapsed_ns;ms=($acks[$i].elapsed_ns-$acks[$i-1].elapsed_ns)/1e6}
    }
    $slow = $slow | Sort-Object ms -Descending | Select-Object -First 1
    $sync = @($events | Where-Object { $_.event -notin @('native_io_gap','native_io_period') })
    $gaps = @($events | Where-Object event -eq native_io_gap)
    $topGaps = foreach ($g in ($gaps | Sort-Object duration_ns -Descending | Select-Object -First 8)) {
        $begin = [long]$g.elapsed_ns - [long]$g.duration_ns
        $overlap = foreach ($e in $sync) {
            $start = [long]$e.elapsed_ns - [long]$e.duration_ns
            $ns = [Math]::Min([long]$e.elapsed_ns,[long]$g.elapsed_ns)-[Math]::Max($start,$begin)
            if ($ns -gt 0) { [pscustomobject]@{event=$e.event;height=$e.height;overlap_ms=$ns/1e6;duration_ms=$e.duration_ns/1e6;body=$e.body;fragment=$e.fragment;start_ns=[Math]::Max($start,$begin);end_ns=[Math]::Min([long]$e.elapsed_ns,[long]$g.elapsed_ns)} }
        }
        [pscustomobject]@{height=$g.height;start_ns=$begin;end_ns=$g.elapsed_ns;gap_ms=$g.duration_ns/1e6;observed_sync_overlap_ms=(Measure-UnionNs @($overlap))/1e6;sync=@($overlap)}
    }
    $windows = foreach ($w in @(
        [pscustomobject]@{name='first_height';height=1;start_ns=0;end_ns=$acks[0].elapsed_ns},
        [pscustomobject]@{name='slowest_ack_interval';height=$slow.height;start_ns=$slow.start_ns;end_ns=$slow.end_ns}
    )) {
        $intervals = @($events | Where-Object { $_.elapsed_ns -gt $w.start_ns -and ($_.elapsed_ns-$_.duration_ns) -lt $w.end_ns })
        $clipped = @($intervals | ForEach-Object {
            [pscustomobject]@{event=$_.event;start_ns=[Math]::Max([long]$w.start_ns,[long]$_.elapsed_ns-[long]$_.duration_ns);end_ns=[Math]::Min([long]$w.end_ns,[long]$_.elapsed_ns)}
        })
        $syncWindow = @($clipped | Where-Object event -notin @('native_io_gap','native_io_period'))
        $gapWindow = @($clipped | Where-Object event -eq native_io_gap)
        $maxGap = $gapWindow | ForEach-Object { ($_.end_ns-$_.start_ns)/1e6 } | Measure-Object -Maximum
        [pscustomobject]@{name=$w.name;height=$w.height;start_ns=$w.start_ns;end_ns=$w.end_ns;ms=($w.end_ns-$w.start_ns)/1e6;
            scope='first_height starts at process profile origin, not transaction submission; observed unions exclude evicted/subthreshold events; gap and sync overlap, do not add';
            observed_sync_union_ms=(Measure-UnionNs $syncWindow)/1e6;observed_gap_union_ms=(Measure-UnionNs $gapWindow)/1e6;observed_max_gap_ms=$maxGap.Maximum;
            critical=@($p.critical_path.events | Where-Object { $_.elapsed_ns -ge $w.start_ns -and $_.elapsed_ns -le $w.end_ns });
            control=$intervals}
    }
    $phases = foreach ($field in $p.phases.PSObject.Properties) {
        if ($field.Name -match 'native_|body_') {
            [pscustomobject]@{phase=$field.Name;calls=$field.Value.calls;sum_ms=$field.Value.wall_ns/1e6;max_ms=$field.Value.max_wall_ns/1e6}
        }
    }
    [pscustomobject]@{node=$n.node_index;matched_final_head=$n.matched_final_head;phases=@($phases);control_evicted=$p.control_work.evicted;
        critical_dropped=$p.critical_path.dropped;slowest=$slow;top_gaps=@($topGaps);windows=@($windows)}
}
[pscustomobject]@{report=(Resolve-Path -LiteralPath $Report).Path;passed=$r.passed;transactions=$r.transactions;signature_backend=$r.signature_backend;
    successful_tps=$r.successful_tps;elapsed_seconds=$r.elapsed_seconds;latency_us=$r.latency_us;cold_recovery_pass=$r.cold_recovery_pass;
    full_fee_state_audited=$r.full_fee_state_audited;input_hash=$r.signed_workload_sha256;binary_sha256=$r.binary_sha256;aoem_sha256=$r.aoem_sha256;
    scope='inclusive process-local work; overlapping phases not additive; bounded events omit sub-1ms work and evicted intervals; body != fragment';nodes=@($nodes)} |
    ConvertTo-Json -Depth 25 | Set-Content -LiteralPath $Output -Encoding utf8NoBOM
Get-Item -LiteralPath $Output | Select-Object FullName,Length
