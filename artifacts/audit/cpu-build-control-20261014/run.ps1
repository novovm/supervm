# Bounded negative control, not a replacement kernel or a new load generator.
# Same accepted runtime source; formal A/B/B/A, then separate diagnostics A/B.
$ErrorActionPreference = 'Stop'
$repo = (git rev-parse --show-toplevel).Trim()
Set-Location -LiteralPath $repo
Add-Type -AssemblyName System.Windows.Forms
$node = Join-Path $repo 'target/release/novovm-node.exe'
$driver = Join-Path $repo 'target/validation/dataflow-c48dd7bd/novovm_node-54e8d64d0bc52c1f.exe'
$old = Join-Path $repo 'aoem/windows/core/bin/aoem_ffi.dll'
$rebuilt = Join-Path $repo 'target/validation/cpu-build-control-20261014/rebuilt.dll'
$expected = @{
    $node = '4D6F4BBC8C85174D5D2B809781380335BE74E8490845AFF52510239BB77FD8F8'
    $driver = '07090A3C5BE02A4F0C3467E671F08520654DD41E7A6650E9DB22E625BEb5044A'
    $old = '31306037D7E764BD93E493FF61D4C69952ED1FFC18C2663B5D2C806D815E8947'
    $rebuilt = '9D0294F12C39664DAE729499EC08C8A6A417BA0A10D70CFC7F1AF32B06B65559'
}
$env:NOVOVM_RESIDENT_TEST_EXECUTION_BACKEND = 'cpu_callbacks'
$env:NOVOVM_RESIDENT_NODE_BINARY = $node
$env:NOVOVM_RESIDENT_RPC_LOAD_MODE = 'single_ingress'
$env:NOVOVM_RESIDENT_RPC_LOAD_INGRESS_NODE = '0'
$env:NOVOVM_RESIDENT_RPC_REQUIRE_HISTORY = '1'
$env:NOVOVM_RESIDENT_RPC_REQUIRE_EARLY_REUSE = '1'
$env:NOVOVM_RESIDENT_RPC_REQUIRE_ASYNC_AUTH = '1'
$env:NOVOVM_RESIDENT_RPC_REQUIRE_APFL = '1'
$env:AOEM_GPU_VK_DEVICE_NAME = '5090'
$env:AOEM_GPU_VK_VENDOR_ID = '0x10de'
foreach ($sample in @('old-1','rebuilt-1','rebuilt-2','old-2','old-diagnostic','rebuilt-diagnostic')) {
    if ([System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus -ne 'Online') { throw 'AC required' }
    if (Get-Process cargo,rustc,novovm-node -ErrorAction SilentlyContinue) { throw 'Build/node active' }
    foreach ($file in $expected.Keys) {
        if ((Get-FileHash -LiteralPath $file).Hash -ne $expected[$file]) { throw "Artifact changed: $file" }
    }
    $log = Join-Path $PSScriptRoot "$sample.log"
    if (Test-Path -LiteralPath $log) { throw "Refuse overwrite: $sample" }
    $env:NOVOVM_AOEM_TEST_LIBRARY = if ($sample.StartsWith('old-')) { $old } else { $rebuilt }
    $env:NOVOVM_PROFILE_STAGES = if ($sample.EndsWith('diagnostic')) { '1' } else { '0' }
    "sample=$sample utc=$([DateTime]::UtcNow.ToString('o')) profile=$env:NOVOVM_PROFILE_STAGES power=Online" | Tee-Object -FilePath $log
    & $driver actual_product_rpc_65536_signed_transfers_finality_and_cold_economics --ignored --nocapture --test-threads=1 2>&1 | Tee-Object -FilePath $log -Append
    $code = $LASTEXITCODE
    $reportLine = Get-Content -LiteralPath $log | Select-String 'real RPC load report=' | Select-Object -Last 1
    if ($reportLine) {
        $reportPath = $reportLine.Line.Substring($reportLine.Line.IndexOf('real RPC load report=') + 21)
        Copy-Item -LiteralPath $reportPath -Destination (Join-Path $PSScriptRoot "$sample.json")
    }
    "power_after=$([System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus)" | Tee-Object -FilePath $log -Append
    if ($code -ne 0) { throw "Load failed: $sample ($code)" }
    if ([System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus -ne 'Online') { throw 'AC changed; sample not comparable' }
}
