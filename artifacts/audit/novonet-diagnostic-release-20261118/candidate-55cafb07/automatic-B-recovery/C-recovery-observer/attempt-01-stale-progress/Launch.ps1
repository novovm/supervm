$ErrorActionPreference='Stop'
$code=0
try {
    $watch = @{
        HostId = 'C'
        Source = '5b656189784c854ac356aae77610c931b9f2189d'
        NodeSha256 = '4b0d03e8ad7a5a93051cee2672d38aa9a4ec8c0cb99669ae89af62b8eeef4038'
        WorkName = 'novovm-common-5b656189-20261117'
        CommonCapacity8 = $true
        TerminalCircuitCount = 2
        MaximumWaitSeconds = 7200
        EvidenceRoot = Join-Path $PSScriptRoot 'evidence'
        ExpectedOrigin = 'novovm-mldsa65:6144e3de68011031592a7f66422b077989e643b7281b8904384b6cf1c321d5b3'
        ExpectedPath = @(
            'novovm-mldsa65:3a44e06e371273a050641a80d9103ac4d357954aabc5336072cfb72864b4cf8f',
            'novovm-mldsa65:516fc66162a3caa1d0153da9c42b5bbae9eae1e8bdcfb097b70aaa4ee97dd9f2',
            'novovm-mldsa65:49ba6bfc9206a0bc5fcb93e83975c10d10a228dbf7cd9bf4a2d909ca7df0d946'
        )
    }
    & 'D:\WEB3_AI\SUPERVM\artifacts\audit\novonet-single-relay-20261111\Watch-Relay.ps1' @watch
} catch {
    $code=1
    Write-Error $_ -ErrorAction Continue
} finally {
    @{utc=[DateTime]::UtcNow.ToString('o');pid=$PID;exit_code=$code}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $PSScriptRoot 'collector-exit.json') -Encoding utf8
}
exit $code
