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

$created=[DateTime]::UtcNow
$fixture=[pscustomobject]@{ProcessId=123;ParentProcessId=1;CreationDate=$created}
$state=Get-NovonetNodeExecutionState -ReadNodes {$fixture} -ProbeProcess {
    param($ProcessId) [pscustomobject]@{ProcessId=$ProcessId;WaitResult=258;ExitCode=259;CreatedUtc=$created}
}
if($state.active.Count -ne 1 -or $state.exited.Count -ne 0){throw 'A running kernel process cannot be ignored'}
$state=Get-NovonetNodeExecutionState -ReadNodes {$fixture} -ProbeProcess {
    param($ProcessId) [pscustomobject]@{ProcessId=$ProcessId;WaitResult=0;ExitCode=259;CreatedUtc=$created}
}
if($state.active.Count -ne 0 -or $state.exited.Count -ne 1){throw 'Signaled process remains terminated even when its exit code is 259'}
foreach($kind in @('access','pid','created','wait')) {
    $rejected=$false
    try {
        $null=Get-NovonetNodeExecutionState -ReadNodes {$fixture} -ProbeProcess {
            param($ProcessId)
            if($kind -ceq 'access'){throw 'Access denied fixture'}
            [pscustomobject]@{ProcessId=$(if($kind -ceq 'pid'){124}else{$ProcessId});WaitResult=$(if($kind -ceq 'wait'){4294967295}else{0});ExitCode=0;CreatedUtc=$(if($kind -ceq 'created'){$created.AddSeconds(2)}else{$created})}
        }
    }catch{$rejected=$true}
    if(!$rejected){throw "Unproven $kind process must fail closed"}
}
$current=Get-NovonetKernelProcessState -ProcessId ([Diagnostics.Process]::GetCurrentProcess().Id)
if($current.WaitResult -ne 258){throw 'Actual running test process must be nonsignaled'}
$shellName=if($PSEdition -ceq 'Desktop'){'powershell.exe'}else{'pwsh.exe'}
$child=Start-Process -FilePath (Join-Path $PSHOME $shellName) -ArgumentList '-NoProfile -NonInteractive -Command "Start-Sleep -Milliseconds 1200; exit 7"' -WindowStyle Hidden -PassThru
try {
    $null=$child.Handle
    $running=Get-NovonetKernelProcessState -ProcessId $child.Id
    if($running.WaitResult -ne 258){throw 'Actual helper must initially be running'}
    if(!$child.WaitForExit(10000)){throw 'Isolated helper did not exit'}
    $ended=Get-NovonetKernelProcessState -ProcessId $child.Id
    if($ended.WaitResult -ne 0 -or $ended.ExitCode -ne 7 -or $ended.CreatedUtc -ne $running.CreatedUtc){throw 'Same real process handle must prove completed exit'}
} finally {$child.Dispose()}
Write-Output 'PASS 8 process-state cases including actual live and terminated Windows processes; no node was stopped'
