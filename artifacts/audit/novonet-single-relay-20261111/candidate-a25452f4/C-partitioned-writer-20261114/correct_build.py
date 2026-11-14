"""Preserve failed build, apply edition fix only to isolated source copies."""
from pathlib import Path
import hashlib,json
ROOT=Path('target/c-partitioned-writer-20261114')
assert json.loads((ROOT/'instrumented/build-public.json').read_text())['exit']==101
(ROOT/'instrumented').rename(ROOT/'instrumented-edition-failed')
changes=[]
for path in ['core/src/fmt/mod.rs','alloc/src/fmt.rs','alloc/src/string.rs']:
 p=ROOT/'library'/path;t=p.read_text(encoding='utf-8');assert 'unsafe { extern "C"' in t
 before=hashlib.sha256(p.read_bytes()).hexdigest();p.write_bytes(t.replace('unsafe { extern "C"','unsafe { unsafe extern "C"').encode())
 changes.append(dict(path=path,before=before,after=hashlib.sha256(p.read_bytes()).hexdigest()))
p=ROOT/'source/crates/novovm-node/src/bin/c-lowimpact-capture.rs';t=p.read_text(encoding='utf-8')
assert t.count('next_query+=std::time::Duration::from_secs(10);')==1
p.write_bytes(t.replace('next_query+=std::time::Duration::from_secs(10);','lowtrace::query(0);next_query+=std::time::Duration::from_secs(10);').encode())
(ROOT/'edition-fix.json').write_bytes((json.dumps(changes,indent=2)+'\n').encode())
