#Requires -Version 7.0
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$repo=Split-Path (Split-Path $PSScriptRoot)
$path=Join-Path $repo 'artifacts/audit/novonet-seed-retention-20261110/candidate-1ef31893/Inspect-C-Progress.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($path,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Inspector syntax invalid'}
foreach($name in @('Assert-NoRedirect','Shape','PublicFacts','Read-PublicArtifact')){
    $f=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]},$false)|Where-Object Name -CEQ $name)
    if($f.Count -ne 1){throw 'Missing inspection function'}
    . ([scriptblock]::Create($f[0].Extent.Text))
}
$root=Join-Path $repo ('target/progress-inspection-tests-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root|Out-Null
function Fixture([string]$Name,[byte[]]$Bytes){
    $file=Join-Path $root $Name
    $f=[IO.File]::Open($file,[IO.FileMode]::CreateNew)
    try{$f.Write($Bytes)}finally{$f.Dispose()}
    return Read-PublicArtifact $file
}
$cases=0
$z=Fixture 'zero.json' ([byte[]]::new(813127))
if($z.sha256 -cne 'A917D1DAF6F38D05748209520752B122ECDC1250812DF597B3CFD7B7021EA36F' -or $z.status -cne 'all_zero_not_json' -or $z.zero_bytes -ne 813127){throw 'C zero-byte reproduction mismatch'};$cases++
if((Read-PublicArtifact (Join-Path $root 'missing')).status -cne 'missing'){throw 'Missing file classification'};$cases++
if((Fixture 'empty.json' ([byte[]]::new(0))).status -cne 'empty'){throw 'Empty file classification'};$cases++
$n=Fixture 'null.json' ([Text.Encoding]::UTF8.GetBytes('null'))
if($n.status -cne 'valid_json' -or $n.json_kind -cne 'Null' -or $n.shape.type -cne 'null'){throw 'Literal null distinction'};$cases++
$pretty=@{entry='original_native_execution_tick';chain_id=91129655606029;current_height=0;head=@(1,2);execution_failures=0;secret='DO_NOT_EXPORT';padding=@(1..30)}|ConvertTo-Json -Depth 5
$p=Fixture 'pretty.stdout' ([Text.Encoding]::UTF8.GetBytes($pretty))
if($p.status -cne 'valid_json' -or $p.facts.chain_id -ne 91129655606029 -or ($p|ConvertTo-Json -Depth 20) -match 'DO_NOT_EXPORT'){throw 'Full multiline status or filtering failed'};$cases++
$exit=Fixture 'exit.json' ([Text.Encoding]::UTF8.GetBytes('{"exit_code":0,"start":"existing","time":"2026-11-10T00:00:00Z"}'))
if($exit.facts.exit_code -ne 0 -or $exit.facts.start -cne 'existing'){throw 'Exit projection failed'};$cases++
$bad=Fixture 'bad.json' ([Text.Encoding]::UTF8.GetBytes('{DO_NOT_EXPORT'))
if($bad.status -cne 'invalid_json_or_encoding' -or ($bad|ConvertTo-Json) -match 'DO_NOT_EXPORT'){throw 'Invalid input leaked'};$cases++
if((Fixture 'oversize' ([byte[]]::new(4MB+1))).status -cne 'exceeds_bounded_read'){throw 'Read bound missing'};$cases++
$bom=[Text.Encoding]::Unicode.GetPreamble()+[Text.Encoding]::Unicode.GetBytes($pretty)
if((Fixture 'utf16.stdout' $bom).facts.chain_id -ne 91129655606029){throw 'BOM detection failed'};$cases++
if((Fixture 'whitespace' ([Text.Encoding]::UTF8.GetBytes('   '))).status -cne 'invalid_json_or_encoding'){throw 'Whitespace accepted as JSON'};$cases++
$r=Fixture 'trailing-zero' ([Text.Encoding]::UTF8.GetBytes('{}')+[byte[]]@(0))
if($r.status -cne 'invalid_json_or_encoding'){throw 'NUL accepted by strict parser'};$cases++
Write-Output "PASS $cases artifact inspections; exact C all-zero digest reproduced; no node or database operations"
