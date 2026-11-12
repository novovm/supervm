# Generate candidate-pinned copies of the original updater; no node is started.
param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
$source='a25452f4c49be37cd984e650f2079642e6c273b3'
$manifest='8e3042fcc99ab86ffd5193c864c879f7569164a4270e2767240357de21e60b9f'
$node='1758e4a79dd34d1688d8f7bf4b60f46ed9ced518186f42fdb16baf04c0d0c46d'
$oldNode='9174f29bad4bf1feb9acfbd5d2b5ed36bb47ff2a2cd1519d7742a2e1b944dee7'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Preserve existing generated tools'}
function Replace-Line([string]$Text,[string]$Prefix,[string]$Replacement){
    $pattern='(?m)^'+[regex]::Escape($Prefix)+'.*\r?$'
    if([regex]::Matches($Text,$pattern).Count -ne 1){throw "Expected one source assignment: $Prefix"}
    [regex]::Replace($Text,$pattern,[Text.RegularExpressions.MatchEvaluator]{param($m) $Replacement})
}
function Adapt([string]$Text){
    $Text.Replace('3fa11f940b18e54b46e6b927d3d66294bea37088',$source).
        Replace('424e70b8b0acf847598f2bd7b5aefb4276bad052ab2afb64f7e8412d60d9e1bb',$manifest).
        Replace('5a471fc805f86aaf57d6b06a4cfc1f4ba8aacff7eea67c74fabe036718a27b27',$node).
        Replace('3fa11f94-20261110','3fa11f94-20261112').
        Replace('3fa11f94-Existing-20261110','3fa11f94-Existing-20261112').
        Replace('3fa11f94-Existing-Rollback-20261110','3fa11f94-Existing-Rollback-20261112').
        Replace('3fa11f94-$HostId-20261110','3fa11f94-$HostId-20261112').
        Replace('novonet-sqlite-3fa11f94','novonet-observation-a25452f4').Replace('3fa11f94','a25452f4')
}
$windows=Adapt (Get-Content "$repo/scripts/windows/Update-NovonetExisting.ps1" -Raw)
foreach($pair in @(
    @('$oldNodePin=',('$oldNodePin='''+$oldNode+'''')),
    @('$oldWork=','$oldWork=Join-Path $state ''novonet-observation-875d3307-20261111'''),
    @('$oldRelease=','$oldRelease=''C:\ProgramData\NOVOVM\releases\novonet-observation-875d3307-windows-x86_64'''),
    @('$oldTask=','$oldTask="NOVOVM-$HostId-875d3307-Existing-20261111"'),
    @('$oldProgressPath=','$oldProgressPath=Join-Path $oldWork ''logs\progress.json'''),
    @('$oldRuleName=','$oldRuleName="NOVOVM-875d3307-$HostId-20261111"')
)){$windows=Replace-Line $windows $pair[0] $pair[1]}
$test=Adapt (Get-Content "$repo/scripts/tests/novonet-existing-update.tests.ps1" -Raw)
$test=$test.Replace("'../windows/Update-NovonetExisting.ps1'","'Update-Windows.ps1'").
    Replace("'../../target/relay-evidence-20261109/gate-tests-'","'gate-tests-'").
    Replace('9040ac63828c30011e8e7c4801be31183ecb69960854dd4974671df3d5d14d51',$oldNode).
    Replace('ff68c5930b9b205b25ba5351a17ad08aa68f51d7cf71779d0507b6269f4087f3',$oldNode).
    Replace('NOVOVM-D-08236129-Network-20261108','NOVOVM-D-875d3307-Existing-20261111').
    Replace('"$oldWork\progress.json"','"$oldWork\logs\progress.json"').
    Replace('D original 082 mapping differs','D original 875 mapping differs').
    Replace('A/C original 5181 mapping differs','A/C original 875 mapping differs')
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
foreach($pair in @(@('Update-Windows.ps1',$windows),@('Test-Update-Windows.ps1',$test),@('NovonetExistingProcess.ps1',(Get-Content "$repo/scripts/windows/NovonetExistingProcess.ps1" -Raw)))){
    $stream=[IO.File]::Open((Join-Path $OutputDirectory $pair[0]),[IO.FileMode]::CreateNew)
    try{$stream.Write([Text.UTF8Encoding]::new($false).GetBytes($pair[1]))}finally{$stream.Dispose()}
}
Write-Output 'Original Windows existing updater pinned for 875 -> a254; no live change'
