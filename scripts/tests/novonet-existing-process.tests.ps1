$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot '../windows/NovonetExistingProcess.ps1')

# Isolated exit-enumeration fixtures. No real node, service or database is used.
$script:readCount = 0
Wait-NovonetNodeExit -ReadNodes {
    $script:readCount += 1
    if ($script:readCount -le 2) {
        [pscustomobject]@{ ProcessId=123; ParentProcessId=1; ExecutablePath='fixture.exe'; CreationDate=$null }
    }
}
if ($script:readCount -ne 4) { throw 'Transient exit rows must drain before two empty samples' }

$script:readCount = 0
Wait-NovonetNodeExit -ReadNodes {
    $script:readCount += 1
    if ($script:readCount -eq 2) { [pscustomobject]@{ ProcessId=456 } }
}
if ($script:readCount -ne 4) { throw 'An intervening process must reset the empty barrier' }

$rejected = $false
try {
    Wait-NovonetNodeExit -TimeoutMilliseconds 150 -ReadNodes {
        [pscustomobject]@{ ProcessId=789; ParentProcessId=1; ExecutablePath='fixture.exe'; CreationDate=$null }
    }
} catch {
    if ($_.Exception.Message -notlike '*Node exit barrier timed out*789*') { throw }
    $rejected = $true
}
if (-not $rejected) { throw 'Persistent node must fail the exit barrier' }

Wait-NovonetNodeExit -ReadNodes { $null }
$rejected = $false
try { Wait-NovonetNodeExit -ReadNodes { throw 'fixture query failure' } } catch {
    if ($_.Exception.Message -ne 'fixture query failure') { throw }
    $rejected = $true
}
if (-not $rejected) { throw 'Query errors must propagate' }
Write-Output 'PASS 5 isolated node-exit barrier cases'
