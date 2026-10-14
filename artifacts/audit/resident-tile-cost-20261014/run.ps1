$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (git rev-parse --show-toplevel).Trim()
Add-Type -AssemblyName System.Windows.Forms
if ([System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus -ne 'Online') { throw 'AC required' }
if (Get-Process cargo,rustc,novovm-node -ErrorAction SilentlyContinue) { throw 'Concurrent build/node' }
$env:NOVOVM_AOEM_TEST_LIBRARY = (Resolve-Path aoem/windows/core/bin/aoem_ffi.dll).Path
$env:AOEM_GPU_VK_DEVICE_NAME = '5090'
$env:AOEM_GPU_VK_VENDOR_ID = '0x10de'
$env:NOVOVM_PROFILE_STAGES = '0'
$test = 'target/release/deps/novovm_node-3fdab5433ef50d79.exe'
$log = Join-Path $PSScriptRoot 'tile-cost.log'
if (Test-Path -LiteralPath $log) { throw 'Evidence already exists' }
Get-FileHash $test,$env:NOVOVM_AOEM_TEST_LIBRARY | Format-List | Out-String | Tee-Object -FilePath $log
& $test resident_complete_plan_tile_cost_and_output_equivalence --ignored --nocapture --test-threads=1 2>&1 | Tee-Object -FilePath $log -Append
if ($LASTEXITCODE -ne 0) { throw 'Tile experiment failed' }
"power_after=$([System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus)" | Tee-Object -FilePath $log -Append
