from pathlib import Path
import hashlib,json,zipfile
HERE=Path(__file__).resolve().parent
ROOT=Path('target/c-lowimpact-capture-20261114').resolve();SRC=ROOT/'source'
assert not SRC.exists()
with zipfile.ZipFile(ROOT/'source.zip') as z:
 for n in z.namelist():assert (SRC/n).resolve().is_relative_to(SRC)
 z.extractall(SRC)
changes=[]
def change(path,old,new,n=1):
 p=SRC/path;t=p.read_text(encoding='utf-8');assert t.count(old)==n,(path,old)
 before=hashlib.sha256(p.read_bytes()).hexdigest();p.write_bytes(t.replace(old,new).encode())
 changes.append({'path':path,'before':before,'after':hashlib.sha256(p.read_bytes()).hexdigest()})
net='crates/novovm-network/'
node='crates/novovm-node/'
(SRC/net/'src/lowtrace.rs').write_bytes((HERE/'lowtrace.rs').read_bytes())
with (SRC/net/'src/lib.rs').open('ab') as f:f.write(b'\npub mod lowtrace;\n')
change(net+'Cargo.toml','[features]','[features]\nlowtrace = []')
change(node+'Cargo.toml','[features]','[features]\nlowtrace = ["novovm-network/lowtrace"]')
change(net+'src/transport.rs','let packet = scratch[..n].to_vec();','let packet = scratch[..n].to_vec();\n crate::lowtrace::record(1,1,0,0,0,packet.len(),0,&packet[..packet.len().min(32)],u64::from(src.port()),0);',2)
f=net+'src/novorudp/fragments.rs'
change(f,'pub struct OutgoingMessage {','pub struct OutgoingMessage {\n #[cfg(feature="lowtrace")] diagnostic_generation:u64,')
change(f,'impl OutgoingMessage {','''#[cfg(feature="lowtrace")]
impl Drop for OutgoingMessage {fn drop(&mut self){crate::lowtrace::record(3,3,0,self.bytes.as_ptr() as usize,self.diagnostic_generation,self.bytes.len(),self.bytes.capacity(),&self.id,0,0);}}
impl OutgoingMessage {
 pub fn diagnostic_generation(&self)->u64 {#[cfg(feature="lowtrace")] {self.diagnostic_generation} #[cfg(not(feature="lowtrace"))] {0}}
 pub fn diagnostic_buffer(&self)->usize {self.bytes.as_ptr() as usize}''')
change(f,'        Ok(Self {\n            profile,\n            domain,\n            id: hash.finalize().into(),','        Ok(Self {\n            #[cfg(feature="lowtrace")] diagnostic_generation:crate::lowtrace::generation(),\n            profile,\n            domain,\n            id: hash.finalize().into(),')
change(node+'src/native_pipeline/consensus/native.rs','        profile_asset_message("asset_prepared", message, object.id());','        novovm_network::lowtrace::record(2,2,0,object.diagnostic_buffer(),object.diagnostic_generation(),object.byte_len(),0,&object.id(),Arc::strong_count(&object) as u64,0);\n        profile_asset_message("asset_prepared", message, object.id());')
change(f,'                self.remember_completion(key.clone(), entry.length, now);','                crate::lowtrace::record(4,4,0,0,0,entry.length,0,&key.1,0,0);\n                self.remember_completion(key.clone(), entry.length, now);')
p=node+'src/native_pipeline/profile.rs'
change(p,'Value::String(bytes.iter().map(|byte| format!("{byte:02x}")).collect())','''Value::String(bytes.iter().map(|byte| {
            novovm_network::lowtrace::record(10,10,0,byte as *const u8 as usize,0,1,0,&[],u64::from(*byte),0);
            let text=format!("{byte:02x}");
            novovm_network::lowtrace::record(11,11,0,&text as *const String as usize,0,text.len(),text.capacity(),&[],text.as_ptr() as u64,u64::from(*byte));
            text
        }).collect())''')
# Trace existing probe events without string formatting in the recorder.
p=node+'src/native_pipeline/consensus/native_lifecycle/probe.rs'
change(p,'        profile::delivery_event(event, 0, peer, object, 0);','''        let kind=match event {"probe_object_done"=>20,"probe_received_integrity_verified"=>21,_=>22};
        novovm_network::lowtrace::record(kind,20,0,0,0,0,0,&object,0,0);
        profile::delivery_event(event, 0, peer, object, 0);''')
old=HERE.parent/'C-isolated-network-capture-20261114/c-joint-network-capture.rs'
base=old.read_text(encoding='utf-8')
prepare=base[base.index('fn prepare('):base.index('fn run(')]
policy=base[base.index('fn policy()'):]
imports=base[:base.index('#[allow(unsafe_code)]')].replace('#![deny(unsafe_code)]','')
imports+='\nuse novovm_node::native_pipeline::{service::network::{Driver,Config},consensus::{transport::NetworkProbe,native_lifecycle::probe::ProbeCommand,wire::Validator}};\nuse novovm_network::lowtrace;\n'
imports+='\nextern "system" {fn GetCurrentThreadId()->u32;}\n'
harness=(HERE/'harness-body.rs').read_text(encoding='utf-8')
final=imports+prepare+harness+policy
(SRC/node/'src/bin/c-lowimpact-capture.rs').write_bytes(final.encode())
with (SRC/node/'Cargo.toml').open('ab') as f:f.write(b'\n[[bin]]\nname="c-lowimpact-capture"\npath="src/bin/c-lowimpact-capture.rs"\n')
(ROOT/'patches.json').write_bytes((json.dumps(changes,indent=2)+'\n').encode())
print('Fresh diagnostic export prepared')
