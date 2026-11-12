# Export public receipts and the original, candidate-pinned update tools only.
$ErrorActionPreference='Stop'
$PSNativeCommandUseErrorActionPreference=$false
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
$delivery='C:/Users/leadb/AppData/Local/NOVOVM/candidate-observation-a25452f4-20261112'
$tools=Join-Path $delivery 'update-tools'
$names=@('Update-Windows.ps1','Test-Update-Windows.ps1','NovonetExistingProcess.ps1')
$paths=@($names|ForEach-Object {Join-Path $tools $_})
$archive=Join-Path $PSScriptRoot 'existing-update-tools-a25452f4-v1.zip'
if(Test-Path $archive){throw 'Preserve previous exported tools'}
& pwsh -NoProfile -File "$tools/Test-Update-Windows.ps1" -PackageRoot "$repo/artifacts/product-overlay/novonet-observation-a25452f4-windows-x86_64" *> "$PSScriptRoot/windows-updater-tests.log"
if($LASTEXITCODE -ne 0){throw 'Derived original updater tests failed'}
Compress-Archive -LiteralPath $paths -DestinationPath $archive
$files=@($paths|ForEach-Object {[ordered]@{name=[IO.Path]::GetFileName($_);bytes=(Get-Item $_).Length;sha256=(Get-FileHash $_).Hash.ToLowerInvariant()}})
[ordered]@{source='a25452f4c49be37cd984e650f2079642e6c273b3';full_run=43155828234;archive=[IO.Path]::GetFileName($archive);bytes=(Get-Item $archive).Length;sha256=(Get-FileHash $archive).Hash.ToLowerInvariant();files=$files;generator_sha256=(Get-FileHash "$PSScriptRoot/Prepare-Windows.ps1").Hash.ToLowerInvariant();original_updater_sha256=(Get-FileHash "$repo/scripts/windows/Update-NovonetExisting.ps1").Hash.ToLowerInvariant();derived_updater_tests_exit=0;node_program_changed=$false;scope='875-to-a254 original existing updater bindings; not C/D deployment acceptance'}|ConvertTo-Json -Depth 8|Out-File "$PSScriptRoot/update-tools-manifest.json" -NoClobber -Encoding utf8
$a="$delivery/A-update-20261112"
foreach($name in @('result.json','stopped-owner-preflight.json','old-exit.json','firewall.json')){Copy-Item -LiteralPath "$a/$name" -Destination "$PSScriptRoot/A-$name"}
$after=Get-Content "$a/after.json" -Raw|ConvertFrom-Json
$after|Select-Object host_id,time,source,pid,exe,node_sha256,sid,database,start,initialize_executed,keys_generated,custody_unchanged,aoem_module,aoem_sha256,new_task,listeners,new_command_file_present|ConvertTo-Json -Depth 8|Out-File "$PSScriptRoot/A-update-public.json" -NoClobber -Encoding utf8
foreach($name in @('A-after-status','B-before-status','B-after-status')){
    $from="$repo/target/a254-deployment-20261112/$name"
    foreach($file in @('health-public.json','status-original.json','status-original.stderr','query-exit.json')){Copy-Item -LiteralPath "$from/$file" -Destination "$PSScriptRoot/$name-$file"}
}
Get-Content "$PSScriptRoot/update-tools-manifest.json"
