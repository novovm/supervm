const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const root = process.cwd();
const hash = p => crypto.createHash('sha256').update(fs.readFileSync(path.join(root,p))).digest('hex');
const samples = [];
for (const backend of ['integer','cpu']) for (const variant of ['baseline','candidate']) for (const round of [1,2]) {
  const name = `${variant}-${backend}-${round}`;
  const r = JSON.parse(fs.readFileSync(path.join(__dirname,`${name}.json`)));
  if (!r.passed || r.counts.finalized_successful !== 65536 || r.counts.finalized_business_failed !== 0 || r.unresolved_submitted !== 0) throw Error(`Incomplete ${name}`);
  samples.push({name,backend,variant,round,tps:r.successful_tps,seconds:r.elapsed_seconds,
    p95_us:r.latency_us.p95,p99_us:r.latency_us.p99,counts:r.counts,
    permanently_rejected:r.unique_permanently_rejected,unresolved:r.unresolved_submitted});
}
const combined = {};
for (const backend of ['integer','cpu']) {
  combined[backend] = {};
  for (const variant of ['baseline','candidate']) {
    const group = samples.filter(s=>s.backend===backend && s.variant===variant);
    combined[backend][variant] = 131072/group.reduce((s,r)=>s+r.seconds,0);
  }
  combined[backend].change_percent = 100*(combined[backend].candidate/combined[backend].baseline-1);
}
const drafts = JSON.parse(fs.readFileSync(path.join(root,'artifacts/audit/resident-integer-device-20261013/validation.json'))).preserved_drafts.files;
const drift = drafts.filter(f=>hash(f.path)!==f.sha256.toLowerCase()).map(f=>f.path);
if (drift.length) throw Error(`Draft drift ${drift}`);
const attribution = ['candidate-cpu-3','candidate-cpu-4','same-source-cpu-1','same-source-cpu-2','same-source-integer-1'].map(name=>{
  const r = JSON.parse(fs.readFileSync(path.join(__dirname,`${name}.json`)));
  if (!r.passed || r.counts.finalized_successful!==65536 || r.unresolved_submitted!==0) throw Error(name);
  return {name,tps:r.successful_tps,seconds:r.elapsed_seconds,p95_us:r.latency_us.p95,p99_us:r.latency_us.p99,counts:r.counts};
});
const final = ['final-cpu','final-integer'].map(name=>{
  const r = JSON.parse(fs.readFileSync(path.join(__dirname,`${name}.json`)));
  if (!r.passed || r.counts.finalized_successful!==65536 || r.counts.finalized_business_failed!==0 || r.unresolved_submitted!==0) throw Error(name);
  return {name,tps:r.successful_tps,seconds:r.elapsed_seconds,p95_us:r.latency_us.p95,p99_us:r.latency_us.p99,counts:r.counts};
});
console.log(JSON.stringify({samples,combined,attribution,final,hashes:{
  node:hash('target/release/novovm-node.exe'),
  driver:hash('target/validation/dataflow-c48dd7bd/novovm_node-54e8d64d0bc52c1f.exe'),
  tests:hash('target/release/deps/novovm_node-3fdab5433ef50d79.exe'),
  baseline_aoem:hash('target/validation/resident-exchange-20261014/aoem_ffi-baseline.dll'),
  candidate_aoem:hash('target/validation/resident-exchange-20261014/aoem_ffi-candidate.dll'),
  same_source_unpatched_aoem:hash('target/validation/resident-exchange-20261014/aoem_ffi-same-source-baseline.dll'),
  final_aoem:hash('target/validation/resident-exchange-20261014/aoem_ffi-final.dll'),
  packaged_aoem:hash('aoem/windows/core/bin/aoem_ffi.dll')
},preserved_drafts:{count:drafts.length,drift}},null,2));
