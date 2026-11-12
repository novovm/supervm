# Adapt only the proven one-shot sender's fixed path and CLI arity. No send here.
param([Parameter(Mandatory)][string]$OutputFile)
$ErrorActionPreference='Stop'
$repo=(Resolve-Path (Join-Path $PSScriptRoot '../../../..')).Path
$original=Join-Path $repo 'target/relay-constraint-20261111/Send-A-C-D.ps1'
if((Get-FileHash $original).Hash.ToLowerInvariant() -cne 'c5d2b328ebb9dfdddecff24e4bc84d341a007928169366ba54cee2982e112f74'){throw 'Original one-shot sender differs'}
if(Test-Path $OutputFile){throw 'Preserve generated sender'}
$s=Get-Content $original -Raw
function Change([string]$Old,[string]$New){
    if(!$script:s.Contains($Old)){throw "Original sender binding missing: $Old"}
    $script:s=$script:s.Replace($Old,$New)
}
Change '[string]$CReadyFile,' '[string]$BReadyFile,[Parameter(Mandatory)][string]$CReadyFile,'
Change '$short-20261111' '$short-20261112'
Change '@(0,2,3|' '@(0,1,2,3|'
Change "'b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43','1c054" "'b5d25773baaf82e8c109d41782a15ac3e45b57562f9bb023c8c1cab63d6acc43','4cecd4ba4092c821333501687e7a363d829d544c1a203ea2390d76e8b2a5627f','1c054"
Change "`$pathPeers=@('novovm-mldsa65:516fc" "`$pathPeers=@('novovm-mldsa65:3a44e06e371273a050641a80d9103ac4d357954aabc5336072cfb72864b4cf8f','novovm-mldsa65:516fc"
Change '--recipient $ids[2] --via $ids[1]' '--recipient $ids[3] --via $ids[1] --via $ids[2]'
Change "via='C'" "via=@('B','C')"
Change 'A/C/D' 'A/B/C/D'
Change 'A->C->D' 'A->B->C->D'
Change 'C forwarding' 'B/C forwarding'
$guard=@'
$b=Get-Content -LiteralPath $BReadyFile -Raw|ConvertFrom-Json
$bAge=([DateTime]::UtcNow-([DateTime]$b.utc).ToUniversalTime()).TotalSeconds
if($b.host -cne 'B' -or $b.source -cne $source -or $b.node_sha256 -cne '89854a24d4259b912fdbb8c6f8e21ea48af6c74c0c89da8f3cdc648c1845a769' -or !$b.read_only -or $b.pid -le 1 -or $bAge -lt -5 -or $bAge -gt 60 -or $b.execution_failures -ne 0 -or $b.diagnostic_relay.verified_locator_count -ne 3){throw 'Fresh B original-owner snapshot required'}
Copy-Item -LiteralPath $BReadyFile -Destination "$out/B-before.json"
'@
Change '$nonce=Hex ([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))' ($guard+"`n"+'$nonce=Hex ([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))')
$tokens=$null;$errors=$null
$null=[Management.Automation.Language.Parser]::ParseInput($s,[ref]$tokens,[ref]$errors)
if($errors.Count){throw 'Derived sender parse failed'}
if(([regex]::Matches($s,'--via \$ids\[')).Count -ne 2 -or !$s.Contains('--recipient $ids[3]') -or !$s.Contains('if($failed.Count)') -or !$s.Contains('send-relay --from') -or !$s.Contains('cancel --from')){throw 'Original guard/arity contract differs'}
$stream=[IO.File]::Open($OutputFile,[IO.FileMode]::CreateNew)
try{$stream.Write([Text.UTF8Encoding]::new($false).GetBytes($s))}finally{$stream.Dispose()}
Write-Output 'PASS original one-shot sender adapted: A/B/C/D, two ordered via, same prepare/send/cancel and deadlines; NOT executed'
