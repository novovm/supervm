// Existing product v10 phase spans only; separate from the eight formal samples.
const fs = require('fs');
const path = require('path');
const assert = require('assert/strict');
const output = {};
for (const variant of ['baseline', 'candidate']) {
  const r = JSON.parse(fs.readFileSync(path.join(__dirname, `${variant}-integer-diagnostic.json`)));
  assert(r.passed && r.counts.finalized_successful === 65536);
  assert(r.execution_backend === 'aoem_integer_v7');
  output[variant] = {tps: r.successful_tps, counts: r.counts,
    nodes: r.evidence.profile_end.map((end, i) => {
      const start = r.evidence.profile_start[i];
      assert(start.enabled && end.enabled);
      return Object.fromEntries(Object.keys(end.phases).map(key => [key, {
        calls: end.phases[key].calls - start.phases[key].calls,
        ms: (end.phases[key].wall_ns - start.phases[key].wall_ns) / 1e6
      }]));
    })};
}
console.log(JSON.stringify({scope:'Inclusive nested/concurrent wall time, not additive critical path. Actual v10, no legacy profiled integer row call. Diagnostic samples not promotion evidence.', ...output}, null, 2));
