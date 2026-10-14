$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (git rev-parse --show-toplevel).Trim()
$contract = Get-Content (Join-Path $PSScriptRoot 'contract.json') -Raw | ConvertFrom-Json
foreach ($label in $contract.formal_order) {
    $variant = $label.Split('-')[0]
    $node = if ($variant -eq 'preoverlap') {
        'target/validation/resident-overlap-20261014/baseline-node.exe'
    } else {
        "target/validation/resident-cpu-queue-20261014/$variant-node.exe"
    }
    $backend = if ($label.Contains('-integer-')) { 'aoem_integer_v7' } else { 'cpu_callbacks' }
    & (Join-Path $PSScriptRoot 'run-load.ps1') -Label $label -Backend $backend -Node $node
    $result = Get-Content (Join-Path $PSScriptRoot "$label.json") -Raw | ConvertFrom-Json
    "FORMAL $label TPS=$($result.successful_tps) P95=$($result.latency_us.p95) P99=$($result.latency_us.p99)"
}
