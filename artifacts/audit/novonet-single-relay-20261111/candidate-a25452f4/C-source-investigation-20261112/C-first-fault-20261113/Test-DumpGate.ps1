#Requires -Version 7.0
$ErrorActionPreference='Stop'
$base=$PSScriptRoot
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $base 'FirstFaultOwner-v3.ps1'),[ref]$tokens,[ref]$errors)
if($errors.Count){throw $errors}
$fn=$ast.Find({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'AssertPrivateDump'},$true)
. ([scriptblock]::Create($fn.Extent.Text))
$actual=AssertPrivateDump (Join-Path $base 'dump-path-control-01/control-private.dmp')
$tests=[Collections.Generic.List[object]]::new()
$tests.Add(@{name='actual_full_memory_dump';pass=$actual.full_memory})
$root=Join-Path $base 'dump-gate-tests-01'
if(Test-Path $root){throw 'Evidence exists'}
New-Item -ItemType Directory $root|Out-Null
function Rejected($name,$path){
 $rejected=$false
 try{AssertPrivateDump $path|Out-Null}catch{$rejected=$true}
 $tests.Add(@{name=$name;pass=$rejected})
 if(!$rejected){throw "Unexpected acceptance: $name"}
}
Rejected 'missing' (Join-Path $root 'absent.dmp')
$file=Join-Path $root 'truncated.dmp';[IO.File]::WriteAllBytes($file,[byte[]](77,68,77,80));Rejected 'truncated' $file
$file=Join-Path $root 'bad-header.dmp';[IO.File]::WriteAllBytes($file,[byte[]]::new(32));Rejected 'invalid_signature' $file
$file=Join-Path $root 'not-full-memory.dmp';$bytes=[byte[]]::new(32);[Text.Encoding]::ASCII.GetBytes('MDMP').CopyTo($bytes,0);[IO.File]::WriteAllBytes($file,$bytes);Rejected 'missing_full_memory' $file
$source=Get-Content (Join-Path $base 'FirstFaultOwner-v3.ps1') -Raw
if($source.Contains('.dump /ma $dump;')){throw 'Old escaped path remains'}
if(([regex]::Matches($source,'\.dump /ma \$dumpCommandPath;')).Count -ne 2){throw 'Event commands mismatch'}
$tests.Add(@{name='both_fault_commands_normalized';pass=$true})
$tests.Add(@{name='ast_parse';pass=$true})
@{utc=[DateTime]::UtcNow.ToString('o');checks=$tests.ToArray();node_started=$false;product_fixed=$false}|ConvertTo-Json -Depth 6|Set-Content (Join-Path $root 'checks-public.json')
$tests|Format-Table
