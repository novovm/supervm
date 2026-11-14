param([Parameter(Mandatory)][string]$Root)
$ErrorActionPreference='Stop'
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if($sid -cne 'S-1-5-19'){throw 'LocalService required'}
$start=[DateTime]::UtcNow.ToString('o')
& (Join-Path $Root 'novovm-wer-probe-C-20261114.exe') (Join-Path $Root 'probe-metadata.json')
$code=$LASTEXITCODE
[IO.File]::WriteAllText((Join-Path $Root 'probe-exit.json'),(@{sid=$sid;start_utc=$start;end_utc=[DateTime]::UtcNow.ToString('o');exit_code=$code;exit_hex=('0x{0:x8}' -f $code)} | ConvertTo-Json))
exit $code
