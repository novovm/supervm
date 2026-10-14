const fs = require('fs');
const path = require('path');
const assert = require('assert/strict');
const contract = require('./contract.json');
const samples = contract.sequence.map(name => {
  const r = JSON.parse(fs.readFileSync(path.join(__dirname, `${name}.json`)));
  assert(r.passed && r.counts.finalized_successful === 65536);
  assert.equal(r.counts.finalized_business_failed, 0);
  assert.equal(r.unresolved_submitted, 0);
  assert.equal(r.unique_permanently_rejected, 0);
  assert.equal(r.execution_backend, 'cpu_callbacks');
  for (const s of r.evidence.live_status) {
    assert.equal(s.execution_integer_rows_total, 0);
    assert(s.execution_peak_callbacks <= 24);
  }
  const nodes = r.evidence.profile_end.map((end, i) => {
    const start = r.evidence.profile_start[i];
    assert(start.enabled && end.enabled);
    return Object.fromEntries(Object.keys(end.phases).map(k => [k, {
      calls: end.phases[k].calls - start.phases[k].calls,
      ms: (end.phases[k].wall_ns - start.phases[k].wall_ns) / 1e6
    }]));
  });
  return { name, tps:r.successful_tps, seconds:r.elapsed_seconds,
    p95_us:r.latency_us.p95, p99_us:r.latency_us.p99,
    observation_requests:r.counts.observation_requests,
    peak_callbacks:r.evidence.live_status.map(s=>s.execution_peak_callbacks), nodes };
});
const phases = Object.keys(samples[0].nodes[0]);
const comparisons = [0, 1].map(i => {
  const parent = samples.find(s=>s.name===`parent-profile-${i+1}`);
  const candidate = samples.find(s=>s.name===`candidate-profile-${i+1}`);
  const mean = (s,k)=>s.nodes.reduce((n,p)=>n+p[k].ms,0)/s.nodes.length;
  return { pair:i+1, tps_change_percent:100*(candidate.tps/parent.tps-1),
    mean_node_ms:Object.fromEntries(phases.map(k=>[k, {
      parent:mean(parent,k), candidate:mean(candidate,k),
      delta:mean(candidate,k)-mean(parent,k)
    }])) };
});
console.log(JSON.stringify({scope:contract.interpretation,samples,comparisons},null,2));
