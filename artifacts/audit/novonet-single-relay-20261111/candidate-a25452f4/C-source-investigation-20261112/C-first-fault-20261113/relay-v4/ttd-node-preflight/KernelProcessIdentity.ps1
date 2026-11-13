Add-Type -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices; using System.ComponentModel; using System.Security.Principal;
public static class KernelProcessIdentity {
 public class Facts {public uint pid;public string path,created_utc,owner_sid;}
 [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,uint pid);
 [DllImport("kernel32.dll",SetLastError=true,CharSet=CharSet.Unicode)] static extern bool QueryFullProcessImageName(IntPtr process,uint flags,StringBuilder path,ref uint size);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetProcessTimes(IntPtr process,out long created,out long exited,out long kernel,out long user);
 [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr process,uint access,out IntPtr token);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
 public static Facts Read(uint pid){IntPtr p=OpenProcess(0x1000,false,pid);if(p==IntPtr.Zero)throw new Win32Exception();IntPtr token=IntPtr.Zero;
  try{uint size=32768;var path=new StringBuilder((int)size);if(!QueryFullProcessImageName(p,0,path,ref size))throw new Win32Exception();long c,e,k,u;if(!GetProcessTimes(p,out c,out e,out k,out u))throw new Win32Exception();if(!OpenProcessToken(p,8,out token))throw new Win32Exception();using(var id=new WindowsIdentity(token)){return new Facts{pid=pid,path=path.ToString(),created_utc=DateTime.FromFileTimeUtc(c).ToString("o"),owner_sid=id.User.Value};}}
  finally{if(token!=IntPtr.Zero)CloseHandle(token);CloseHandle(p);}
 }
}
'@
