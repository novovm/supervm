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
    foreach($k in @('entry','schema','profile','chain_id','current_height','head','execution_failures','ticks','recovery_in_progress','projection_error')){
        if($Value.Contains($k)){$r[$k]=$Value[$k]}
    }
    return $r
}
Assert-NoRedirect $file
$item=Get-Item -LiteralPath $file
if($item.Length -gt 2MB){throw 'Report exceeds bounded read; report size without exporting content'}
$hash=(Get-FileHash -LiteralPath $file).Hash
$value=Get-Content -LiteralPath $file -Raw|ConvertFrom-Json -AsHashtable -NoEnumerate
$nested=[ordered]@{}
if($value -is [Collections.IDictionary]){
    foreach($k in @($value.Keys|Select-Object -First 48)){if($value[$k] -is [Collections.IDictionary]){$nested[$k]=PublicFacts $value[$k]}}
}
$stdout=Join-Path $work 'logs\node.stdout'
$tail=@()
if(Test-Path -LiteralPath $stdout){
    Assert-NoRedirect $stdout
    foreach($line in @(Get-Content -LiteralPath $stdout -Tail 12)){
        if($line.Length -gt 2MB){continue}
        try{$row=$line|ConvertFrom-Json -AsHashtable -NoEnumerate;$tail+=@{shape=(Shape $row);facts=(PublicFacts $row)}}catch{continue}
    }
}
$exitPath=Join-Path $work 'logs\exit.json'
$exitRecord=$null
if(Test-Path -LiteralPath $exitPath){
    Assert-NoRedirect $exitPath
    $exitRecord=Get-Content -LiteralPath $exitPath -Raw|ConvertFrom-Json|Select-Object exit_code,start,time
}
if((Get-FileHash -LiteralPath $file).Hash -cne $hash){throw 'Report changed during read; do not mix snapshots'}
New-Item -ItemType Directory -Path $out|Out-Null
Assert-NoRedirect $out
[ordered]@{
    utc=[DateTime]::UtcNow.ToString('o');host_id='C';progress_path=$file
    progress_bytes=$item.Length;progress_sha256=$hash;last_write_utc=$item.LastWriteTimeUtc.ToString('o')
    shape=(Shape $value);facts=(PublicFacts $value);nested_facts=$nested;stdout_json_tail=$tail;owner_exit=$exitRecord
    node_started=$false;database_opened=$false;keys_read=$false;progress_modified=$false
}|ConvertTo-Json -Depth 30|Set-Content -LiteralPath (Join-Path $out 'progress-structure-public.json') -Encoding utf8NoBOM
Write-Output "READ_ONLY_COMPLETE: $out\progress-structure-public.json"
