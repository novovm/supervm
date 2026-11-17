$ErrorActionPreference='Stop'
$code=0
try {
  & 'D:\WEB3_AI\SUPERVM\artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1' -HostId C -Source 5b656189784c854ac356aae77610c931b9f2189d -NodeSha256 4b0d03e8ad7a5a93051cee2672d38aa9a4ec8c0cb99669ae89af62b8eeef4038 -WorkName novovm-common-5b656189-20261117 -CommonCapacity8 -TerminalCircuitCount 2 -MaximumWaitSeconds 7200 -EvidenceRoot 'D:\WEB3_AI\SUPERVM\target\c-55-inflight-observer-20261118-01\evidence'
} catch {
  $code=1
  Write-Error $_ -ErrorAction Continue
} finally {
  @{utc=[DateTime]::UtcNow.ToString('o');pid=$PID;exit_code=$code}|ConvertTo-Json|Set-Content -LiteralPath 'D:\WEB3_AI\SUPERVM\target\c-55-inflight-observer-20261118-01\collector-exit.json' -Encoding utf8
}
exit $code
