const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const hash = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
// A rejected DLL is deliberately removed, not retained as a runnable alternative.
// The immutable original measurements retain its observed digest; never report
// that historical value as a currently present artifact.
const candidatePath = 'target/validation/resident-layer-batch-20261014/candidate.dll';
const samples = [];
for (const backend of ['integer','cpu']) for (const variant of ['baseline','candidate']) for (const round of [1,2]) {
  const name = `${variant}-${backend}-${round}`;
  const r = JSON.parse(fs.readFileSync(path.join(__dirname,`${name}.json`)));
  if (!r.passed || r.counts.finalized_successful !== 65536 || r.counts.finalized_business_failed !== 0 || r.unresolved_submitted !== 0 || r.unique_permanently_rejected !== 0 || r.counts.transport_errors !== 0) throw Error(name);
  samples.push({name,backend,variant,round,tps:r.successful_tps,seconds:r.elapsed_seconds,
    p95_us:r.latency_us.p95,p99_us:r.latency_us.p99,counts:r.counts});
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
const drafts = JSON.parse(fs.readFileSync('artifacts/audit/resident-integer-device-20261013/validation.json')).preserved_drafts.files;
const drift = drafts.filter(f=>hash(f.path)!==f.sha256.toLowerCase()).map(f=>f.path);
if (drift.length) throw Error(`Draft drift ${drift}`);
console.log(JSON.stringify({samples,combined,hashes:{
  node:hash('target/release/novovm-node.exe'),
  driver:hash('target/validation/dataflow-c48dd7bd/novovm_node-54e8d64d0bc52c1f.exe'),
  tests:hash('target/release/deps/novovm_node-3fdab5433ef50d79.exe'),
  baseline:hash('target/validation/resident-layer-batch-20261014/baseline.dll'),
  candidate:fs.existsSync(candidatePath) ? hash(candidatePath) : null
},candidate_artifact_present:fs.existsSync(candidatePath),preserved_drafts:{count:drafts.length,drift}},null,2));
