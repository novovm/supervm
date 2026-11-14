from pathlib import Path
import hashlib,json,shutil,subprocess
HERE=Path(__file__).resolve().parent
ROOT=Path('target/c-partitioned-writer-20261114').resolve()
OLD=HERE.parent/'C-lowimpact-capture-20261114'
assert not (ROOT/'source').exists()
subprocess.run(['git','archive','a25452f4','-o',str(ROOT/'source.zip')],check=True)
script=(OLD/'prepare.py').read_text(encoding='utf-8')
script=script.replace("HERE=Path(__file__).resolve().parent",f'HERE=Path({str(OLD)!r})')
script=script.replace("target/c-lowimpact-capture-20261114","target/c-partitioned-writer-20261114")
# Reuse the reviewed original-network harness; only isolated export changes below.
exec(compile(script,str(OLD/'prepare.py'),'exec'),{})
SRC=ROOT/'source';net=SRC/'crates/novovm-network';node=SRC/'crates/novovm-node'
(net/'src/lowtrace.rs').write_bytes((HERE/'lowtrace.rs').read_bytes())
p=node/'src/native_pipeline/profile.rs';t=p.read_text(encoding='utf-8')
start=t.index('            novovm_network::lowtrace::record(10,10,0,byte')
end=t.index('            text\n        }).collect())',start)
t=t[:start]+'''            let _scope=novovm_network::lowtrace::target();
            let text=format!("{byte:02x}");
'''+t[end:];p.write_bytes(t.encode())
p=node/'src/bin/c-lowimpact-capture.rs';t=p.read_text(encoding='utf-8')
t='#![cfg_attr(feature="lowtrace",feature(novovm_diag))]\n'+t
t=t.replace('fn main()->Result<()> {','fn main()->Result<()> {\n #[cfg(feature="lowtrace")] core::fmt::__novovm_diag_set_hook(novovm_diag_boundary);')
t=t.replace('let seq=query_ns.len() as u64+1;lowtrace::record','let seq=query_ns.len() as u64+1;lowtrace::query(seq);lowtrace::record')
t=t.replace('next_query+=std::time::Duration::from_secs(10);','lowtrace::query(0);next_query+=std::time::Duration::from_secs(10);')
# Stop target capture before final summary; do not let a shutdown-only snapshot
# replace the measured workload's boundary history. The actual status call stays.
t=t.replace('let final_status=product.status();','lowtrace::query(0);let final_status=product.status();')
t+='''
#[no_mangle]
pub extern "C" fn novovm_diag_boundary(kind:u64,object:usize,buffer:usize,len:usize,capacity:usize,text:usize,text_len:usize,formatter:usize) {
 lowtrace::boundary(kind,object,buffer,len,capacity,text,text_len,formatter);
}
'''
p.write_bytes(t.encode())
sysroot=Path(subprocess.check_output(['rustc','+1.94.0','--print','sysroot'],text=True).strip())
std_original=sysroot/'lib/rustlib/src/rust/library';std=ROOT/'library'
shutil.copytree(std_original,std)
changes=[]
def change(path,old,new,n=1):
 p=std/path;t=p.read_text(encoding='utf-8');assert t.count(old)==n,(path,old,t.count(old))
 before=hashlib.sha256(p.read_bytes()).hexdigest();p.write_bytes(t.replace(old,new).encode())
 changes.append(dict(path=path,original_sha256=before,diagnostic_sha256=hashlib.sha256(p.read_bytes()).hexdigest()))
def hook(kind,obj,buf='0',length='0',cap='0',txt='0',tl='0',formatter='0'):
 prefix='crate::fmt::' if kind in (102,103) else 'core::fmt::'
 return f'{prefix}__novovm_diag_emit({kind},{obj},{buf},{length},{cap},{txt},{tl},{formatter});'
change('alloc/src/fmt.rs','        let mut output = string::String::with_capacity(capacity);',
 '        let mut output = string::String::with_capacity(capacity);\n'+hook(101,'&mut output as *mut string::String as usize','output.as_ptr() as usize','output.len()','output.capacity()'))
change('alloc/src/fmt.rs','        output\n    }',hook(106,'&mut output as *mut string::String as usize','output.as_ptr() as usize','output.len()','output.capacity()')+'\n        output\n    }')
change('alloc/src/string.rs','    fn write_str(&mut self, s: &str) -> fmt::Result {\n        self.push_str(s);',
 '    fn write_str(&mut self, s: &str) -> fmt::Result {\n'+hook(104,'self as *mut Self as usize',txt='s.as_ptr() as usize',tl='s.len()')+'\n'+hook(105,'self as *mut Self as usize','self.as_ptr() as usize','self.len()','self.capacity()','s.as_ptr() as usize','s.len()')+'\n        self.push_str(s);')
change('core/src/fmt/mod.rs',"pub fn write(output: &mut dyn Write, fmt: Arguments<'_>) -> Result {", "pub fn write(output: &mut dyn Write, fmt: Arguments<'_>) -> Result {\n"+hook(102,'output as *mut dyn Write as *mut () as usize'))
# Capture direct receiver and text immediately before each write in pad_integral.
p=std/'core/src/fmt/mod.rs';t=p.read_text(encoding='utf-8');start=t.index('    pub fn pad_integral(');end=t.index('    /// Takes a string slice',start)
part=t[start:end];assert part.count('self.buf.write_str(buf)')==3
call=hook(103,'self.buf as *mut dyn Write as *mut () as usize',txt='buf.as_ptr() as usize',tl='buf.len()',formatter='self as *mut Self as usize')
new=part.replace('self.buf.write_str(buf)',call+'\n            self.buf.write_str(buf)')
change('core/src/fmt/mod.rs',part,new)
with (std/'core/src/fmt/mod.rs').open('ab') as f:f.write(b'\n'+(HERE/'std_hook.rs').read_bytes())
p=std/'alloc/src/lib.rs';t=p.read_text(encoding='utf-8');at=t.index('#![feature(')
p.write_bytes((t[:at]+'#![feature(novovm_diag)]\n'+t[at:]).encode())
(ROOT/'std-patches.json').write_bytes((json.dumps(changes,indent=2)+'\n').encode())
print('Independent source/std copies prepared; installed toolchain and production crates untouched')
