const fs = require('fs');
const path = require('path');
const assert = require('assert/strict');
const samples = [];
const diagnostics = [];
for (const workers of [24, 6]) for (const round of [1, 2, 'diagnostic']) {
  const name = `workers${workers}-${round}`;
  const r = JSON.parse(fs.readFileSync(path.join(__dirname, `${name}.json`)));
  assert(r.passed && r.counts.finalized_successful === 65536);
  assert.equal(r.counts.finalized_business_failed, 0);
  assert.equal(r.unresolved_submitted, 0);
  assert.equal(r.unique_permanently_rejected, 0);
  assert.equal(r.execution_backend, 'cpu_callbacks');
  for (const s of r.evidence.live_status) {
    assert.equal(s.execution_integer_rows_total, 0);
    assert(s.execution_peak_callbacks <= workers);
  }
  const result = {name, workers, tps:r.successful_tps, seconds:r.elapsed_seconds,
    p95_us:r.latency_us.p95, p99_us:r.latency_us.p99,
    observation_requests:r.counts.observation_requests,
    peak_callbacks:r.evidence.live_status.map(s=>s.execution_peak_callbacks)};
  if (round === 'diagnostic') {
    result.nodes = r.evidence.profile_end.map((end,i)=> {
      const start = r.evidence.profile_start[i];
      assert(start.enabled && end.enabled);
      return Object.fromEntries(Object.keys(end.phases).map(k=>[k,{
        calls:end.phases[k].calls-start.phases[k].calls,
        ms:(end.phases[k].wall_ns-start.phases[k].wall_ns)/1e6
      }]));
    });
    diagnostics.push(result);
  } else samples.push(result);
}
const combined = Object.fromEntries([24,6].map(w=>[w,
  131072/samples.filter(s=>s.workers===w).reduce((n,s)=>n+s.seconds,0)]));
console.log(JSON.stringify({scope:'Same accepted binary, different explicit worker budgets. Not a new-library attribution or algorithm speedup. Diagnostic wall durations overlap and must not be summed.', samples, combined, diagnostics},null,2));
