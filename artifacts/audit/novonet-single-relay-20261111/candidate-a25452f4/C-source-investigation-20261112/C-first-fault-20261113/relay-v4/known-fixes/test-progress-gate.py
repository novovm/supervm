"""Exercise the real publisher scheduling statements with a deterministic clock.

No node, database, network or generated production data is used. The complete
node is checked separately; this reproduces the missing-field control flow.
"""
from pathlib import Path
import re,subprocess,json,hashlib,sys
base=Path(__file__).parent
source=Path('D:/WEB3_AI/SUPERVM/crates/novovm-node/src/bin/novovm-node.rs')
before=subprocess.check_output(['git','show','a25452f4:crates/novovm-node/src/bin/novovm-node.rs'],cwd=source.parents[4]).decode()
after=source.read_text()
root=base/(sys.argv[1] if len(sys.argv)>1 else 'progress-gate-regression-01');root.mkdir(exist_ok=False)
results=[]
for label,text in [('original_a254',before),('fixed_source',after)]:
 marker='let novonet_observation = if '
 start=text.index(marker)
 end=text.index('        if let Some(product) = udp_drive.as_mut()',start)
 observation=text[start:end]
 latch=''
 if 'let progress_report_due =' in text[:start]:
  m=list(re.finditer(r'let progress_report_due =[^;]+;',text[:start]))[-1]
  latch=m.group(0)
 guard=re.search(r'if ([^{}]+)\s*\{\s*use novovm_node::native_pipeline::service::rpc::\{observe_tick, TickStage\};',text[end:]).group(1).strip()
 insert=re.search(r'if let Some\(observation\) = novonet_observation \{[^}]+\}',text[end:]).group(0)
 code='''use std::{cell::Cell,ops::{Index,IndexMut},time::Duration};
struct Clock { first:u64,later:u64,reads:Cell<u32> }
impl Clock { fn elapsed(&self)->Duration {let n=self.reads.get();self.reads.set(n+1);Duration::from_millis(if n==0{self.first}else{self.later})} }
struct Network<'a>(&'a Cell<u32>);
impl Network<'_> {fn observation(&self)->bool{self.0.set(self.0.get()+1);true}}
struct Drive<'a>{novonet:Option<Network<'a>>}
#[derive(Default)] struct Status {novonet:bool}
impl Index<&str> for Status {type Output=bool;fn index(&self,k:&str)->&bool{assert_eq!(k,"novonet");&self.novonet}}
impl IndexMut<&str> for Status {fn index_mut(&mut self,k:&str)->&mut bool{assert_eq!(k,"novonet");&mut self.novonet}}
fn tick(first:u64,later:u64,network:bool)->(Option<Status>,u32){
let last_progress_report_at=Clock{first,later,reads:Cell::new(0)};
let progress_report_interval_ms=1000;
let calls=Cell::new(0);
let udp_drive=Some(Drive{novonet:network.then_some(Network(&calls))});
'''+latch+'\n'+observation+'\nif '+guard+' { let mut status=Status::default();\n'+insert+'\n(Some(status),calls.get())}else{(None,calls.get())}}\n'+'''
#[test] fn crossing_deadline_never_emits_partial_network_status(){
 let (crossing,calls)=tick(999,1000,true);
 assert!(crossing.as_ref().is_none_or(|s|s.novonet),"deadline crossed during poll published missing novonet");
 assert_eq!(calls,0,"not-due tick must not gather a costly network snapshot");
 let (next,calls)=tick(1000,1001,true);assert!(next.unwrap().novonet);assert_eq!(calls,1);
}
#[test] fn due_tick_contains_actual_network_observation(){let (s,n)=tick(1000,1100,true);assert!(s.unwrap().novonet);assert_eq!(n,1);}
#[test] fn early_tick_does_not_observe_or_emit(){let (s,n)=tick(0,999,true);assert!(s.is_none());assert_eq!(n,0);}
#[test] fn disabled_network_is_not_fabricated(){let (s,n)=tick(1000,1000,false);assert!(!s.unwrap().novonet);assert_eq!(n,0);}
'''
 rs=root/(label+'.rs');rs.write_text(code)
 exe=root/(label+'.exe')
 comp=subprocess.run(['rustc','--edition=2021','--test',str(rs),'-o',str(exe)],capture_output=True,text=True)
 (root/(label+'-compile.log')).write_text(comp.stdout+comp.stderr)
 if comp.returncode:raise RuntimeError(comp.stderr)
 run=subprocess.run([str(exe),'--nocapture'],capture_output=True,text=True)
 (root/(label+'-test.log')).write_text(run.stdout+run.stderr)
 results.append({'source':label,'compile_exit':comp.returncode,'test_exit':run.returncode,'source_sha256':hashlib.sha256(text.encode()).hexdigest(),'log':run.stdout+run.stderr})
assert results[0]['test_exit']!=0 and results[1]['test_exit']==0
report={'scope':'Actual publisher scheduling/assignment statements extracted from a254 and current node source, with deterministic crossing clock and minimal owner stubs. Not full node or network acceptance.', 'cases_per_source':4,'results':results,'node_started':False,'crash_root_cause_fixed':False}
(root/'result-public.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
