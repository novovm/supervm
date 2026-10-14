// Read-only evidence aggregation. Run from the repository root.
const fs = require('node:fs');
const taskCrypto = require('node:crypto');
const dir = 'artifacts/audit/resident-effect-projection-20261014/';
const hash = path => taskCrypto.createHash('sha256').update(fs.readFileSync(path)).digest('hex');
const samples = fs.readdirSync(dir).filter(n => /^(baseline|final|repeat-old)-(cpu|integer)-[0-9]+\.json$/.test(n)).map(n => {
    const r = JSON.parse(fs.readFileSync(dir + n));
    if (!r.passed || r.counts.finalized_successful !== 65536 || r.counts.finalized_business_failed !== 0 || r.unresolved_submitted !== 0 || r.unique_permanently_rejected !== 0) throw Error(n + ' failed');
    return { sample: n.slice(0, -5), tps: r.successful_tps, seconds: r.elapsed_seconds,
        p95_us: r.latency_us.p95, p99_us: r.latency_us.p99, node_sha256: r.evidence.binary_sha256,
        aoem_sha256: r.evidence.aoem_sha256, observation_requests: r.counts.observation_requests,
        successful: r.counts.finalized_successful, business_failed: r.counts.finalized_business_failed,
        transport_errors: r.counts.transport_errors };
});
const combined = {};
for (const prefix of ['repeat-old-cpu', 'baseline-cpu', 'final-cpu', 'baseline-integer', 'final-integer']) {
    const s = samples.filter(x => x.sample.startsWith(prefix));
    combined[prefix] = { samples: s.length, tps: s.reduce((a, x) => a + x.successful, 0) / s.reduce((a, x) => a + x.seconds, 0) };
}
const drafts = JSON.parse(fs.readFileSync('artifacts/audit/resident-integer-device-20261013/validation.json')).preserved_drafts.files;
const drift = drafts.filter(x => hash(x.path).toLowerCase() !== x.sha256.toLowerCase().replace(/^0x/, ''));
const sources = ['crates/novovm-node/src/native_pipeline/business/nov_transfer_batch/full_effects.rs', 'crates/novovm-node/src/native_pipeline/business/nov_transfer_batch/full_effects_tests.rs'];
console.log(JSON.stringify({ samples, combined,
    integer_change_pct: (combined['final-integer'].tps / combined['baseline-integer'].tps - 1) * 100,
    cpu_change_pct: (combined['final-cpu'].tps / combined['baseline-cpu'].tps - 1) * 100,
    protected_drafts: { checked: drafts.length, drift },
    hashes: { node: hash('target/release/novovm-node.exe'), unit_driver: hash('target/release/deps/novovm_node-3fdab5433ef50d79.exe'),
        load_driver: hash('target/validation/dataflow-c48dd7bd/novovm_node-54e8d64d0bc52c1f.exe'), aoem: hash('aoem/windows/core/bin/aoem_ffi.dll') },
    sources: sources.map(path => ({ path, sha256: hash(path) })) }));
