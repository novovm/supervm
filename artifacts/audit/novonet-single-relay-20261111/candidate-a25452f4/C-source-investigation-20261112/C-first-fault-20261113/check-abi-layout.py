from pathlib import Path
import subprocess,re,json,hashlib
b=Path(__file__).parent
def blob(repo,rev,path):
 return subprocess.check_output(['git','show',f'{rev}:{path}'],cwd=repo).decode('utf-8')
h=blob('D:/WEB3_AI/SUPERVM','a25452f4','crates/aoem-bindings/src/lib.rs')
d=blob('D:/WEB3_AI/AOEM','9662470d','crates/ffi/aoem-ffi/src/lib.rs')
names=['AoemCreateOptionsV1','AoemOpV2','AoemExecV2Result','AoemMldsaVerifyItemV1']
defs={}
for side,source in [('host',h),('runtime',d)]:
 defs[side]={name:re.search(r'pub struct '+name+r'\s*\{[^}]+\}',source).group(0) for name in names}
code='\n'.join('mod '+side+' {\n'+ '\n'.join('#[repr(C)]\n'+v for v in rows.values())+'\n}' for side,rows in defs.items())
code+='\nfn main(){\n'
for name in names:
 fields=re.findall(r'pub (\w+)\s*:',defs['host'][name])
 code+=f'assert_eq!(std::mem::size_of::<host::{name}>(), std::mem::size_of::<runtime::{name}>());\n'
 code+=f'assert_eq!(std::mem::align_of::<host::{name}>(), std::mem::align_of::<runtime::{name}>());\n'
 for field in fields:
  code+=f'assert_eq!(std::mem::offset_of!(host::{name}, {field}), std::mem::offset_of!(runtime::{name}, {field}));\n'
 code+=f'println!("{name} size={{}} align={{}} fields={len(fields)}",std::mem::size_of::<host::{name}>(),std::mem::align_of::<host::{name}>());\n'
code+='}\n'
rs=b/'actual-abi-layout.rs';rs.write_text(code)
exe=b/'actual-abi-layout.exe'
comp=subprocess.run(['rustc','--edition=2021',str(rs),'-o',str(exe)],capture_output=True,text=True)
(b/'abi-compile-local.log').write_text(comp.stdout+comp.stderr)
if comp.returncode: raise SystemExit(comp.returncode)
run=subprocess.run([str(exe)],capture_output=True,text=True)
result={'host_revision':'a25452f4','runtime_revision':'9662470d','scope':'Four actual FFI descriptors only; size/alignment/field offsets on this Windows x86_64 compiler. Does not prove pointer lifetimes or runtime memory safety.', 'compiler':subprocess.check_output(['rustc','-V']).decode().strip(),'compile_exit':comp.returncode,'test_exit':run.returncode,'output':run.stdout,'source_sha256':hashlib.sha256(rs.read_bytes()).hexdigest(),'root_cause_fixed':False}
(b/'abi-layout-public.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
raise SystemExit(run.returncode)
