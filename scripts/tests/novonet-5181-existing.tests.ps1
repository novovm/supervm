#Requires -Version 7.0
param([string]$PackageRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$path=Join-Path $PSScriptRoot '../windows/Update-Novonet5181Existing.ps1'
$tokens=$null;$parseErrors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Resolve-Path -LiteralPath $path),[ref]$tokens,[ref]$parseErrors)
if($parseErrors.Count){throw 'Update script parse errors'}
$functions=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$false))
foreach($name in @('Write-NewJson','Assert-NoRedirect','Assert-Package')) {
    $function=@($functions|Where-Object Name -CEQ $name)
    if($function.Count -ne 1){throw 'Expected original deployment gate missing'}
    . ([scriptblock]::Create($function[0].Extent.Text))
}
$source='5181da733d59d998afa898bb9964db5deae9e72c'
$manifestPin='f381ccd4eed0a9176e3b91aded8f699f19bb64bd983307c44f1dbf8086162ab3'
$nodePin='ff68c5930b9b205b25ba5351a17ad08aa68f51d7cf71779d0507b6269f4087f3'
$root=Join-Path $PSScriptRoot ('../../target/novonet-5181-update-20261109/gate-tests-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root|Out-Null
$json=Join-Path $root 'record.json'
Write-NewJson $json ([ordered]@{test=$true;start='existing'})
if((Get-Content -LiteralPath $json -Raw|ConvertFrom-Json).start -cne 'existing'){throw 'New evidence did not round-trip'}
$rejected=$false
try{Write-NewJson $json @{test=$false}}catch [IO.IOException]{$rejected=$true}
if(!$rejected -or !(Get-Content -LiteralPath $json -Raw|ConvertFrom-Json).test){throw 'Existing evidence must remain intact'}
Assert-NoRedirect $json
$bad=Join-Path $root 'wrong-candidate'
New-Item -ItemType Directory -Path $bad|Out-Null
Write-NewJson (Join-Path $bad 'release-manifest.json') @{git_commit='wrong'}
$rejected=$false
try{Assert-Package $bad}catch{if($_.Exception.Message -cne 'Fixed manifest mismatch'){throw};$rejected=$true}
if(!$rejected){throw 'Wrong candidate accepted'}
$rejected=$false
try{& $path -HostId B -Owner}catch [Management.Automation.ParameterBindingException]{$rejected=$true}
if(!$rejected){throw 'B must not be in this update scope'}
$commands=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.CommandAst]},$true))
foreach($command in $commands) {
    if($command.GetCommandName() -in @('Remove-Item','Initialize-Disk','Set-NetFirewallProfile')){throw 'Forbidden state/profile mutation added'}
    if($command.Extent.Text -match 'native-genesis\s+initialize'){throw 'Initialization must not be callable'}
}
if($PackageRoot){Assert-Package $PackageRoot}
Write-Output 'PASS 7 deployment gates: syntax, new-only evidence, duplicate preservation, real path, bad package, host scope, forbidden actions'
if($PackageRoot){Write-Output 'PASS actual fixed Windows package through the same deployment gate'}
