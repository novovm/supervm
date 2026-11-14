# Prepared only; NOT executed during qualification. Preserve every dump.
#Requires -Version 7.0
$ErrorActionPreference='Stop'
$admin=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$admin.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator required'}
$before=Get-Content (Join-Path $PSScriptRoot 'registry-before.json') -Raw|ConvertFrom-Json
$expected=(Get-Content (Join-Path $PSScriptRoot 'registry-after.json') -Raw|ConvertFrom-Json).node
$key='HKLM:\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\novovm-node.exe'
if($before.node.exists -or $expected.path -cne $key){throw 'Not the reviewed absent-before target'}
if(!(Test-Path -LiteralPath $key)){throw 'Already absent; no action'}
$item=Get-Item -LiteralPath $key
if($item.SubKeyCount -ne 0 -or $item.ValueCount -ne 3){throw 'Configuration changed; stop'}
foreach($v in $expected.values){
 if($item.GetValueKind($v.name).ToString() -cne $v.kind -or
    $item.GetValue($v.name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) -cne $v.value){throw 'Configuration changed; stop'}
}
Remove-Item -LiteralPath $key
if(Test-Path -LiteralPath $key){throw 'Removal failed'}
Write-Output 'Removed only the matching per-image key. Dump directories and files preserved; global WER/AeDebug unchanged.'
