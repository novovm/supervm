#Requires -Version 7.0
param([Parameter(Mandatory=$true)][string]$OutputRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../..'))
$out=[IO.Path]::GetFullPath($OutputRoot)
if(Test-Path -LiteralPath $out){throw 'Fresh tools directory required'}
New-Item -ItemType Directory -Path $out|Out-Null
function Adapt([string]$Text){
    $Text=$Text.Replace('3fa11f940b18e54b46e6b927d3d66294bea37088','1ef3189307382ef579775c7ce2b55a08d829d04a')
    $Text=$Text.Replace('424e70b8b0acf847598f2bd7b5aefb4276bad052ab2afb64f7e8412d60d9e1bb','9f9ec5531652bce1cdec552705339e7e2743af5f04d6ccad3625713ddbf037b6')
    $Text=$Text.Replace('5a471fc805f86aaf57d6b06a4cfc1f4ba8aacff7eea67c74fabe036718a27b27','d419df614d6b804136eca82fd7427d84155241c82b7f74c3c00bf2fdb3ea6087')
    return $Text.Replace('3fa11f94','1ef31893').Replace('novonet-sqlite-1ef31893','novonet-observation-1ef31893')
}
function Save-New([string]$Name,[string]$Text){
    $f=[IO.File]::Open((Join-Path $out $Name),[IO.FileMode]::CreateNew)
    try{$f.Write([Text.UTF8Encoding]::new($false).GetBytes($Text))}finally{$f.Dispose()}
}
$script=Adapt (Get-Content -LiteralPath "$repo/scripts/windows/Update-NovonetExisting.ps1" -Raw)
Save-New 'Update-CD.ps1' ($script.Replace("ValidateSet('A','C','D')","ValidateSet('C','D')"))
$test=Adapt (Get-Content -LiteralPath "$repo/scripts/tests/novonet-existing-update.tests.ps1" -Raw)
$test=$test.Replace("'../windows/Update-NovonetExisting.ps1'","'Update-CD.ps1'").Replace("'../../target/relay-evidence-20261109/gate-tests-'","'gate-tests-'")
Save-New 'Test-Update-CD.ps1' $test
Copy-Item -LiteralPath "$repo/scripts/windows/NovonetExistingProcess.ps1" -Destination "$out/NovonetExistingProcess.ps1"
Write-Output "PREPARED_ONLY $out; no node operations"
