"""Apply diagnostic-only hooks to a NEW exported a254 tree, never the main checkout."""
from pathlib import Path
import json,hashlib,subprocess
ROOT=Path('target/c-joint-network-capture-20261114').resolve()
SRC=ROOT/'source'
HERE=Path(__file__).resolve().parent
changes=[]
def change(path,old,new,count=1):
    p=SRC/path; text=p.read_text(encoding='utf-8')
    assert text.count(old)==count,(path,'anchor count')
    original=hashlib.sha256(p.read_bytes()).hexdigest()
    p.write_bytes(text.replace(old,new).encode())
    changes.append({'path':path,'before_sha256':original,'after_sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
assert not (ROOT/'prepared.json').exists()
p=SRC/'crates/novovm-network/src/diagnostic_capture.rs'
p.write_bytes((HERE/'diagnostic_capture.rs').read_bytes())
lib=SRC/'crates/novovm-network/src/lib.rs'
original=hashlib.sha256(lib.read_bytes()).hexdigest()
with lib.open('ab') as f:f.write(b'\npub mod diagnostic_capture;\n')
changes.append({'path':'crates/novovm-network/src/lib.rs','before_sha256':original,'after_sha256':hashlib.sha256(lib.read_bytes()).hexdigest()})
change('crates/novovm-network/src/transport.rs','let packet = scratch[..n].to_vec();','let packet = scratch[..n].to_vec();\n                    crate::diagnostic_capture::record(1,0,u64::from(src.port()),&packet);',2)
change('crates/novovm-node/src/native_pipeline/consensus/native.rs','        profile_asset_message("asset_prepared", message, object.id());','        novovm_network::diagnostic_capture::record(2,Arc::as_ptr(&object) as usize,Arc::strong_count(&object) as u64,&object.id());\n        profile_asset_message("asset_prepared", message, object.id());')
change('crates/novovm-network/src/novorudp/fragments.rs','impl OutgoingMessage {','impl Drop for OutgoingMessage { fn drop(&mut self) { crate::diagnostic_capture::record(3,self as *const Self as usize,0,&self.id); } }\n\nimpl OutgoingMessage {')
change('crates/novovm-network/src/novorudp/fragments.rs','                self.remember_completion(key.clone(), entry.length, now);','                crate::diagnostic_capture::record(4,0,entry.length as u64,&key.1);\n                self.remember_completion(key.clone(), entry.length, now);')
change('crates/novovm-node/src/native_pipeline/profile.rs','        Value::String(bytes.iter().map(|byte| format!("{byte:02x}")).collect())','        novovm_network::diagnostic_capture::joint_hash_begin();\n        let result = Value::String(bytes.iter().map(|byte| format!("{byte:02x}")).collect());\n        novovm_network::diagnostic_capture::joint_hash_end();\n        result')
node=SRC/'crates/novovm-node'
(node/'src/bin/c-joint-network-capture.rs').write_bytes((HERE/'c-joint-network-capture.rs').read_bytes())
with (node/'Cargo.toml').open('ab') as f:f.write(b'\n[[bin]]\nname="c-joint-network-capture"\npath="src/bin/c-joint-network-capture.rs"\n')
(ROOT/'prepared.json').write_bytes((json.dumps({'source_commit':'a25452f4c49be37cd984e650f2079642e6c273b3','diagnostic_only':True,'changes':changes},indent=2)+'\n').encode())
print('Diagnostic source prepared; no node started')
