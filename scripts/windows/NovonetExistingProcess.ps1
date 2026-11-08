# Local deployment process barriers. No node configuration or database access.
function Get-NovonetKernelProcessState {
    param([Parameter(Mandatory=$true)][int]$ProcessId)
    if(-not ('Novonet.ProcessState' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace Novonet {
    public sealed class ProcessObservation {
        public int ProcessId;
        public uint WaitResult;
        public uint ExitCode;
        public DateTime CreatedUtc;
    }
    public static class ProcessState {
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern uint WaitForSingleObject(IntPtr process, uint milliseconds);
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern bool GetExitCodeProcess(IntPtr process, out uint code);
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern uint GetProcessId(IntPtr process);
        [DllImport("kernel32.dll", SetLastError=true)]
        static extern bool GetProcessTimes(IntPtr process, out long created, out long exited, out long kernel, out long user);
        [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
        public static ProcessObservation Read(int pid) {
            // Query and synchronize only: no terminate, write, VM read or debug rights.
            IntPtr handle=OpenProcess(0x00101000, false, pid);
            if(handle==IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "Cannot prove kernel process state");
            try {
                uint wait=WaitForSingleObject(handle, 0);
                if(wait!=0 && wait!=258) throw new Win32Exception(Marshal.GetLastWin32Error(), "Kernel process wait failed");
                uint exit;
                long created, exited, kernel, user;
                if(!GetExitCodeProcess(handle, out exit) || !GetProcessTimes(handle, out created, out exited, out kernel, out user))
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Kernel process query failed");
                uint actual=GetProcessId(handle);
                if(actual!=(uint)pid) throw new InvalidOperationException("Kernel process identity differs");
                return new ProcessObservation { ProcessId=pid, WaitResult=wait, ExitCode=exit, CreatedUtc=DateTime.FromFileTimeUtc(created) };
            } finally { CloseHandle(handle); }
        }
    }
}
'@
    }
    [Novonet.ProcessState]::Read($ProcessId)
}

function Get-NovonetNodeExecutionState {
    param(
        [scriptblock]$ReadNodes = { @(Get-CimInstance Win32_Process -Filter "Name='novovm-node.exe'") },
        [scriptblock]$ProbeProcess = { param($ProcessId) Get-NovonetKernelProcessState -ProcessId $ProcessId }
    )
    $active=@();$exited=@()
    foreach($node in @(& $ReadNodes | Where-Object {$null -ne $_})) {
        $kernel=& $ProbeProcess $node.ProcessId
        if($kernel.ProcessId -ne $node.ProcessId -or [Math]::Abs(($kernel.CreatedUtc-$node.CreationDate.ToUniversalTime()).TotalSeconds) -gt 1){throw 'Kernel/CIM process identity changed; preserve evidence'}
        if($kernel.WaitResult -eq 0) {
            # Only the signaled PROCESS handle proves termination; a zero thread count does not.
            $exited += [pscustomobject]@{pid=$node.ProcessId;parent_pid=$node.ParentProcessId;created_utc=$kernel.CreatedUtc;wait_result=$kernel.WaitResult;exit_code=$kernel.ExitCode;scope='kernel-signaled terminated process; not an active database owner'}
        } elseif($kernel.WaitResult -eq 258) {$active+=$node}
        else {throw 'Unproven kernel process state'}
    }
    [pscustomobject]@{active=@($active);exited=@($exited)}
}

function Wait-NovonetNodeExit {
    param(
        [int]$TimeoutMilliseconds = 10000,
        [scriptblock]$ReadNodes = { @((Get-NovonetNodeExecutionState).active) }
    )
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $emptySamples = 0
    do {
        $nodes = @(& $ReadNodes | Where-Object { $null -ne $_ })
        if ($nodes.Count -eq 0) {
            $emptySamples += 1
            if ($emptySamples -eq 2) { return }
        } else {
            $emptySamples = 0
        }
        Start-Sleep -Milliseconds 100
    } while ($timer.ElapsedMilliseconds -lt $TimeoutMilliseconds)
    $remaining = $nodes | Select-Object ProcessId, ParentProcessId, ExecutablePath, CreationDate
    throw ('Node exit barrier timed out; preserve process evidence: ' + ($remaining | ConvertTo-Json -Compress))
}

function Stop-NovonetExactProcess {
    param(
        [Parameter(Mandatory=$true)][int]$ProcessId,
        [Parameter(Mandatory=$true)][string]$ExpectedPath,
        [Parameter(Mandatory=$true)][string]$ExpectedSha256
    )
    $node = Get-CimInstance Win32_Process -Filter "ProcessId=$ProcessId"
    if ($null -eq $node -or $node.ExecutablePath -cne $ExpectedPath -or
        (Invoke-CimMethod -InputObject $node -MethodName GetOwnerSid).Sid -cne 'S-1-5-19' -or
        (Get-FileHash -LiteralPath $ExpectedPath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ExpectedSha256) {
        throw 'Exact node process identity changed before termination'
    }
    $process = Get-Process -Id $ProcessId -ErrorAction Stop
    try {
        # Keep the same OS process handle across termination and exit waiting.
        $null = $process.Handle
        if ($process.Path -cne $ExpectedPath -or
            [Math]::Abs(($process.StartTime.ToUniversalTime() - $node.CreationDate.ToUniversalTime()).TotalSeconds) -gt 1) {
            throw 'Process handle differs from inspected node'
        }
        $process.Kill()
        if (-not $process.WaitForExit(10000)) { throw 'Exact node process has not exited' }
    } finally {
        $process.Dispose()
    }
    Wait-NovonetNodeExit
}
