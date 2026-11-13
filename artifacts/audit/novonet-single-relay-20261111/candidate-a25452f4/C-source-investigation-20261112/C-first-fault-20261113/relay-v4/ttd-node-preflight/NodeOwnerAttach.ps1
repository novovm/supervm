#Requires -Version 7
param([Parameter(Mandatory)][string]$WorkRoot)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
function Save($name,$value){$p=Join-Path $WorkRoot $name;if(Test-Path -LiteralPath $p){throw 'Evidence exists'};$value|ConvertTo-Json -Depth 8|Set-Content -LiteralPath ($p+'.tmp');Move-Item -LiteralPath ($p+'.tmp') -Destination $p}
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
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetProcessTimes(IntPtr process,out long created,out long exited,out long kernel,out long user);
 public static string Created(PI p){long c,e,k,u;if(!GetProcessTimes(p.process,out c,out e,out k,out u))throw new Win32Exception();return DateTime.FromFileTimeUtc(c).ToString("o");}
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
 [StructLayout(LayoutKind.Sequential)] struct SA {public uint length;public IntPtr descriptor;[MarshalAs(UnmanagedType.Bool)] public bool inherit;}
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateFile(string path,uint access,uint share,ref SA sa,uint disposition,uint flags,IntPtr template);
 public static PI StartLogged(string exe,string stdout,string stderr){
  var sa=new SA{length=(uint)Marshal.SizeOf(typeof(SA)),inherit=true};IntPtr o=IntPtr.Zero,e=IntPtr.Zero,i=IntPtr.Zero;
  try{o=CreateFile(stdout,0x40000000,1,ref sa,1,0x80,IntPtr.Zero);if(o.ToInt64()==-1)throw new Win32Exception();e=CreateFile(stderr,0x40000000,1,ref sa,1,0x80,IntPtr.Zero);if(e.ToInt64()==-1)throw new Win32Exception();i=CreateFile("NUL",0x80000000,3,ref sa,3,0x80,IntPtr.Zero);if(i.ToInt64()==-1)throw new Win32Exception();var si=new SI();si.cb=(uint)Marshal.SizeOf(si);si.flags=0x100;si.input=i;si.output=o;si.error=e;PI p;if(!CreateProcess(exe,new StringBuilder("\""+exe+"\""),IntPtr.Zero,IntPtr.Zero,true,0x08000004,IntPtr.Zero,null,ref si,out p))throw new Win32Exception();return p;}
  finally{foreach(var h in new[]{o,e,i})if(h!=IntPtr.Zero&&h.ToInt64()!=-1)CloseHandle(h);}
 }
 public static PI Start(string exe,string arg){var si=new SI();si.cb=(uint)Marshal.SizeOf(si);PI p;if(!CreateProcess(exe,new StringBuilder("\""+exe+"\" "+arg),IntPtr.Zero,IntPtr.Zero,false,0x08000004,IntPtr.Zero,null,ref si,out p))throw new Win32Exception();return p;}
 public static uint Exit(PI p){uint code;if(!GetExitCodeProcess(p.process,out code))throw new Win32Exception();return code;}
}
'@
$child=$null;$ownerExit=1;$reason='not_started';$forced=$false
try{
 if([Security.Principal.WindowsIdentity]::GetCurrent().User.Value -cne 'S-1-5-19'){throw 'LocalService required'}
 $plan=Get-Content -LiteralPath (Join-Path $WorkRoot 'plan.json') -Raw|ConvertFrom-Json
 $cfg=Get-Content -LiteralPath (Join-Path $WorkRoot 'run.json') -Raw|ConvertFrom-Json
 if($cfg.database -cne $plan.database -or (Get-FileHash -LiteralPath $plan.node).Hash -ine $plan.node_sha256 -or (Get-FileHash -LiteralPath $cfg.library).Hash -ine $plan.runtime_sha256){throw 'Original state/program guard failed'}
 $env:NOVOVM_AOEM_ROOT=$plan.runtime_root;$env:NOVOVM_AOEM_DLL=$cfg.library
 $env:NOVOVM_AOEM_MANIFEST=Join-Path $plan.runtime_root 'manifest\aoem-manifest.json';$env:AOEM_DLL_MANIFEST=$env:NOVOVM_AOEM_MANIFEST;$env:AOEM_DLL_MANIFEST_REQUIRED='1'
 $env:NOVOVM_AOEM_RUNTIME_PROFILE=Join-Path $plan.runtime_root 'config\aoem-runtime-profile.json';$env:NOVOVM_AOEM_PLUGIN_DIR=Join-Path $plan.runtime_root 'windows\core\plugins'
 $env:AOEM_SEMANTIC_GRAPH_V2_WORKERS='1';$env:NOVOVM_NODE_MODE='native_execution_tick'
 $env:NOVOVM_NATIVE_EXECUTION_CONFIG=Join-Path $WorkRoot 'run.json';$env:NOVOVM_NATIVE_EXECUTION_START='existing'
 $env:NOVOVM_NATIVE_EXECUTION_RUN_MS='7200000';$env:NOVOVM_PROFILE_NATIVE_DELIVERY='1';$env:RUST_BACKTRACE='full'
 $env:NOVOVM_NATIVE_EXECUTION_PIPELINE_PROGRESS_REPORT_PATH=$plan.progress
 $child=[SuspendedChild]::StartLogged($plan.node,(Join-Path $WorkRoot 'node-private.stdout'),(Join-Path $WorkRoot 'node-private.stderr'))
 Save 'node-created-public.json' @{pid=$child.pid;tid=$child.tid;path=$plan.node;created_utc=[SuspendedChild]::Created($child);owner_sid='S-1-5-19';created_suspended=$true;start='existing';relay_ready=$false}
 $clock=[Diagnostics.Stopwatch]::StartNew();$reason='attach_gate'
 while(!(Test-Path -LiteralPath (Join-Path $WorkRoot 'resume-approved.json'))){
  if($clock.Elapsed.TotalSeconds -gt 120 -or (Test-Path -LiteralPath (Join-Path $WorkRoot 'stop-request.json'))){throw 'Attach gate expired; node main not resumed'}
  if([SuspendedChild]::WaitForSingleObject($child.process,0) -eq 0){throw 'Node exited before resume'}
  Start-Sleep -Milliseconds 100
 }
 $gate=Get-Content -LiteralPath (Join-Path $WorkRoot 'resume-approved.json') -Raw|ConvertFrom-Json
 if(!$gate.recording_started -or $gate.pid -ne $child.pid -or ([DateTime]$gate.created_utc).ToUniversalTime().Ticks -ne ([DateTime][SuspendedChild]::Created($child)).ToUniversalTime().Ticks){throw 'Resume binding differs'}
 $count=[SuspendedChild]::ResumeThread($child.thread);if($count -ne 1){throw "Unexpected suspend count $count"}
 Save 'node-resumed-public.json' @{utc=[DateTime]::UtcNow.ToString('o');pid=$child.pid;prior_suspend_count=$count;run_seconds=7200;hard_seconds=7320;relay_ready=$false}
 $clock.Restart();$reason='natural_exit';$next=0
 while([SuspendedChild]::WaitForSingleObject($child.process,250) -ne 0){
  $trace=Join-Path $WorkRoot 'node-private.run';$bytes=if(Test-Path -LiteralPath $trace){(Get-Item -LiteralPath $trace).Length}else{0}
  $p=Get-Process -Id $child.pid
  if((Test-Path -LiteralPath (Join-Path $WorkRoot 'stop-request.json')) -or $clock.Elapsed.TotalSeconds -ge 7320 -or (Get-PSDrive D).Free -lt 12GB -or $p.PrivateMemorySize64 -gt 6GB -or $bytes -gt 2068MB){
   $reason=if(Test-Path -LiteralPath (Join-Path $WorkRoot 'stop-request.json')){'coordinator_stop'}elseif($clock.Elapsed.TotalSeconds -ge 7320){'time_bound'}elseif((Get-PSDrive D).Free -lt 12GB){'disk_bound'}elseif($p.PrivateMemorySize64 -gt 6GB){'memory_bound'}else{'trace_size_bound'}
   break
  }
  if($clock.Elapsed.TotalSeconds -ge $next){
   $l=Join-Path $WorkRoot 'liveness-public.json';@{utc=[DateTime]::UtcNow.ToString('o');pid=$child.pid;elapsed_seconds=$clock.Elapsed.TotalSeconds;trace_bytes=$bytes;ring_tail_only=$true;relay_ready=$false}|ConvertTo-Json|Set-Content -LiteralPath ($l+'.tmp');Move-Item -LiteralPath ($l+'.tmp') -Destination $l -Force;$next=$clock.Elapsed.TotalSeconds+10
  }
 }
 $ownerExit=0
}catch{Save 'owner-error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message;node_created=($null -ne $child);relay_ready=$false;root_cause_fixed=$false}}
finally{
 if($null -ne $child){
  try{
   if([SuspendedChild]::WaitForSingleObject($child.process,0) -ne 0){$forced=[SuspendedChild]::TerminateProcess($child.process,3758096385);if(!$forced -or [SuspendedChild]::WaitForSingleObject($child.process,15000) -ne 0){throw 'Owned kernel-handle exit unconfirmed'}}
   $code=[SuspendedChild]::Exit($child);$signed=[BitConverter]::ToInt32([BitConverter]::GetBytes($code),0)
   Save 'node-exit-public.json' @{utc=[DateTime]::UtcNow.ToString('o');pid=$child.pid;exit_code=$signed;exit_hex=('0x{0:x8}' -f $code);forced=$forced;reason=$reason;kernel_handle_exit=$true;relay_ready=$false;root_cause_fixed=$false}
  }catch{Save 'owner-cleanup-error-public.json' @{utc=[DateTime]::UtcNow.ToString('o');error=$_.Exception.Message};$ownerExit=1}
  [void][SuspendedChild]::CloseHandle($child.thread);[void][SuspendedChild]::CloseHandle($child.process)
 }
}
Save 'owner-terminal-public.json' @{utc=[DateTime]::UtcNow.ToString('o');exit_code=$ownerExit;relay_ready=$false;root_cause_fixed=$false}
exit $ownerExit
