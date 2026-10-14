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
  const expectedNode = name.startsWith('baseline-')
    ? '6c4c723573001e0ed4377d2d8cb15b3d12016bf186b6a6a86dc21ec1ebc3fd82'
    : '77a504eb520b66f66f9e8c21f90ea97ab72f517a41afd0ebfd539679dab2ff62';
  assert.equal(r.evidence.binary_sha256, '0x' + expectedNode);
  assert(fs.readFileSync(path.join(__dirname, name + '.log'), 'utf8').includes('power_after=Online'));
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
for (const name of ['baseline-integer-diagnostic', 'candidate-integer-diagnostic']) {
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
const summary = {samples,combined,diagnostics,
  scope:contract.scope,
  hashes:{node:hash('target/release/novovm-node.exe'),
    driver:hash('target/validation/dataflow-c48dd7bd/novovm_node-54e8d64d0bc52c1f.exe'),
    tests:hash('target/release/deps/novovm_node-3fdab5433ef50d79.exe')},
  preserved_drafts:{count:drafts.length,drift}};
fs.writeFileSync(path.join(__dirname, 'summary.json'), JSON.stringify(summary,null,2)+'\n');
const sourceFiles = [
  'crates/novovm-node/src/native_pipeline/pipeline/compute.rs',
  'crates/novovm-node/src/native_pipeline/pipeline/compute/tests.rs',
  'crates/novovm-node/src/native_pipeline/pipeline/compute/lanes.rs',
  'crates/novovm-node/src/native_pipeline/pipeline/compute/lanes/tests.rs',
  'crates/novovm-node/src/native_pipeline/business/nov_transfer_batch/execution.rs',
  'crates/novovm-node/src/native_pipeline/business/nov_transfer_batch/integer.rs',
  'crates/novovm-relay/src/rendezvous_self_owned_owner.rs'
];
const pipeline = fs.readFileSync(path.join(__dirname,'pipeline-tests.log'),'utf8');
assert(pipeline.includes('402 passed; 0 failed; 63 ignored'));
const validation = {
  schema:'resident-overlap-20261014-v1', source:'0f7babce20a5231ac4e5891b19cb491f38f31f2f',
  summary:'summary.json', hashes:summary.hashes,
  sdk_sha256:contract.sdk_sha256, sdk_changed:false, extra_native_context:false,
  cpu_default_unchanged:true, cpu_algorithm_unchanged:true, cpu_queue_wrapper_changed:true,
  gpu_device_residency_changed:false, stable_tps_improvement_accepted:false,
  production_accepted:false, independent_bigint_oracle_rerun:false,
  source_files:sourceFiles.map(p=>({path:p,sha256:hash(p)})),
  preserved_drafts:summary.preserved_drafts,
  explicit_regressions:read('regressions'),
  pipeline:pipeline.split(/\r?\n/).find(l=>l.startsWith('test result:')),
  clippy:{command:'cargo clippy --release --locked --target-dir target -p novovm-node --lib -- -D warnings',passed:true,log_sha256:hash(path.join(__dirname,'clippy.log'))},
  fmt:{command:'cargo fmt --all -- --check',passed:true},
  input_main_ci:{sha:'0f7babce20a5231ac4e5891b19cb491f38f31f2f',run:38547158541,conclusion:'failure',scope:'relay formatting; mainchain integration not reached; formatting corrected without logic change'},
  delivery_ci:'pending push; not verified',
  known_failures:['Initial test build used AdmissionRow.checked instead of result; corrected; failed build log retained'],
  limitations:['Local Windows only; four processes are not four physical devices','Fixed integer-lane hold proves independent authentication progress, not simultaneous hardware utilization','Shared panic latch does not replace native resource error poisoning','No GPU algorithm or proof capability improvement claimed']
};
fs.writeFileSync(path.join(__dirname,'validation.json'), JSON.stringify(validation,null,2)+'\n');
console.log(JSON.stringify({combined,samples,diagnostics,preserved_drafts:summary.preserved_drafts},null,2));
