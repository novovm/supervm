# Diagnostic handoff only. Never extends a protocol/object/session deadline.
function Assert-RelayReadiness {
    param($Ready,$Live,[string]$HostId,[string]$Source,[string]$NodeSha256,[DateTime]$NowUtc=[DateTime]::UtcNow)
    if($Ready.status -cne ($HostId+'_RELAY_OBSERVER_ARMED') -or $Ready.source -cne $Source -or $Ready.node_sha256 -cne $NodeSha256 -or $Ready.read_only -ne $true){throw 'Wrong observer source/identity'}
    if($Ready.observer_version -notin @(2,3) -or $Live.observer_version -ne $Ready.observer_version -or $Live.active -ne $true -or $Live.host -cne $HostId -or $Live.observer_pid -ne $Ready.observer_pid -or $Live.node_pid -ne $Ready.pid){throw 'Fresh matching observer identity required'}
    $age=($NowUtc-([DateTime]$Ready.utc).ToUniversalTime()).TotalSeconds
    $heartbeatAge=($NowUtc-([DateTime]$Live.utc).ToUniversalTime()).TotalSeconds
    if($age -lt -60 -or $heartbeatAge -lt -60 -or $heartbeatAge -gt 600){throw 'Stale or future observer heartbeat'}
    $limit=[double]$Ready.maximum_wait_seconds
    if(![double]::IsFinite($limit) -or $limit -lt 1800 -or $limit -gt 7200){throw 'Observer wait outside bounded policy'}
    if($Ready.observer_version -eq 2){
        if($limit -ne 1800 -or $age -gt 1500){throw 'Legacy observer window too close to expiry'}
    }else{
        $elapsed=[double]$Live.elapsed_seconds
        if(![double]::IsFinite($elapsed) -or $elapsed -lt 0 -or $Live.maximum_wait_seconds -ne $limit){throw 'Invalid monotonic observer lifetime'}
        # Conservatively account for both reported monotonic age and transit delay.
        $used=[Math]::Max($age,$elapsed+[Math]::Max(0,$heartbeatAge))
        if($limit-$used -lt 120){throw 'Insufficient observer window for setup and delivery'}
    }
}
