# Read the original owner's public observation, never synthesize missing fields.
function Read-RelayProgress {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$EvidenceRoot,
        [ValidateRange(1,15000)][int]$WaitMilliseconds=15000
    )
    $watch=[Diagnostics.Stopwatch]::StartNew()
    do {
        $bytes=$null;$keys=@();$reason=$null
        $item=Get-Item -LiteralPath $Path -ErrorAction Stop
        if($item.Length -gt 4MB -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Invalid observation file'}
        if(([DateTime]::UtcNow-$item.LastWriteTimeUtc).TotalSeconds -gt 15){throw 'Stale progress'}
        $stream=[IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
        try {
            # Bound the actual read even if the file grows after Get-Item.
            $bytes=[byte[]]::new(4MB+1);$count=0
            while($count -lt $bytes.Length){$n=$stream.Read($bytes,$count,$bytes.Length-$count);if(!$n){break};$count+=$n}
            if($count -gt 4MB){throw 'Observation exceeds bound'}
            $bytes=$bytes[0..($count-1)]
            if(!$count){$bytes=[byte[]]@()}
        } finally {$stream.Dispose()}
        try {$p=[Text.Encoding]::UTF8.GetString($bytes)|ConvertFrom-Json -AsHashtable -ErrorAction Stop}
        catch {$p=$null;$reason='incomplete_json'}
        if($p -is [System.Collections.IDictionary]) {
            $keys=@($p.Keys|Sort-Object)
            if($p['chain_id'] -ne 91129655606029){throw 'Wrong chain owner'}
            if($p['execution_failures'] -ne 0){throw 'Execution failure or missing health field'}
            if(!$p.Contains('novonet')){$reason='missing_novonet'}
            elseif($p['novonet'] -isnot [System.Collections.IDictionary] -or !$p['novonet'].Contains('diagnostic_relay')){throw 'Invalid diagnostic owner'}
            elseif($p['novonet']['diagnostic_relay'] -isnot [System.Collections.IDictionary] -or $p['novonet']['diagnostic_relay']['enabled'] -ne $true){throw 'Diagnostic owner disabled'}
            elseif($p['native_network_probe'] -isnot [System.Collections.IDictionary] -or $p['native_network_probe']['transport'] -isnot [System.Collections.IDictionary] -or !$p['native_network_probe']['transport'].Contains('relay_circuits')){throw 'Missing native transport observation'}
            else {return ([Text.Encoding]::UTF8.GetString($bytes)|ConvertFrom-Json)}
        } elseif(!$reason){$reason='incomplete_json'}
        # Only bounded public metadata, never export arbitrary report bytes.
        $record=[ordered]@{utc=[DateTime]::UtcNow.ToString('o');reason=$reason;bytes=$count;sha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([byte[]]$bytes)).ToLowerInvariant();top_level_keys=$keys;accepted=$false}
        $encoded=$record|ConvertTo-Json -Depth 5
        if(!(Test-Path -LiteralPath "$EvidenceRoot/read-anomaly-first.json")){$encoded|Set-Content "$EvidenceRoot/read-anomaly-first.json" -Encoding utf8}
        $encoded|Set-Content "$EvidenceRoot/read-anomaly-last.json" -Encoding utf8
        if($watch.ElapsedMilliseconds -ge $WaitMilliseconds){throw "No complete diagnostic snapshot within observation bound: $reason"}
        Start-Sleep -Milliseconds ([Math]::Max(1,[Math]::Min(100,$WaitMilliseconds-$watch.ElapsedMilliseconds)))
    } while($true)
}
