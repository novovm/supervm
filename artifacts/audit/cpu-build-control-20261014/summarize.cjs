// Read-only analysis of a predeclared negative control. No benchmark selection.
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const assert = require('assert/strict');
const hash = b => crypto.createHash('sha256').update(b).digest('hex');
function pe(p) {
  const raw = fs.readFileSync(p), normalized = Buffer.from(raw);
  const header = raw.readUInt32LE(60), optional = header + 24;
  assert.equal(raw.readUInt32LE(header), 0x4550);
  assert.equal(raw.readUInt16LE(optional), 0x20b);
  const sections = [];
  for (let i = 0; i < raw.readUInt16LE(header + 6); i++) {
    const at = optional + raw.readUInt16LE(header + 20) + 40 * i;
    const size = raw.readUInt32LE(at + 16), offset = raw.readUInt32LE(at + 20);
    sections.push({name: raw.subarray(at, at + 8).toString().replace(/\0/g, ''),
      rva: raw.readUInt32LE(at + 12), offset, size,
      sha256: hash(raw.subarray(offset, offset + size))});
  }
  const ranges = [{offset: header + 8, bytes: 4, kind: 'COFF timestamp'}];
  const directory = optional + 112 + 6 * 8;
  const rva = raw.readUInt32LE(directory), size = raw.readUInt32LE(directory + 4);
  const section = sections.find(s => rva >= s.rva && rva + size <= s.rva + s.size);
  assert(section); assert.equal(size % 28, 0);
  const debug = section.offset + rva - section.rva;
  for (let at = debug; at < debug + size; at += 28) {
    ranges.push({offset: at + 4, bytes: 4, kind: 'debug timestamp'});
    if (raw.readUInt32LE(at + 12) === 2) {
      const cv = raw.readUInt32LE(at + 24);
      assert.equal(raw.subarray(cv, cv + 4).toString(), 'RSDS');
      ranges.push({offset: cv + 4, bytes: 16, kind: 'CodeView PDB GUID'});
    }
  }
  for (const r of ranges) normalized.fill(0, r.offset, r.offset + r.bytes);
  return {path: p, sha256: hash(raw), bytes: raw.length, sections,
    ignored_metadata: ranges, normalized_sha256: hash(normalized)};
}
const binaries = [pe('aoem/windows/core/bin/aoem_ffi.dll'),
  pe('target/validation/cpu-build-control-20261014/rebuilt.dll')];
assert.equal(binaries[0].normalized_sha256, binaries[1].normalized_sha256);
const order = ['old-1','rebuilt-1','rebuilt-2','old-2','old-diagnostic','rebuilt-diagnostic'];
const samples = order.map(name => {
  const r = JSON.parse(fs.readFileSync(path.join(__dirname, name + '.json')));
  assert(r.passed); assert.equal(r.execution_backend, 'cpu_callbacks');
  assert.equal(r.counts.finalized_successful, 65536);
  assert.equal(r.counts.finalized_business_failed, 0);
  assert.equal(r.unresolved_submitted, 0); assert.equal(r.unique_permanently_rejected, 0);
  assert.equal(r.evidence.binary_sha256, '0x4d6f4bbc8c85174d5d2b809781380335be74e8490845aff52510239bb77fd8f8');
  const variant = name.startsWith('old-') ? 0 : 1;
  assert.equal(r.evidence.aoem_sha256, '0x' + binaries[variant].sha256);
  const diagnostic = name.endsWith('diagnostic');
  const phases = diagnostic ? r.evidence.profile_end.map((end, i) => {
    const start = r.evidence.profile_start[i];
    assert(start.enabled && end.enabled);
    return Object.fromEntries(Object.keys(end.phases).map(k => [k, {
      calls: end.phases[k].calls - start.phases[k].calls,
      wall_ms: (end.phases[k].wall_ns - start.phases[k].wall_ns) / 1e6,
      // Max is process-lifetime; subtracting maxima would be invalid.
      lifetime_max_ms: end.phases[k].max_wall_ns / 1e6
    }]));
  }) : null;
  for (const p of phases || []) {
    assert.equal(p.execute_integer.calls, 0); assert.equal(p.integer_wire.calls, 0);
  }
  return {name, diagnostic, seconds: r.elapsed_seconds, tps: r.successful_tps,
    latency_us: r.latency_us, counts: r.counts, phases};
});
const formal = samples.filter(s => !s.diagnostic);
const combined = Object.fromEntries(['old','rebuilt'].map(v => {
  const group = formal.filter(s => s.name.startsWith(v + '-'));
  return [v, 131072 / group.reduce((n, s) => n + s.seconds, 0)];
}));
const drafts = JSON.parse(fs.readFileSync('artifacts/audit/resident-integer-device-20261013/validation.json')).preserved_drafts.files;
assert.equal(drafts.length, 49);
for (const f of drafts) assert.equal(hash(fs.readFileSync(f.path)), f.sha256.toLowerCase(), f.path);
console.log(JSON.stringify({schema:'cpu-build-negative-control/v1', binaries, samples,
  combined_tps: combined, combined_change_percent: 100 * (combined.rebuilt / combined.old - 1),
  formal_min_tps: Math.min(...formal.map(s => s.tps)),
  formal_max_tps: Math.max(...formal.map(s => s.tps)),
  preserved_drafts: 49,
  scope: 'Frozen original node and driver; same runtime source; only PE timestamps/PDB GUID differ. Inclusive nested wall phases are not additive CPU time. Negative control does not exonerate rejected GPU patches.'}, null, 2));
