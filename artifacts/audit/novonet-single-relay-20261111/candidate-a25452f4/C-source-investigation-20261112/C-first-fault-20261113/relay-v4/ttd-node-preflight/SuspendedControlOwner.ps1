#Requires -Version 7
param([Parameter(Mandatory)][string]$EvidenceRoot)
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
function Save($name,$value){$p=Join-Path $EvidenceRoot $name;if(Test-Path -LiteralPath $p){throw 'Evidence exists'};$value|ConvertTo-Json -Depth 6|Set-Content -LiteralPath ($p+'.tmp');Move-Item -LiteralPath ($p+'.tmp') -Destination $p}
Add-Type -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices; using System.ComponentModel;
public static class SuspendedChild {
 [StructLayout(LayoutKind.Sequential)] public struct PI {public IntPtr process,thread;public uint pid,tid;}
 [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] struct SI {public uint cb;public string reserved,desktop,title;public uint x,y,xs,ys,xc,yc,fill,flags;public ushort show,cb2;public IntPtr reserved2,input,output,error;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcess(string app,StringBuilder command,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string cwd,ref SI si,out PI pi);
 [DllImport("kernel32.dll",SetLastError=true)] public static extern uint ResumeThread(IntPtr thread);
 [DllImport("kernel32.dll",SetLastError=true)] public static extern bool TerminateProcess(IntPtr process,uint exit);
 [DllImport("kernel32.dll",SetLastError=true)] public static extern uint WaitForSingleObject(IntPtr process,uint ms);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetExitCodeProcess(IntPtr process,out uint code);
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
 public static PI Start(string exe,string arg){var si=new SI();si.cb=(uint)Marshal.SizeOf(si);PI p;if(!CreateProcess(exe,new StringBuilder("\""+exe+"\" "+arg),IntPtr.Zero,IntPtr.Zero,false,0x08000004,IntPtr.Zero,null,ref si,out p))throw new Win32Exception();return p;}
 public static uint Exit(PI p){uint code;if(!GetExitCodeProcess(p.process,out code))throw new Win32Exception();return code;}
}
'@
$child=$null
try{
 if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -cne 'S-1-5-19'){throw 'LocalService required'}
 foreach($mode in 'clean','bad'){
  $exe=Join-Path $EvidenceRoot 'ttd-pointer-control-aligned.exe'
  $child=[SuspendedChild]::Start($exe,$mode)
  $c=Get-CimInstance Win32_Process -Filter "ProcessId=$($child.pid)"
  if(!$c -or $c.ExecutablePath -ine $exe -or (Invoke-CimMethod $c -MethodName GetOwnerSid).Sid -cne 'S-1-5-19'){throw 'Child identity mismatch'}
  Save "$mode-child.json" @{pid=$child.pid;tid=$child.tid;path=$exe;created_utc=$c.CreationDate.ToUniversalTime().ToString('o');owner_sid='S-1-5-19';created_suspended=$true}
  $wait=[Diagnostics.Stopwatch]::StartNew()
  while(!(Test-Path -LiteralPath (Join-Path $EvidenceRoot "$mode-resume.json"))){
   if($wait.Elapsed.TotalSeconds -gt 100 -or (Test-Path -LiteralPath (Join-Path $EvidenceRoot 'stop.json'))){throw 'Attach gate expired'}
   if([SuspendedChild]::WaitForSingleObject($child.process,0) -eq 0){throw 'Child exited before resume'}
   Start-Sleep -Milliseconds 100
  }
  $count=[SuspendedChild]::ResumeThread($child.thread)
  if($count -ne 1){throw "Unexpected suspend count $count"}
  Save "$mode-resumed.json" @{utc=[DateTime]::UtcNow.ToString('o');prior_suspend_count=$count}
  $wait.Restart()
  while([SuspendedChild]::WaitForSingleObject($child.process,100) -ne 0){
   if($wait.Elapsed.TotalSeconds -gt 120 -or (Test-Path -LiteralPath (Join-Path $EvidenceRoot 'stop.json'))){throw 'Child execution bound reached'}
  }
  $u=[SuspendedChild]::Exit($child);$s=[BitConverter]::ToInt32([BitConverter]::GetBytes($u),0)
  Save "$mode-guest-exit.json" @{utc=[DateTime]::UtcNow.ToString('o');exit_code=$s;exit_hex=('0x{0:x8}' -f $u);kernel_handle_exit=$true}
  [void][SuspendedChild]::CloseHandle($child.thread);[void][SuspendedChild]::CloseHandle($child.process);$child=$null
 }
 Save 'owner-result.json' @{utc=[DateTime]::UtcNow.ToString('o');exit_code=0;node_started=$false};exit 0
}catch{Save 'owner-error.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;node_started=$false};exit 1}
finally{if($null -ne $child){$forced=$false;if([SuspendedChild]::WaitForSingleObject($child.process,0) -ne 0){$forced=[SuspendedChild]::TerminateProcess($child.process,0xe0000001);[void][SuspendedChild]::WaitForSingleObject($child.process,15000)};Save 'child-cleanup.json' @{pid=$child.pid;forced=$forced;exit_code=[SuspendedChild]::Exit($child)};[void][SuspendedChild]::CloseHandle($child.thread);[void][SuspendedChild]::CloseHandle($child.process)}}
