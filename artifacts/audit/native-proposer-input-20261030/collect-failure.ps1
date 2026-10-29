param(
    [Parameter(Mandatory = $true)][string]$ReportPath,
    [Parameter(Mandatory = $true)][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$')][string]$Name
)
$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..'))
$gitRoot = & git -C $repository rev-parse --show-toplevel
if ($LASTEXITCODE -ne 0 -or [IO.Path]::GetFullPath($gitRoot.Trim()) -ne $repository -or
    !(Test-Path -LiteralPath (Join-Path $repository 'crates/novovm-node/Cargo.toml'))) { throw 'Unexpected repository root.' }
$reportFile = (Resolve-Path -LiteralPath $ReportPath).Path
$loadRoot = (Join-Path $repository 'target/native-product-load') + [IO.Path]::DirectorySeparatorChar
if ([IO.Path]::GetFileName($reportFile) -cne 'report.json' -or
    !$reportFile.StartsWith($loadRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Expected an in-repository native-product-load report.json.' }
$source = Split-Path -Parent $reportFile
$archive = Join-Path $PSScriptRoot "$Name.zip"
$manifestPath = Join-Path $PSScriptRoot "$Name.manifest.json"
if ((Test-Path -LiteralPath $archive) -or (Test-Path -LiteralPath $manifestPath)) { throw 'Destination already exists.' }
$allowedNames = @('report.json', 'create-0.progress.json', 'create-1.progress.json', 'create-2.progress.json', 'create-3.progress.json')
$files = @($allowedNames | ForEach-Object {
    $file = Get-Item -LiteralPath (Join-Path $source $_)
    if ($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Not a regular public file: $_" }
    $file
})
function Format-Query($Record) {
    if ($null -eq $Record) { return $null }
    $result = [ordered]@{}
    foreach ($key in @('node', 'index', 'count', 'sent', 'elapsed_us', 'deadline_us', 'sequence', 'state')) {
        if ($null -ne $Record.PSObject.Properties[$key]) { $result[$key] = $Record.$key }
    }
    if ($null -ne $Record.fragment) { $result['fragment'] = [Convert]::ToHexString([byte[]]$Record.fragment).ToLowerInvariant() }
    $result
}
$report = Get-Content -LiteralPath $reportFile -Raw | ConvertFrom-Json
$nodes = @(for ($node = 0; $node -lt 4; $node++) {
    $progress = Get-Content -LiteralPath $files[$node + 1].FullName -Raw | ConvertFrom-Json
    [ordered]@{ node = $node; head = $progress.head.height; indexed = $progress.rpc_indexed_height
        execution_failures = $progress.execution_failures; receiver_queue = $progress.native_receiver_queue
        asset_evicted = $progress.stage_profile.asset_ingress.evicted
        query_events = @($progress.stage_profile.asset_ingress.events | Where-Object {
            $_.event -like 'query_*' -and $_.detail.request_sequence -le 128
        } | Select-Object -Last 16 | ForEach-Object {
            [ordered]@{ event = $_.event; elapsed_ms = $_.elapsed_ns / 1e6; peer = $_.detail.peer
                sequence = $_.detail.request_sequence; state = $_.detail.state
                fragment = $(if ($null -ne $_.detail.fragment) { [Convert]::ToHexString([byte[]]$_.detail.fragment).ToLowerInvariant() }) }
        }) }
})
$fileManifest = @($files | ForEach-Object { [ordered]@{ name = $_.Name; bytes = $_.Length
    sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() } })
$manifest = [ordered]@{ schema = 'native-public-failure-v1'; created_utc = [DateTime]::UtcNow.ToString('o')
    source_report = [IO.Path]::GetRelativePath($repository, $reportFile); files = $fileManifest
    archive = [ordered]@{ name = [IO.Path]::GetFileName($archive); bytes = $null; sha256 = $null }
    summary = [ordered]@{ passed = $report.passed; error = $report.error; binary_sha256 = $report.binary_sha256
        elapsed_seconds = $report.elapsed_seconds; incomplete_seconds = $report.incomplete_timed_attempt_seconds
        counts = ($report.counts | Select-Object application_batches_sent, application_batches_transport_delivered,
            successful_transactions, deadline_expirations, receipt_query_requests_by_node, receipt_query_rows_received_by_node)
        query_timeout = (Format-Query $report.counts.query_timeout)
        latest_responses = @($report.counts.last_query_response | ForEach-Object { Format-Query $_ })
        recent_request_completions = @(for ($node = 0; $node -lt 4; $node++) {
            $recent = if ($null -ne $report.counts.query_transport_recent) { @($report.counts.query_transport_recent[$node]) } else { @() }
            [ordered]@{ node = $node; available = ($null -ne $report.counts.query_transport_recent); events = @($recent | ForEach-Object { Format-Query $_ }) }
        }); nodes = $nodes }
    limits = 'Only five exact public JSON files; no configs, databases, keys or journals. Missing counters are unknown; query summary retains last 16 covered events per node.' }
Compress-Archive -LiteralPath $files.FullName -DestinationPath $archive -CompressionLevel Optimal
$manifest.archive.bytes = (Get-Item -LiteralPath $archive).Length
$manifest.archive.sha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
for ($i = 0; $i -lt $files.Count; $i++) {
    if ((Get-FileHash -LiteralPath $files[$i].FullName -Algorithm SHA256).Hash.ToLowerInvariant() -ne $fileManifest[$i].sha256) { throw 'Source changed during collection; archive is not qualified.' }
}
$manifest | ConvertTo-Json -Depth 14 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
Write-Output $manifestPath
