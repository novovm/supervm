"""Avoid unresolved externs in dependency cdylibs; only instrument the node image.
This changes the diagnostic integration, not a production fix.
"""
from pathlib import Path
import hashlib,json,re
ROOT=Path('target/c-partitioned-writer-20261114');HERE=Path(__file__).parent
assert json.loads((ROOT/'instrumented/build-public.json').read_text())['exit']==101
(ROOT/'instrumented').rename(ROOT/'instrumented-link-failed')
changes=[]
for name in ['core/src/fmt/mod.rs','alloc/src/fmt.rs','alloc/src/string.rs']:
 p=ROOT/'library'/name;t=p.read_text(encoding='utf-8');before=hashlib.sha256(p.read_bytes()).hexdigest()
 pattern=r'unsafe \{ unsafe extern "C" \{fn novovm_diag_boundary\(k:u64,o:usize,b:usize,l:usize,c:usize,t:usize,n:usize,f:usize\);\} novovm_diag_boundary\(([^;]+)\); \}'
 t,n=re.subn(pattern,lambda m:('crate::fmt::' if name.startswith('core/') else 'core::fmt::')+'__novovm_diag_emit('+m.group(1)+');',t)
 assert n in (2,4),(name,n)
 if name.startswith('core/'):t+='\n'+(HERE/'std_hook.rs').read_text(encoding='utf-8')
 p.write_bytes(t.encode());changes.append(dict(path=name,before=before,after=hashlib.sha256(p.read_bytes()).hexdigest()))
p=ROOT/'library/alloc/src/lib.rs';t=p.read_text(encoding='utf-8');at=t.index('#![feature(');t=t[:at]+'#![feature(novovm_diag)]\n'+t[at:];p.write_bytes(t.encode())
p=ROOT/'source/crates/novovm-node/src/bin/c-lowimpact-capture.rs';t=p.read_text(encoding='utf-8')
t='#![cfg_attr(feature="lowtrace",feature(novovm_diag))]\n'+t
t=t.replace('fn main()->Result<()> {','fn main()->Result<()> {\n #[cfg(feature="lowtrace")] core::fmt::__novovm_diag_set_hook(novovm_diag_boundary);')
p.write_bytes(t.encode())
(ROOT/'callback-fix.json').write_bytes((json.dumps(changes,indent=2)+'\n').encode())
