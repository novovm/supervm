#Requires -Version 7.0
param(
    [Parameter(Mandatory)][ValidatePattern('^https://192\.168\.71\.73:[0-9]+$')][string]$Endpoint,
    [Parameter(Mandatory)][ValidatePattern('^sha256//[A-Za-z0-9+/]{43}=$')][string]$TlsPin,
    [Parameter(Mandatory)][DateTimeOffset]$Deadline
)
$ErrorActionPreference='Stop'
$end=$Deadline
$base=Join-Path $env:LOCALAPPDATA 'NOVOVM'
$root=Join-Path $base 'D-Mailbox'
$program=Join-Path $base 'Rendezvous/novovm-rendezvous-service.exe'
$config=Join-Path $base 'Rendezvous/config.json'
$out=Join-Path $base ('D-rolling-retry-'+[datetime]::UtcNow.ToString('yyyyMMddTHHmmss'))
if(Test-Path $out){throw 'Rolling handoff already attempted'}
New-Item -ItemType Directory $out|Out-Null
$tls=@('--fail-with-body','--silent','--show-error','--noproxy','*','--interface','192.168.71.59','--proto','=https','--insecure','--pinnedpubkey',$TlsPin,'--max-time','18')
$approved=Get-Content 'D:/WEB3_AI/SUPERVM/target/d-v8-approved-inputs/A-successor-pin.json' -Raw|ConvertFrom-Json
$round=0
$lastD=-1
$lastA=-1
$skipD=-1
function WaitInWindow([int]$Seconds=5){
    $remaining=($end-[DateTimeOffset]::UtcNow).TotalSeconds
    if($remaining -gt 0){Start-Sleep -Seconds ([math]::Min($Seconds,[math]::Ceiling($remaining)))}
}
function ReadRetryAfter([string]$Headers){
    $value=@(Get-Content $Headers|Select-String '^Retry-After:\s*(.+)'|ForEach-Object {$_.Matches[0].Groups[1].Value.Trim()})|Select-Object -Last 1
    if(!$value){return 5}
    $seconds=0
    if([int]::TryParse($value,[ref]$seconds)){return [math]::Max(1,$seconds)}
    $date=[DateTimeOffset]::MinValue
    if([DateTimeOffset]::TryParse($value,[ref]$date)){return [int][math]::Max(1,[math]::Ceiling(($date-[DateTimeOffset]::UtcNow).TotalSeconds))}
    throw 'Invalid Retry-After; response preserved'
}
function Transfer([string]$Url,[string]$File,[string]$Prefix){
    $arguments=@($tls)+@('--dump-header',"$Prefix.headers",'--output',"$Prefix.body",'--write-out','%{http_code}')
    if($File){$arguments+=@('--header','Content-Type: application/json','--data-binary',("@"+$File))}
    $code=& curl.exe @arguments $Url
    $exitCode=$LASTEXITCODE
    @{utc=[DateTimeOffset]::UtcNow.ToString('o');http=$code;curl_exit=$exitCode}|ConvertTo-Json|Set-Content "$Prefix.status.json"
    if($exitCode -notin @(0,22)){throw "TLS/network failure $exitCode; evidence preserved"}
    return [string]$code
}
function Verify([string]$record,[string]$pin,[string]$label){
    $p=Start-Process $program -WindowStyle Hidden -ArgumentList @('--verify-private-apfl-announcement',('"'+$config+'"'),('"'+$record+'"'),('"'+$pin+'"')) -RedirectStandardOutput "$out/$label.stdout" -RedirectStandardError "$out/$label.stderr" -PassThru -Wait
    if($p.ExitCode -ne 0 -or (Get-Content "$out/$label.stdout" -Raw) -notmatch 'role=forwarder'){throw "$label original verification failed"}
}
try {
    if([DateTimeOffset]::UtcNow -ge $end){throw 'Window expired'}
    $reply=& curl.exe @tls "$endpoint/ready"
    if($LASTEXITCODE -ne 0 -or $reply.Trim() -cne 'D-to-C APFL receiver online'){throw 'Pinned TLS probe failed'}
    while([DateTimeOffset]::UtcNow.AddSeconds(20) -lt $end){
        if(!(Get-Process -Id 56632 -ErrorAction SilentlyContinue) -or !(Get-Process -Id 300 -ErrorAction SilentlyContinue)){throw 'Original processes changed'}
        $round++
        $dir=Join-Path $out "round-$round"
        New-Item -ItemType Directory $dir|Out-Null
        $get=Transfer "$endpoint/a-second-hop" '' "$dir/A-download"
        if($get -eq '503'){WaitInWindow;continue}
        if($get -ne '200'){throw "A download HTTP $get; response preserved"}
        Copy-Item "$dir/A-download.body" "$dir/A.zip"
        [IO.Compression.ZipFile]::ExtractToDirectory("$dir/A.zip","$dir/A")
        $pin="$dir/A/A-private-apfl-approved-pin.json"
        $record="$dir/A/A-private-apfl-current.json"
        $p=Get-Content $pin -Raw|ConvertFrom-Json
        if(($p.identity -join ',') -cne ($approved.identity -join ',') -or ($p.scope -join ',') -cne ($approved.scope -join ',')){throw 'A approved identity or scope differs'}
        $a=Get-Content $record -Raw|ConvertFrom-Json
        if($a.record.sequence -lt $lastA){throw 'A sequence regressed'}
        if($a.record.expires_at_ms -le [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()){WaitInWindow;continue}
        Verify $record $pin "A-$round"
        $lastA=$a.record.sequence
        $tmp=Join-Path $root ('private-apfl-next.'+[guid]::NewGuid().ToString('N')+'.tmp')
        [IO.File]::WriteAllBytes($tmp,[IO.File]::ReadAllBytes($record))
        [IO.File]::Replace($tmp,"$root/private-apfl-next.json","$dir/A-next-previous.json")
        $bytes=[IO.File]::ReadAllBytes("$root/private-apfl-record.json")
        $d=[Text.Encoding]::UTF8.GetString($bytes)|ConvertFrom-Json
        if($d.record.expires_at_ms -le [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()){WaitInWindow;continue}
        if($d.record.sequence -gt [math]::Max($lastD,$skipD)){
            $pinD=Join-Path $base 'D-first-hop-20261122T164559/D-approved-pin.json.compact'
            [IO.File]::WriteAllBytes("$dir/D.json",$bytes)
            Verify "$dir/D.json" $pinD "D-$round"
            $sha=(Get-FileHash "$dir/D.json").Hash.ToLowerInvariant()
            $attempt=0
            do {
                $attempt++
                $prefix="$dir/C-upload-$attempt"
                $http=Transfer "$endpoint/d-first-hop" "$dir/D.json" $prefix
                if($http -eq '425'){
                    WaitInWindow (ReadRetryAfter "$prefix.headers")
                    if($d.record.expires_at_ms -le [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()){$http='410';break}
                }
            } while($http -eq '425' -and [DateTimeOffset]::UtcNow.AddSeconds(20) -lt $end)
            if($http -in @('409','410')){$skipD=$d.record.sequence;WaitInWindow;continue}
            if($http -eq '503'){WaitInWindow;continue}
            if($http -eq '425'){break}
            if($http -ne '201' -or (Get-Content "$prefix.body" -Raw) -notmatch [regex]::Escape($sha)){throw "C HTTP $http or digest failure; full response preserved"}
            $lastD=$d.record.sequence
            @{utc=[DateTimeOffset]::UtcNow.ToString('o');A_sequence=$lastA;D_sequence=$lastD;sha256=$sha;http=201;round=$round;end_utc=$end.ToUniversalTime().ToString('o');owner_pid=56632;validator_pid=300}|ConvertTo-Json -Compress|Add-Content "$out/receipts.jsonl"
        }
        @{utc=[DateTimeOffset]::UtcNow.ToString('o');round=$round;A_sequence=$lastA;D_sequence=$lastD;end_utc=$end.ToUniversalTime().ToString('o')}|ConvertTo-Json|Set-Content "$out/liveness.json"
        Start-Sleep -Seconds 12
    }
    @{status='WINDOW_ENDED';A_sequence=$lastA;D_sequence=$lastD;rounds=$round}|ConvertTo-Json|Set-Content "$out/result.json"
} catch { @{error=$_.Exception.Message;round=$round;A_sequence=$lastA;D_sequence=$lastD}|ConvertTo-Json|Set-Content "$out/error.json";exit 1 }
