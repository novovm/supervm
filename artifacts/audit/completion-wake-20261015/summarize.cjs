const fs = require('fs');
const path = require('path');
const assert = require('assert/strict');
const samples = require('./contract.json').sequence.map(name => {
  const r = JSON.parse(fs.readFileSync(path.join(__dirname, `${name}.json`)));
  assert(r.passed && r.counts.finalized_successful === 65536);
  assert.equal(r.counts.finalized_business_failed, 0);
  assert.equal(r.unique_permanently_rejected, 0);
  assert.equal(r.unresolved_submitted, 0);
  assert.equal(r.execution_backend, 'cpu_callbacks');
  return {name,tps:r.successful_tps,seconds:r.elapsed_seconds,
    p95_us:r.latency_us.p95,p99_us:r.latency_us.p99,
    observation_requests:r.counts.observation_requests};
});
const combined = Object.fromEntries(['parent','candidate'].map(label =>
  [label,131072/samples.filter(s=>s.name.startsWith(label)).reduce((n,s)=>n+s.seconds,0)]));
console.log(JSON.stringify({samples,combined,
  change_percent:100*(combined.candidate/combined.parent-1),
  scope:'Same CPU load and SDK, diagnostics off; four-node final receipts with economics/history/cold checks. Not GPU, proof or production capacity.'},null,2));
