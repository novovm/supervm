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
  const variant = name.split('-')[0];
  const expectedNode = hash(variant === 'preoverlap'
    ? 'target/validation/resident-overlap-20261014/baseline-node.exe'
    : `target/validation/resident-cpu-queue-20261014/${variant}-node.exe`);
  assert.equal(r.evidence.binary_sha256, '0x' + expectedNode);
  const log = fs.readFileSync(path.join(__dirname, name + '.log'), 'utf8');
  assert(log.includes('power_after=Online'));
  const rawReport = log.split(/\r?\n/).find(l=>l.startsWith('real RPC load report=')).slice(21);
  const rawDir = path.dirname(rawReport);
  const eventsFile = name + '.events.jsonl';
  fs.copyFileSync(path.join(rawDir,'events.jsonl'),path.join(__dirname,eventsFile));
  const observations = path.join(rawDir,'observations.json');
  return {name, tps:r.successful_tps, seconds:r.elapsed_seconds,
    p95_us:r.latency_us.p95, p99_us:r.latency_us.p99, counts:r.counts,
    node_sha256:r.evidence.binary_sha256,
    events_file:eventsFile,
    local_observations:{path:path.relative(process.cwd(),observations),bytes:fs.statSync(observations).size,sha256:hash(observations)}};
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
const controls = samples.filter(s=>s.name.startsWith('preoverlap-'));
const preoverlapCpu = 65536*controls.length/controls.reduce((n,s)=>n+s.seconds,0);
const controlComparison = {preoverlap_cpu_tps:preoverlapCpu,
  overlap_baseline_vs_preoverlap_percent:100*(combined.cpu.baseline/preoverlapCpu-1),
  candidate_vs_preoverlap_percent:100*(combined.cpu.candidate/preoverlapCpu-1)};
const drafts = JSON.parse(fs.readFileSync('artifacts/audit/resident-integer-device-20261013/validation.json')).preserved_drafts.files;
const drift = drafts.filter(f => hash(f.path) !== f.sha256.toLowerCase()).map(f=>f.path);
assert.equal(drift.length,0);
const summary = {samples,combined,controlComparison,
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
  'crates/novovm-node/src/native_pipeline/pipeline/compute/lanes/tests.rs'
];
const pipeline = fs.readFileSync(path.join(__dirname,'pipeline-tests.log'),'utf8');
assert(pipeline.includes('403 passed; 0 failed; 63 ignored'));
const regressionLogs = fs.readdirSync(__dirname).filter(n=>
  /^(real_|resident_|actual_product_).*\.log$/.test(n));
assert.equal(regressionLogs.length,9);
const regressions = regressionLogs.map(n=>{
  const log = fs.readFileSync(path.join(__dirname,n),'utf8');
  assert(log.includes('test result: ok. 1 passed; 0 failed;'),n);
  return {name:n,sha256:hash(path.join(__dirname,n)),result:log.split(/\r?\n/).find(l=>l.startsWith('test result:'))};
});
const validation = {
  schema:'resident-cpu-queue-20261014-v1', source:'7d8ff5a7c6e684b211b9838e87755b0c0dfcc5fa',
  summary:'summary.json', hashes:summary.hashes,
  sdk_sha256:contract.sdk_sha256, sdk_changed:false, extra_native_context:false,
  cpu_default_unchanged:true, cpu_algorithm_unchanged:true, cpu_queue_wrapper_changed:true,
  gpu_device_residency_changed:false, stable_tps_improvement_accepted:false,
  production_accepted:false, independent_bigint_oracle_rerun:false,
  source_files:sourceFiles.map(p=>({path:p,sha256:hash(p)})),
  preserved_drafts:summary.preserved_drafts,
  explicit_regressions:regressions,
  pipeline:pipeline.split(/\r?\n/).find(l=>l.startsWith('test result:')),
  clippy:{command:'cargo clippy --release --locked --target-dir target -p novovm-node --lib -- -D warnings',passed:true,log_sha256:hash(path.join(__dirname,'clippy.log'))},
  fmt:{command:'cargo fmt --all -- --check',passed:true},
  prior_delivery_ci:{sha:'da93a4dcdbcf4140243b70636921f14723b7b206',run:38563008376,conclusion:'cancelled'},
  delivery_ci:'pending push; not verified',
  limitations:['Local Windows only; four processes are not four physical devices','Removal of redundant CPU queue permit operations does not establish cause of earlier 3.11 percent regression','No GPU algorithm or proof capability improvement claimed']
};
fs.writeFileSync(path.join(__dirname,'validation.json'), JSON.stringify(validation,null,2)+'\n');
console.log(JSON.stringify({combined,controlComparison,samples,preserved_drafts:summary.preserved_drafts},null,2));
