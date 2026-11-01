// Fixed-capture offline CSwitch/ReadyThread reduction; never starts a node.
const fs = require('node:fs');
const path = require('node:path');
const base = path.resolve(process.argv[2]);
const profiles = [{round:2,backend:'cpu',pid:31004,tid:19464},{round:3,backend:'gpu_required',pid:24928,tid:3800}];
const low=300000, high=5200000;
const output=[];
for(const p of profiles){
  const lines=fs.readFileSync(path.join(base,`schedule-round-${p.round}-dump.csv`),'utf8').split(/\r?\n/);
  let state='Unknown',reason='',last=0;
  const sums={Running:0,Ready:0,Waiting:0,Unknown:0},reasons={},periods={Running:[],Ready:[],Waiting:[]},anomalies=[],names=new Map();
  let switchesIn=0,switchesOut=0,readyCount=0,lastEventTime=0,firstReliable=null;
  const procId=s=>Number(((s||'').match(/\(\s*(-?\d+)\)/)||[])[1]);
  const move=(time,next,nextReason)=>{
    if(time<last){anomalies.push({kind:'backward',time,last});return;}
    const begin=Math.max(last,low),end=Math.min(time,high);
    if(end>begin){
      sums[state]=(sums[state]||0)+end-begin;
      if(periods[state])periods[state].push({us:end-begin,begin,end,reason});
      if(state==='Waiting')reasons[reason]=(reasons[reason]||0)+end-begin;
    }
    last=time;state=next;reason=nextReason||'';
  };
  for(const line of lines){
    const c=line.split(',').map(s=>s.trim());
    if(c[0]==='FirstReliableCSwitchEventTimeStamp'&&/^\d+$/.test(c[1])){firstReliable=Number(c[1]);continue;}
    if(c[0]==='T-DCStart'&&procId(c[2])===p.pid)names.set(Number(c[3]),c.at(-1));
    if(c[0]==='CSwitch'&&c.length>16&&/^\d+$/.test(c[1])){
      const t=Number(c[1]);lastEventTime=Math.max(lastEventTime,t);
      if(procId(c[8])===p.pid&&Number(c[9])===p.tid){
        switchesOut++;
        if(state!=='Running'&&state!=='Unknown'){
          anomalies.push({kind:'out_nonrunning_interval_unclassified',time:t,state,since:last});
          state='Unknown';
        }
        const next=c[12]==='Waiting'?'Waiting':(['Ready','DeferredReady','Standby'].includes(c[12])?'Ready':'Unknown');
        move(t,next,c[13]);
      }
      if(procId(c[2])===p.pid&&Number(c[3])===p.tid){
        switchesIn++;
        if(state==='Waiting')anomalies.push({kind:'in_from_wait_without_ready',time:t,reason});
        move(t,'Running');
      }
    }else if(c[0]==='ReadyThread'&&/^\d+$/.test(c[1])&&procId(c[4])===p.pid&&Number(c[5])===p.tid){
      const t=Number(c[1]);readyCount++;
      if(state==='Running')anomalies.push({kind:'ready_before_switchout_ignored_until_out',time:t});
      if(state!=='Ready'&&state!=='Running')move(t,'Ready');
    }
  }
  if(last<high)move(high,state,reason);
  if(firstReliable>=low)throw Error('Invalid reliable range');
  if(Object.values(sums).reduce((a,b)=>a+b,0)!==high-low)throw Error('Incomplete slice');
  if(names.get(p.tid)!=='novovm-native-tick')throw Error('Wrong target thread');
  const stats={};
  for(const [key,arr] of Object.entries(periods)){
    const sorted=arr.map(p=>p.us).sort((a,b)=>a-b),q=f=>sorted[Math.max(0,Math.ceil(sorted.length*f)-1)]||0;
    stats[key]={count:sorted.length,p50_us:q(.5),p95_us:q(.95),p99_us:q(.99),max_us:sorted.at(-1)||0};
  }
  const util=fs.readFileSync(path.join(base,`schedule-round-${p.round}-util.txt`),'utf8').split(/\r?\n/).map(x=>x.split(',').map(s=>s.trim())).filter(c=>procId(c[1])===p.pid);
  const independent=util.find(c=>Number(c[2])===p.tid);
  output.push({...p,thread_name:names.get(p.tid),range_us:[low,high],duration_us:high-low,first_reliable_cswitch_us:firstReliable,last_cswitch_us:lastEventTime,
    state_us:sums,state_percent:Object.fromEntries(Object.entries(sums).map(([k,v])=>[k,v/(high-low)*100])),waiting_reason_us:reasons,intervals:stats,switchesIn,switchesOut,readyCount,anomalies,
    longest_waiting:periods.Waiting.sort((a,b)=>b.us-a.us).slice(0,10),longest_ready:periods.Ready.sort((a,b)=>b.us-a.us).slice(0,10),
    independent_xperf_running_us:Number(independent[0]),
    entry_thread_cpu_util_top:util.map(c=>({tid:Number(c[2]),thread_name:names.get(Number(c[2]))||'UnresolvedName',running_us:Number(c[0])})).sort((a,b)=>b.running_us-a.running_us).slice(0,10),
    named_receiver_threads:[...names.entries()].filter(([tid,name])=>/recv|receiv|udp|rudp|send/i.test(name)).map(([tid,name])=>({tid,name})),
    scope:'4.9s interior slice after both FirstReliableCSwitch timestamps; context-switch wall intervals, not CPU useful work. Missing-pair interval left Unknown; ReadyThread racing before switchout does not imply on-CPU waiting. Waiting reason is scheduler classification, not wait callsite. DPC/ISR not excluded; no corresponding events captured. Leading/trailing clip makes interval maxima bounded, not full lifetimes.'});
}
fs.writeFileSync(path.join(base,'schedule-attribution.json'),JSON.stringify(output,null,2)+'\n');
console.log(JSON.stringify(output.map(s=>({round:s.round,pid:s.pid,tid:s.tid,state_us:s.state_us}))));
