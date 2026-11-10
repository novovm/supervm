#Requires -Version 7.0
# Read-only inspection of the exact C old-owner report that blocked existing update.
param([Parameter(Mandatory=$true)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Administrator PowerShell required to read original owner evidence'}
$work='C:\ProgramData\NOVOVM\selfrun\91129655606029\C\novonet-probe-5181da73-20261109'
$file=Join-Path $work 'logs\progress.json'
$out=[IO.Path]::GetFullPath($EvidenceRoot)
if(Test-Path -LiteralPath $out){throw 'Fresh evidence directory required'}
function Assert-NoRedirect([string]$Path){
    $item=Get-Item -LiteralPath $Path -Force
    while($null -ne $item){
        if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Redirected evidence path'}
        $item=if($item -is [IO.DirectoryInfo]){$item.Parent}else{$item.Directory}
    }
}
function Shape($Value,[int]$Depth=0){
    if($null -eq $Value){return @{type='null'}}
    if($Value -is [Collections.IDictionary]){
        $r=[ordered]@{type='object';keys=@($Value.Keys|Select-Object -First 48)}
        if($Depth -lt 2){
            $children=[ordered]@{}
            foreach($k in @($Value.Keys|Select-Object -First 48)){$children[$k]=Shape $Value[$k] ($Depth+1)}
            $r['children']=$children
        }
        return $r
    }
    if($Value -is [array]){return @{type='array';count=$Value.Count;sample=@($Value|Select-Object -First 2|ForEach-Object {if($Depth -lt 2){Shape $_ ($Depth+1)}})}}
    return @{type=$Value.GetType().Name}
}
function PublicFacts($Value){
    $r=[ordered]@{}
    if($Value -isnot [Collections.IDictionary]){return $r}
    foreach($k in @('entry','schema','profile','chain_id','current_height','head','execution_failures','ticks','recovery_in_progress','exit_code','start','time')){
        if($Value.Contains($k)){$r[$k]=$Value[$k]}
    }
    return $r
}
function Read-PublicArtifact([string]$Path){
    # Do not mistake a JSON parser's null return for a valid report. No raw bytes
    # or exception text (which can quote file contents) enter the public result.
    if(!(Test-Path -LiteralPath $Path)){return [ordered]@{path=$Path;status='missing'}}
    Assert-NoRedirect $Path
    $item=Get-Item -LiteralPath $Path
    $r=[ordered]@{path=$Path;bytes=$item.Length;last_write_utc=$item.LastWriteTimeUtc.ToString('o')}
    if($item.PSIsContainer){$r.status='not_file';return $r}
    if($item.Length -gt 4MB){$r.status='exceeds_bounded_read';return $r}
    $bytes=[IO.File]::ReadAllBytes($Path)
    $digest=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes))
    $r.sha256=$digest
    $zeroCount=0
    foreach($b in $bytes){if($b -eq 0){$zeroCount++}}
    $r.zero_bytes=$zeroCount
    if($bytes.Length -eq 0){$r.status='empty'}
    elseif($zeroCount -eq $bytes.Length){$r.status='all_zero_not_json'}
    else {
        try {
            # StreamReader handles the BOM emitted by older PowerShell writers.
            $stream=[IO.MemoryStream]::new($bytes,$false)
            $reader=[IO.StreamReader]::new($stream,[Text.UTF8Encoding]::new($false,$true),$true)
            try{$text=$reader.ReadToEnd()}finally{$reader.Dispose()}
            $document=[System.Text.Json.JsonDocument]::Parse($text)
            try{$r.json_kind=$document.RootElement.ValueKind.ToString()}finally{$document.Dispose()}
            $v=ConvertFrom-Json -InputObject $text -AsHashtable -NoEnumerate
            $r.status='valid_json';$r.shape=Shape $v;$r.facts=PublicFacts $v
            $nested=[ordered]@{}
            if($v -is [Collections.IDictionary]){
                foreach($k in @($v.Keys|Select-Object -First 48)){if($v[$k] -is [Collections.IDictionary]){$nested[$k]=PublicFacts $v[$k]}}
            }
            $r.nested_facts=$nested
        } catch {$r.status='invalid_json_or_encoding';$r.error_type=$_.Exception.GetType().Name}
    }
    if((Get-FileHash -LiteralPath $Path).Hash -cne $digest){throw 'Artifact changed during read; do not mix snapshots'}
    return $r
}
Assert-NoRedirect $file
$item=Get-Item -LiteralPath $file
if($item.Length -gt 2MB){throw 'Report exceeds bounded read; report size without exporting content'}
$hash=(Get-FileHash -LiteralPath $file).Hash
$artifacts=@('progress.json','node.stdout','exit.json'|ForEach-Object {Read-PublicArtifact (Join-Path $work "logs\$_")})
if((Get-FileHash -LiteralPath $file).Hash -cne $hash){throw 'Report changed during read; do not mix snapshots'}
New-Item -ItemType Directory -Path $out|Out-Null
Assert-NoRedirect $out
[ordered]@{
    utc=[DateTime]::UtcNow.ToString('o');host_id='C';progress_path=$file
    progress_bytes=$item.Length;progress_sha256=$hash;last_write_utc=$item.LastWriteTimeUtc.ToString('o')
    inspection_version=2;artifacts=$artifacts
    node_started=$false;database_opened=$false;keys_read=$false;progress_modified=$false
}|ConvertTo-Json -Depth 30|Set-Content -LiteralPath (Join-Path $out 'progress-structure-public.json') -Encoding utf8NoBOM
Write-Output "READ_ONLY_COMPLETE: $out\progress-structure-public.json"
