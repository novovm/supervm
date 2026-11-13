use std::{cell::Cell,ops::{Index,IndexMut},time::Duration};
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
let progress_report_due =
            last_progress_report_at.elapsed() >= Duration::from_millis(progress_report_interval_ms);
let novonet_observation = if progress_report_due {
            udp_drive
                .as_ref()
                .and_then(|drive| drive.novonet.as_ref())
                .map(|network| network.observation())
        } else {
            None
        };

if progress_report_due { let mut status=Status::default();
if let Some(observation) = novonet_observation {
                    status["novonet"] = observation;
                }
(Some(status),calls.get())}else{(None,calls.get())}}

#[test] fn crossing_deadline_never_emits_partial_network_status(){
 let (crossing,calls)=tick(999,1000,true);
 assert!(crossing.as_ref().is_none_or(|s|s.novonet),"deadline crossed during poll published missing novonet");
 assert_eq!(calls,0,"not-due tick must not gather a costly network snapshot");
 let (next,calls)=tick(1000,1001,true);assert!(next.unwrap().novonet);assert_eq!(calls,1);
}
#[test] fn due_tick_contains_actual_network_observation(){let (s,n)=tick(1000,1100,true);assert!(s.unwrap().novonet);assert_eq!(n,1);}
#[test] fn early_tick_does_not_observe_or_emit(){let (s,n)=tick(0,999,true);assert!(s.is_none());assert_eq!(n,0);}
#[test] fn disabled_network_is_not_fabricated(){let (s,n)=tick(1000,1000,false);assert!(!s.unwrap().novonet);assert_eq!(n,0);}
