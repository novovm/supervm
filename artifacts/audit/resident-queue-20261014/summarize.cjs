const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const assert = require('assert/strict');
const read = name => JSON.parse(fs.readFileSync(path.join(__dirname, name + '.json')));
const hash = p => crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const contract = read('contract');
function checked(name) {
  const r = read(name);
  assert(r.passed && r.counts.finalized_successful === 65536, name);
  assert.equal(r.counts.finalized_business_failed, 0);
  assert.equal(r.unresolved_submitted, 0);
  assert.equal(r.unique_permanently_rejected, 0);
  assert.equal(r.evidence.aoem_sha256, '0x' + contract.sdk_sha256);
  return r;
}
const samples = contract.formal_order.map(name => {
  const r = checked(name);
  assert(r.evidence.live_status.length === 4);
  assert(r.evidence.live_status.every(p => !p.stage_profile.enabled));
  return {name, tps:r.successful_tps, seconds:r.elapsed_seconds,
    p95_us:r.latency_us.p95, p99_us:r.latency_us.p99, counts:r.counts,
    node_sha256:r.evidence.binary_sha256};
});
const combined = {};
for (const backend of ['integer', 'cpu']) {
  combined[backend] = {};
  for (const variant of ['baseline', 'candidate']) {
    const group = samples.filter(s => s.name.startsWith(`${variant}-${backend}-`));
    combined[backend][variant] = 65536 * group.length / group.reduce((n,s) => n+s.seconds,0);
  }
  combined[backend].change_percent = 100*(combined[backend].candidate/combined[backend].baseline-1);
}
const diagnostics = {};
for (const name of ['baseline-integer-diagnostic', 'baseline-cpu-diagnostic', 'candidate-integer-diagnostic']) {
  const r = checked(name);
  diagnostics[name] = {tps:r.successful_tps, counts:r.counts,
    nodes:r.evidence.profile_end.map((end,i) => {
      const start = r.evidence.profile_start[i];
      assert(end.enabled && start.enabled);
      return Object.fromEntries(Object.keys(end.phases).map(k => [k, {
        calls:end.phases[k].calls-start.phases[k].calls,
        ms:(end.phases[k].wall_ns-start.phases[k].wall_ns)/1e6
      }]));
    })};
}
const drafts = JSON.parse(fs.readFileSync('artifacts/audit/resident-integer-device-20261013/validation.json')).preserved_drafts.files;
const drift = drafts.filter(f => hash(f.path) !== f.sha256.toLowerCase()).map(f=>f.path);
assert.equal(drift.length,0);
console.log(JSON.stringify({samples,combined,diagnostics,
  scope:contract.diagnostic_scope,
  hashes:{node:hash('target/release/novovm-node.exe'),
    driver:hash('target/validation/dataflow-c48dd7bd/novovm_node-54e8d64d0bc52c1f.exe'),
    tests:hash('target/release/deps/novovm_node-3fdab5433ef50d79.exe')},
  preserved_drafts:{count:drafts.length,drift}},null,2));
