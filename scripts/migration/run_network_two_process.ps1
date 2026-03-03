param(
    [string]$RepoRoot = "D:\WorksArea\SUPERVM",
    [string]$OutputDir = "D:\WorksArea\SUPERVM\artifacts\migration\network-two-process",
    [int]$TimeoutSeconds = 15,
    [ValidateRange(1, 50)]
    [int]$Rounds = 1,
    [ValidateRange(2, 12)]
    [int]$NodeCount = 2,
    [ValidateSet("mesh", "pair_matrix")]
    [string]$ProbeMode = "mesh",
    [ValidateSet("none", "class_mismatch", "hash_mismatch", "codec_corrupt")]
    [string]$TamperBlockWireMode = "none"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-Cargo {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()

    if ($proc.ExitCode -ne 0) {
        throw "cargo $($CargoArgs -join ' ') failed in $WorkDir`n$stdout`n$stderr"
    }
}

function Start-NetworkProbeProcess {
    param(
        [string]$ExePath,
        [int]$NodeId,
        [string]$ListenAddr,
        [string]$PeerAddr,
        [string]$PeerSpec,
        [string]$TamperBlockWireMode
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $ExePath
    $psi.WorkingDirectory = (Split-Path -Path $ExePath -Parent)
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Environment["NOVOVM_NODE_MODE"] = "network_probe"
    $psi.Environment["NOVOVM_NET_NODE_ID"] = "$NodeId"
    $psi.Environment["NOVOVM_NET_LISTEN"] = $ListenAddr
    $psi.Environment["NOVOVM_NETWORK_STRICT"] = "1"
    $psi.Environment["NOVOVM_NET_TIMEOUT_MS"] = "2500"
    $psi.Environment["NOVOVM_NET_MIN_RUNTIME_MS"] = "800"
    if ($PeerAddr) {
        $psi.Environment["NOVOVM_NET_PEER"] = $PeerAddr
    }
    if ($PeerSpec) {
        $psi.Environment["NOVOVM_NET_PEERS"] = $PeerSpec
    }
    if ($TamperBlockWireMode -and $TamperBlockWireMode -ne "none") {
        $psi.Environment["NOVOVM_NET_TAMPER_BLOCK_WIRE"] = $TamperBlockWireMode
    }

    return [System.Diagnostics.Process]::Start($psi)
}

function Parse-NetworkProbeLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^network_probe_out:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }

    $m = [regex]::Match(
        $line,
        "^network_probe_out:\s+transport=(?<transport>\w+)\s+node=(?<node>\d+)\s+listen=(?<listen>\S+)\s+peer=(?<peer>\S+)\s+sent=(?<sent>\d+)\s+received=(?<received>\d+)\s+discovery=(?<discovery>true|false)\s+gossip=(?<gossip>true|false)\s+sync=(?<sync>true|false)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }

    return [ordered]@{
        parse_ok = $true
        transport = $m.Groups["transport"].Value
        node = [int]$m.Groups["node"].Value
        listen = $m.Groups["listen"].Value
        peer = $m.Groups["peer"].Value
        sent = [int]$m.Groups["sent"].Value
        received = [int]$m.Groups["received"].Value
        discovery = [bool]::Parse($m.Groups["discovery"].Value)
        gossip = [bool]::Parse($m.Groups["gossip"].Value)
        sync = [bool]::Parse($m.Groups["sync"].Value)
        raw = $line
    }
}

function Parse-NetworkProbeGraphLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^network_probe_graph:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }

    $m = [regex]::Match(
        $line,
        "^network_probe_graph:\s+node=(?<node>\d+)\s+peers=(?<peers>\d+)\s+discovery_ok=(?<d_ok>\d+)\/(?<d_total>\d+)\s+gossip_ok=(?<g_ok>\d+)\/(?<g_total>\d+)\s+sync_ok=(?<s_ok>\d+)\/(?<s_total>\d+)\s+edge_ok=(?<e_ok>\d+)\/(?<e_total>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }

    return [ordered]@{
        parse_ok = $true
        node = [int]$m.Groups["node"].Value
        peers = [int]$m.Groups["peers"].Value
        discovery_ok = [int]$m.Groups["d_ok"].Value
        discovery_total = [int]$m.Groups["d_total"].Value
        gossip_ok = [int]$m.Groups["g_ok"].Value
        gossip_total = [int]$m.Groups["g_total"].Value
        sync_ok = [int]$m.Groups["s_ok"].Value
        sync_total = [int]$m.Groups["s_total"].Value
        edge_ok = [int]$m.Groups["e_ok"].Value
        edge_total = [int]$m.Groups["e_total"].Value
        raw = $line
    }
}

function Parse-IdList {
    param([string]$Raw)
    if (-not $Raw -or $Raw -eq "-") {
        return @()
    }
    return @($Raw -split "," | Where-Object { $_ -ne "" } | ForEach-Object { [int]$_ })
}

function Parse-NetworkProbeEdgesLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^network_probe_edges:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }

    $m = [regex]::Match(
        $line,
        "^network_probe_edges:\s+node=(?<node>\d+)\s+up=(?<up>[-0-9,]+)\s+down=(?<down>[-0-9,]+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }

    return [ordered]@{
        parse_ok = $true
        node = [int]$m.Groups["node"].Value
        up = Parse-IdList -Raw $m.Groups["up"].Value
        down = Parse-IdList -Raw $m.Groups["down"].Value
        raw = $line
    }
}

function Parse-NetworkBlockWireLine {
    param([string]$Text)

    $line = ($Text -split "`r?`n" | Where-Object { $_ -match "^network_block_wire:" } | Select-Object -Last 1)
    if (-not $line) {
        return $null
    }

    $m = [regex]::Match(
        $line,
        "^network_block_wire:\s+codec=(?<codec>\S+)\s+node=(?<node>\d+)\s+peers=(?<peers>\d+)\s+verified=(?<verified>\d+)\/(?<total>\d+)\s+expected_class=(?<class>\S+)\s+expected_hash=(?<hash>[0-9a-fA-F]+)\s+pass=(?<pass>true|false)\s+bytes_min=(?<bmin>\d+)\s+bytes_max=(?<bmax>\d+)$"
    )
    if (-not $m.Success) {
        return [ordered]@{
            parse_ok = $false
            raw = $line
        }
    }

    return [ordered]@{
        parse_ok = $true
        codec = $m.Groups["codec"].Value
        node = [int]$m.Groups["node"].Value
        peers = [int]$m.Groups["peers"].Value
        verified = [int]$m.Groups["verified"].Value
        total = [int]$m.Groups["total"].Value
        expected_class = $m.Groups["class"].Value
        expected_hash = $m.Groups["hash"].Value.ToLowerInvariant()
        pass = [bool]::Parse($m.Groups["pass"].Value)
        bytes_min = [int]$m.Groups["bmin"].Value
        bytes_max = [int]$m.Groups["bmax"].Value
        raw = $line
    }
}

function Wait-OrKill {
    param(
        [System.Diagnostics.Process]$Process,
        [int]$TimeoutMs
    )

    if (-not $Process.WaitForExit($TimeoutMs)) {
        try { $Process.Kill() } catch {}
        throw "process timed out after $TimeoutMs ms"
    }
}

function Test-ProbePass {
    param(
        $Parsed,
        $BlockWire
    )
    if (-not $Parsed -or -not $Parsed.parse_ok -or -not $BlockWire -or -not $BlockWire.parse_ok) {
        return $false
    }
    return (
        $Parsed.transport -eq "udp" -and
        $Parsed.sent -ge 3 -and
        $Parsed.received -ge 3 -and
        $Parsed.discovery -and
        $Parsed.gossip -and
        $Parsed.sync -and
        $BlockWire.pass -and
        $BlockWire.verified -eq $BlockWire.total -and
        $BlockWire.total -ge 1
    )
}

function Invoke-PairProbe {
    param(
        [string]$ExePath,
        [int]$LeftNodeId,
        [int]$RightNodeId,
        [string]$LeftListen,
        [string]$RightListen,
        [int]$TimeoutSeconds,
        [string]$TamperBlockWireMode
    )

    $procA = $null
    $procB = $null
    try {
        $leftPeerSpec = "$RightNodeId@$RightListen"
        $rightPeerSpec = "$LeftNodeId@$LeftListen"
        $procA = Start-NetworkProbeProcess -ExePath $ExePath -NodeId $LeftNodeId -ListenAddr $LeftListen -PeerAddr $RightListen -PeerSpec $leftPeerSpec -TamperBlockWireMode $TamperBlockWireMode
        Start-Sleep -Milliseconds 80
        $procB = Start-NetworkProbeProcess -ExePath $ExePath -NodeId $RightNodeId -ListenAddr $RightListen -PeerAddr $LeftListen -PeerSpec $rightPeerSpec -TamperBlockWireMode $TamperBlockWireMode

        Wait-OrKill -Process $procA -TimeoutMs ($TimeoutSeconds * 1000)
        Wait-OrKill -Process $procB -TimeoutMs ($TimeoutSeconds * 1000)
    } finally {
        if ($procA -and -not $procA.HasExited) { try { $procA.Kill() } catch {} }
        if ($procB -and -not $procB.HasExited) { try { $procB.Kill() } catch {} }
    }

    $aStdout = $procA.StandardOutput.ReadToEnd()
    $aStderr = $procA.StandardError.ReadToEnd()
    $bStdout = $procB.StandardOutput.ReadToEnd()
    $bStderr = $procB.StandardError.ReadToEnd()

    $aParsed = Parse-NetworkProbeLine -Text ($aStdout + $aStderr)
    $bParsed = Parse-NetworkProbeLine -Text ($bStdout + $bStderr)
    $aBlockWire = Parse-NetworkBlockWireLine -Text ($aStdout + $aStderr)
    $bBlockWire = Parse-NetworkBlockWireLine -Text ($bStdout + $bStderr)
    $aPass = Test-ProbePass -Parsed $aParsed -BlockWire $aBlockWire
    $bPass = Test-ProbePass -Parsed $bParsed -BlockWire $bBlockWire

    $pairPass = (
        $procA.ExitCode -eq 0 -and
        $procB.ExitCode -eq 0 -and
        $aPass -and
        $bPass
    )

    return [ordered]@{
        pair = "$LeftNodeId-$RightNodeId"
        pass = $pairPass
        left_node = $LeftNodeId
        right_node = $RightNodeId
        left_exit_code = $procA.ExitCode
        right_exit_code = $procB.ExitCode
        left_received = if ($aParsed -and $aParsed.parse_ok) { [int]$aParsed.received } else { 0 }
        right_received = if ($bParsed -and $bParsed.parse_ok) { [int]$bParsed.received } else { 0 }
        left_block_wire_pass = if ($aBlockWire -and $aBlockWire.parse_ok) { [bool]$aBlockWire.pass } else { $false }
        right_block_wire_pass = if ($bBlockWire -and $bBlockWire.parse_ok) { [bool]$bBlockWire.pass } else { $false }
        left = [ordered]@{
            node_id = $LeftNodeId
            exit_code = $procA.ExitCode
            pass = $aPass
            parsed = $aParsed
            block_wire = $aBlockWire
            stdout = $aStdout
            stderr = $aStderr
        }
        right = [ordered]@{
            node_id = $RightNodeId
            exit_code = $procB.ExitCode
            pass = $bPass
            parsed = $bParsed
            block_wire = $bBlockWire
            stdout = $bStdout
            stderr = $bStderr
        }
        listen_left = $LeftListen
        listen_right = $RightListen
    }
}

function Get-NodeResult {
    param(
        [System.Diagnostics.Process]$Process,
        [int]$NodeId
    )

    $stdout = $Process.StandardOutput.ReadToEnd()
    $stderr = $Process.StandardError.ReadToEnd()
    $probe = Parse-NetworkProbeLine -Text ($stdout + $stderr)
    $blockWire = Parse-NetworkBlockWireLine -Text ($stdout + $stderr)
    $graph = Parse-NetworkProbeGraphLine -Text ($stdout + $stderr)
    $edges = Parse-NetworkProbeEdgesLine -Text ($stdout + $stderr)
    $probePass = Test-ProbePass -Parsed $probe -BlockWire $blockWire
    $graphPass = $false
    if ($graph -and $graph.parse_ok) {
        $graphPass = (
            $graph.discovery_ok -eq $graph.discovery_total -and
            $graph.gossip_ok -eq $graph.gossip_total -and
            $graph.sync_ok -eq $graph.sync_total -and
            $graph.edge_ok -eq $graph.edge_total
        )
    }
    $blockWirePass = $false
    if ($blockWire -and $blockWire.parse_ok) {
        $blockWirePass = (
            $blockWire.pass -and
            $blockWire.verified -eq $blockWire.total -and
            $blockWire.total -ge 1
        )
    }
    $edgePass = $false
    if ($edges -and $edges.parse_ok) {
        $edgePass = ((@($edges.down)).Count -eq 0)
    }
    $nodePass = (
        $Process.ExitCode -eq 0 -and
        $probePass -and
        $blockWirePass -and
        $graphPass -and
        $edgePass
    )

    return [ordered]@{
        node_id = $NodeId
        exit_code = $Process.ExitCode
        pass = $nodePass
        probe_pass = $probePass
        block_wire_pass = $blockWirePass
        graph_pass = $graphPass
        edge_pass = $edgePass
        parsed = $probe
        block_wire = $blockWire
        graph = $graph
        edges = $edges
        stdout = $stdout
        stderr = $stderr
    }
}

function Build-MeshPairSummary {
    param(
        [object[]]$NodeResults,
        [int]$NodeCount
    )

    $upDirected = @{}
    foreach ($n in $NodeResults) {
        if ($n.edges -and $n.edges.parse_ok) {
            foreach ($to in @($n.edges.up)) {
                $key = "$($n.node_id)->$to"
                $upDirected[$key] = $true
            }
        }
    }

    $pairResults = @()
    $passedPairs = 0
    for ($i = 0; $i -lt $NodeCount; $i++) {
        for ($j = $i + 1; $j -lt $NodeCount; $j++) {
            $left = $NodeResults | Where-Object { $_.node_id -eq $i } | Select-Object -First 1
            $right = $NodeResults | Where-Object { $_.node_id -eq $j } | Select-Object -First 1
            $pairPass = $upDirected.ContainsKey("$i->$j") -and $upDirected.ContainsKey("$j->$i")
            if ($pairPass) {
                $passedPairs++
            }
            $pairResults += [pscustomobject]([ordered]@{
                pair = "$i-$j"
                pass = $pairPass
                left_node = $i
                right_node = $j
                left_exit_code = if ($left) { $left.exit_code } else { -1 }
                right_exit_code = if ($right) { $right.exit_code } else { -1 }
                left_received = if ($left -and $left.parsed -and $left.parsed.parse_ok) { [int]$left.parsed.received } else { 0 }
                right_received = if ($right -and $right.parsed -and $right.parsed.parse_ok) { [int]$right.parsed.received } else { 0 }
                left_block_wire_pass = if ($left -and $left.block_wire -and $left.block_wire.parse_ok) { [bool]$left.block_wire.pass } else { $false }
                right_block_wire_pass = if ($right -and $right.block_wire -and $right.block_wire.parse_ok) { [bool]$right.block_wire.pass } else { $false }
            })
        }
    }

    $totalPairs = [int](($NodeCount * ($NodeCount - 1)) / 2)
    $pairPassRatio = if ($totalPairs -gt 0) {
        [Math]::Round(($passedPairs / $totalPairs), 4)
    } else {
        0.0
    }
    $directedTotal = [int]($NodeCount * ($NodeCount - 1))
    $directedUp = $upDirected.Keys.Count
    $directedRatio = if ($directedTotal -gt 0) {
        [Math]::Round(($directedUp / $directedTotal), 4)
    } else {
        0.0
    }

    return [ordered]@{
        pair_results = $pairResults
        total_pairs = $totalPairs
        passed_pairs = $passedPairs
        pair_pass_ratio = $pairPassRatio
        directed_edges_total = $directedTotal
        directed_edges_up = $directedUp
        directed_edge_ratio = $directedRatio
    }
}

function Invoke-ProbeRound {
    param(
        [string]$ExePath,
        [string]$ProbeMode,
        [string]$TamperBlockWireMode,
        [int]$NodeCount,
        [int]$TimeoutSeconds,
        [int]$BasePort,
        [int]$Round
    )

    $pairResults = @()
    $nodeResults = @()
    $totalPairs = 0
    $passedPairs = 0
    $pairPassRatio = 0.0
    $directedEdgesTotal = 0
    $directedEdgesUp = 0
    $directedEdgeRatio = 0.0
    $blockWireSamples = @()
    $blockWireAvailable = $false
    $blockWirePass = $false
    $blockWireVerified = 0
    $blockWireTotal = 0

    if ($ProbeMode -eq "pair_matrix") {
        $pairIndex = 0
        for ($i = 0; $i -lt $NodeCount; $i++) {
            for ($j = $i + 1; $j -lt $NodeCount; $j++) {
                $portLeft = $BasePort + ($pairIndex * 4)
                $portRight = $portLeft + 1
                $listenLeft = "127.0.0.1:$portLeft"
                $listenRight = "127.0.0.1:$portRight"
                $pair = Invoke-PairProbe `
                    -ExePath $ExePath `
                    -LeftNodeId $i `
                    -RightNodeId $j `
                    -LeftListen $listenLeft `
                    -RightListen $listenRight `
                    -TimeoutSeconds $TimeoutSeconds `
                    -TamperBlockWireMode $TamperBlockWireMode
                $pairResults += [pscustomobject]$pair
                $pairIndex++
            }
        }
        foreach ($p in $pairResults) {
            if ($p.left -and $p.left.block_wire) { $blockWireSamples += $p.left.block_wire }
            if ($p.right -and $p.right.block_wire) { $blockWireSamples += $p.right.block_wire }
        }
        $totalPairs = $pairResults.Count
        $passedPairs = ($pairResults | Where-Object { $_.pass } | Measure-Object).Count
        $pairPassRatio = if ($totalPairs -gt 0) { [Math]::Round(($passedPairs / $totalPairs), 4) } else { 0.0 }
        $directedEdgesTotal = [int]($NodeCount * ($NodeCount - 1))
        $directedEdgesUp = $passedPairs * 2
        $directedEdgeRatio = if ($directedEdgesTotal -gt 0) { [Math]::Round(($directedEdgesUp / $directedEdgesTotal), 4) } else { 0.0 }

        $first = $pairResults | Select-Object -First 1
        if ($first) {
            $nodeResults += [pscustomobject]$first.left
            $nodeResults += [pscustomobject]$first.right
        }
    } else {
        $nodes = @()
        for ($i = 0; $i -lt $NodeCount; $i++) {
            $nodes += [pscustomobject]@{
                node_id = $i
                listen = "127.0.0.1:$($BasePort + $i)"
            }
        }

        $processSlots = @()
        try {
            foreach ($n in $nodes) {
                $peerSpecs = @()
                foreach ($p in $nodes) {
                    if ($p.node_id -ne $n.node_id) {
                        $peerSpecs += "$($p.node_id)@$($p.listen)"
                    }
                }
                $primaryPeer = if ($peerSpecs.Count -gt 0) {
                    ($peerSpecs[0] -split "@", 2)[1]
                } else {
                    ""
                }
                $proc = Start-NetworkProbeProcess `
                    -ExePath $ExePath `
                    -NodeId $n.node_id `
                    -ListenAddr $n.listen `
                    -PeerAddr $primaryPeer `
                    -PeerSpec ($peerSpecs -join ",") `
                    -TamperBlockWireMode $TamperBlockWireMode
                $processSlots += [pscustomobject]@{
                    node_id = $n.node_id
                    listen = $n.listen
                    process = $proc
                }
                Start-Sleep -Milliseconds 35
            }

            foreach ($slot in $processSlots) {
                Wait-OrKill -Process $slot.process -TimeoutMs ($TimeoutSeconds * 1000)
            }
        } finally {
            foreach ($slot in $processSlots) {
                if ($slot.process -and -not $slot.process.HasExited) {
                    try { $slot.process.Kill() } catch {}
                }
            }
        }

        foreach ($slot in $processSlots) {
            $nodeResults += [pscustomobject](Get-NodeResult -Process $slot.process -NodeId $slot.node_id)
        }

        $meshSummary = Build-MeshPairSummary -NodeResults $nodeResults -NodeCount $NodeCount
        $pairResults = @($meshSummary.pair_results)
        $totalPairs = [int]$meshSummary.total_pairs
        $passedPairs = [int]$meshSummary.passed_pairs
        $pairPassRatio = [double]$meshSummary.pair_pass_ratio
        $directedEdgesTotal = [int]$meshSummary.directed_edges_total
        $directedEdgesUp = [int]$meshSummary.directed_edges_up
        $directedEdgeRatio = [double]$meshSummary.directed_edge_ratio
        foreach ($n in $nodeResults) {
            if ($n.block_wire) {
                $blockWireSamples += $n.block_wire
            }
        }
    }

    $expectedBlockWireSamples = if ($ProbeMode -eq "mesh") {
        $NodeCount
    } else {
        [Math]::Max(1, $totalPairs * 2)
    }
    if ($blockWireSamples.Count -gt 0) {
        foreach ($bw in $blockWireSamples) {
            if ($bw.parse_ok) {
                $blockWireTotal += [int]$bw.total
                $blockWireVerified += [int]$bw.verified
            }
        }
        $parsedCount = (@($blockWireSamples | Where-Object { $_.parse_ok }) | Measure-Object).Count
        $passCount = (@($blockWireSamples | Where-Object { $_.parse_ok -and $_.pass -and $_.verified -eq $_.total -and $_.total -ge 1 }) | Measure-Object).Count
        $blockWireAvailable = ($parsedCount -eq $blockWireSamples.Count -and $blockWireSamples.Count -ge $expectedBlockWireSamples)
        $blockWirePass = ($blockWireAvailable -and $passCount -eq $blockWireSamples.Count)
    }

    $overallPass = ($totalPairs -gt 0 -and $passedPairs -eq $totalPairs)
    if ($ProbeMode -eq "mesh") {
        $allNodePass = (
            $nodeResults.Count -eq $NodeCount -and
            ((@($nodeResults | Where-Object { $_.pass }) | Measure-Object).Count -eq $NodeCount)
        )
        $overallPass = ($overallPass -and $allNodePass)
    }
    $overallPass = ($overallPass -and $blockWirePass)
    $firstNode = $nodeResults | Select-Object -First 1
    $secondNode = $nodeResults | Select-Object -Skip 1 -First 1

    return [ordered]@{
        round = $Round
        base_port = $BasePort
        mode = $ProbeMode
        tamper_block_wire_mode = $TamperBlockWireMode
        node_count = $NodeCount
        total_pairs = $totalPairs
        passed_pairs = $passedPairs
        pair_pass_ratio = $pairPassRatio
        directed_edges_total = $directedEdgesTotal
        directed_edges_up = $directedEdgesUp
        directed_edge_ratio = $directedEdgeRatio
        block_wire_available = $blockWireAvailable
        block_wire_pass = $blockWirePass
        block_wire_verified = $blockWireVerified
        block_wire_total = $blockWireTotal
        pass = $overallPass
        node_a = $firstNode
        node_b = $secondNode
        pair_results = $pairResults
        node_results = $nodeResults
    }
}

$nodeDir = Join-Path $RepoRoot "crates\novovm-node"
Invoke-Cargo -WorkDir $nodeDir -CargoArgs @("build")

$exePath = Join-Path $nodeDir "cargo-target\debug\novovm-node.exe"
if (-not (Test-Path $exePath)) {
    $fallback = Join-Path $RepoRoot "target\debug\novovm-node.exe"
    if (-not (Test-Path $fallback)) {
        throw "novovm-node executable not found: $exePath"
    }
    $exePath = $fallback
}

$roundResults = @()
for ($round = 1; $round -le $Rounds; $round++) {
    $basePort = Get-Random -Minimum 39000 -Maximum 43000
    $roundResult = Invoke-ProbeRound `
        -ExePath $exePath `
        -ProbeMode $ProbeMode `
        -TamperBlockWireMode $TamperBlockWireMode `
        -NodeCount $NodeCount `
        -TimeoutSeconds $TimeoutSeconds `
        -BasePort $basePort `
        -Round $round
    $roundResults += [pscustomobject]$roundResult
}

$totalPairs = 0
$passedPairs = 0
$directedEdgesTotal = 0
$directedEdgesUp = 0
$blockWireVerified = 0
$blockWireTotal = 0
$blockWireRoundsPassed = 0
$blockWireRoundsAvailable = 0
$roundsPassed = 0
foreach ($r in $roundResults) {
    $totalPairs += [int]$r.total_pairs
    $passedPairs += [int]$r.passed_pairs
    $directedEdgesTotal += [int]$r.directed_edges_total
    $directedEdgesUp += [int]$r.directed_edges_up
    $blockWireVerified += [int]$r.block_wire_verified
    $blockWireTotal += [int]$r.block_wire_total
    if ($r.block_wire_available) {
        $blockWireRoundsAvailable++
    }
    if ($r.block_wire_pass) {
        $blockWireRoundsPassed++
    }
    if ($r.pass) {
        $roundsPassed++
    }
}

$pairPassRatio = if ($totalPairs -gt 0) {
    [Math]::Round(($passedPairs / $totalPairs), 4)
} else {
    0.0
}
$directedEdgeRatio = if ($directedEdgesTotal -gt 0) {
    [Math]::Round(($directedEdgesUp / $directedEdgesTotal), 4)
} else {
    0.0
}
$roundPassRatio = if ($Rounds -gt 0) {
    [Math]::Round(($roundsPassed / $Rounds), 4)
} else {
    0.0
}
$blockWirePassRatio = if ($Rounds -gt 0) {
    [Math]::Round(($blockWireRoundsPassed / $Rounds), 4)
} else {
    0.0
}
$blockWireVerifiedRatio = if ($blockWireTotal -gt 0) {
    [Math]::Round(($blockWireVerified / $blockWireTotal), 4)
} else {
    0.0
}
$blockWireAvailable = ($blockWireRoundsAvailable -eq $Rounds -and $Rounds -gt 0)
$blockWirePass = ($blockWireRoundsPassed -eq $Rounds -and $Rounds -gt 0)
$overallPass = ($totalPairs -gt 0 -and $passedPairs -eq $totalPairs -and $blockWirePass)

$latestRound = $roundResults | Select-Object -Last 1
$firstNode = if ($latestRound) { $latestRound.node_a } else { $null }
$secondNode = if ($latestRound) { $latestRound.node_b } else { $null }
$pairResults = if ($latestRound) { @($latestRound.pair_results) } else { @() }
$nodeResults = if ($latestRound) { @($latestRound.node_results) } else { @() }

$result = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    mode = $ProbeMode
    tamper_block_wire_mode = $TamperBlockWireMode
    rounds = $Rounds
    rounds_passed = $roundsPassed
    round_pass_ratio = $roundPassRatio
    executable = $exePath
    node_count = $NodeCount
    total_pairs = $totalPairs
    passed_pairs = $passedPairs
    pair_pass_ratio = $pairPassRatio
    directed_edges_total = $directedEdgesTotal
    directed_edges_up = $directedEdgesUp
    directed_edge_ratio = $directedEdgeRatio
    block_wire_available = $blockWireAvailable
    block_wire_pass = $blockWirePass
    block_wire_rounds_passed = $blockWireRoundsPassed
    block_wire_pass_ratio = $blockWirePassRatio
    block_wire_verified = $blockWireVerified
    block_wire_total = $blockWireTotal
    block_wire_verified_ratio = $blockWireVerifiedRatio
    pass = $overallPass
    node_a = $firstNode
    node_b = $secondNode
    pair_results = $pairResults
    node_results = $nodeResults
    round_results = $roundResults
    notes = @(
        "probe mode: $ProbeMode",
        "tamper_block_wire_mode: $TamperBlockWireMode",
        "rounds: $Rounds (aggregate pair/edge stats across rounds)",
        "strict mode is enabled in probe processes",
        "pair stats use undirected edge closure; directed edge ratio is reported separately",
        "network_block_wire requires block_header_wire_v1 payload decode + consensus binding verification on UDP probe path",
        "node_a/node_b/pair_results/node_results expose latest round for compatibility"
    )
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$jsonPath = Join-Path $OutputDir "network-two-process.json"
$mdPath = Join-Path $OutputDir "network-two-process.md"

$result | ConvertTo-Json -Depth 10 | Set-Content -Path $jsonPath -Encoding UTF8

$md = @(
    "# Network Probe"
    ""
    "- generated_at_utc: $($result.generated_at_utc)"
    "- mode: $($result.mode)"
    "- tamper_block_wire_mode: $($result.tamper_block_wire_mode)"
    "- rounds: $($result.rounds)"
    "- rounds_passed: $($result.rounds_passed)"
    "- round_pass_ratio: $($result.round_pass_ratio)"
    "- pass: $($result.pass)"
    "- executable: $($result.executable)"
    "- node_count: $($result.node_count)"
    "- total_pairs: $($result.total_pairs)"
    "- passed_pairs: $($result.passed_pairs)"
    "- pair_pass_ratio: $($result.pair_pass_ratio)"
    "- directed_edges_up: $($result.directed_edges_up)"
    "- directed_edges_total: $($result.directed_edges_total)"
    "- directed_edge_ratio: $($result.directed_edge_ratio)"
    "- block_wire_available: $($result.block_wire_available)"
    "- block_wire_pass: $($result.block_wire_pass)"
    "- block_wire_rounds_passed: $($result.block_wire_rounds_passed)"
    "- block_wire_pass_ratio: $($result.block_wire_pass_ratio)"
    "- block_wire_verified: $($result.block_wire_verified)"
    "- block_wire_total: $($result.block_wire_total)"
    "- block_wire_verified_ratio: $($result.block_wire_verified_ratio)"
    ""
    "## Pair Results"
    ""
    "| pair | pass | left_exit | right_exit | left_recv | right_recv | left_block_wire | right_block_wire |"
    "|---|---|---:|---:|---:|---:|---|---|"
)

foreach ($p in $pairResults) {
    $md += "| $($p.pair) | $($p.pass) | $($p.left_exit_code) | $($p.right_exit_code) | $($p.left_received) | $($p.right_received) | $($p.left_block_wire_pass) | $($p.right_block_wire_pass) |"
}

$md += ""
$md += "## Notes"
$md += ""
foreach ($n in $result.notes) {
    $md += "- $n"
}

$md -join "`n" | Set-Content -Path $mdPath -Encoding UTF8

Write-Host "network probe generated:"
Write-Host "  $jsonPath"
Write-Host "  $mdPath"
