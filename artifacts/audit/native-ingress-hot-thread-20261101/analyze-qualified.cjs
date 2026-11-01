// Offline reduction of this fixed capture's xperf exports. Never starts a node.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const base = path.resolve(process.argv[2]);
const out = path.join(__dirname, 'QUALIFIED-TRACE-RESULTS.json');
const readJson = name => JSON.parse(fs.readFileSync(path.join(base, name), 'utf8'));
const sha = name => crypto.createHash('sha256').update(fs.readFileSync(path.join(base, name))).digest('hex');
const manifest = readJson('capture.json');
const report = JSON.parse(fs.readFileSync(path.resolve(base, '../../native-product-load/paired-20140-1793491596690.json'), 'utf8'));
if (manifest.driver_pid !== 20140 || manifest.failure || manifest.load_exit !== 0) throw Error('Unexpected capture');
const decode = s => s.replace(/<[^>]*>/g, '').replace(/&nbsp;/g, ' ').replace(/&gt;/g, '>').replace(/&lt;/g, '<').replace(/&amp;/g, '&').trim();
const cells = s => [...s.matchAll(/<td[^>]*>(.*?)<\/td>/gs)].map(m => decode(m[1]));
function parseStack(html) {
  const total = Number(cells(html.match(/<tr class='pp'>(.*?)<\/tr>/s)[1])[2]);
  function table(id) {
    const body = html.match(new RegExp(`<a id='${id}'>.*?<tbody>(.*?)</tbody>`, 's'))[1];
    return [...body.matchAll(/<tr[^>]*>(.*?)<\/tr>/gs)].map(m => cells(m[1])).filter(c => c.length)
      .map(c => ({symbol:c[0], hits:Number(c[1])}));
  }
  const section = html.slice(html.indexOf("id='TblSN'"), html.indexOf("id='TblSE'"));
  const functions = [...section.matchAll(/<tbody>(.*?)<\/tbody>/gs)].map(m => {
    const rows = [...m[1].matchAll(/<tr[^>]*>(.*?)<\/tr>/gs)].map(r => cells(r[1])).filter(c => c.length);
    return {symbol:rows[0][0], callers:rows.slice(1).filter(c => c[0].startsWith('<--')).map(c => ({symbol:c[0].slice(3).trim(),hits:Number(c[1])}))};
  });
  return {total, inclusive:table('TblSI'), exclusive:table('TblSE'), functions};
}
const patterns = {
  native_network:'NativeLifecycle7poll_io',
  object_send:'NativeObjectSender17poll_send_limited',
  system_send:'UdpSocket::send_to', system_receive:'UdpSocket::recv_from',
  loopback_enqueue:'tcpip.sys!IppLoopbackEnqueue',
  secure_seal:'E2eSecureChannelV119seal_novorudp_frame',
  secure_open:'E2eSecureChannelV125open_novorudp_frame_owned',
  aead_encrypt:'AeadInPlace16encrypt_in_place', aead_decrypt:'AeadInPlace16decrypt_in_place',
  frame_checksum:'novorudp_transport_frame_checksum',
  admission_state_poll:'UcaBatchStateRead4poll', admission_state_prepare:'UcaBatchStateRead3new',
  bulk_capture:'BulkCapture17advance_with_seed', tree_node_decode:'tree4Node6decode',
  sha256_compress:'x86_sha8compress', consensus_poll:'NativeLifecycle4poll',
  mldsa_bridge_on_this_thread:'aoem_mldsa_bridge_verify'
};
const cpu = [];
for (const [round,pid,tid] of [[0,9360,33260],[1,17884,13856],[4,24200,26324]]) {
  const s = parseStack(fs.readFileSync(path.join(base, `round-${round}-tick-fullsymbols.html`), 'utf8'));
  const samples = fs.readFileSync(path.join(base, `round-${round}-profile-events.csv`), 'utf8').split(/\r?\n/)
    .filter(l => /^\s*SampledProfile,/.test(l)).map(l => l.split(',').map(c => c.trim()))
    .filter(c => c[2] === `novovm-node.exe (${pid})` && Number(c[3]) === tid);
  if (!samples.length || samples.some(c => Number(c[8]) !== 1)) throw Error('Unexpected sampling weights');
  const raw = samples.length;
  if (s.exclusive.reduce((sum,x) => sum+x.hits,0) !== s.total || s.total > raw) throw Error('Stack denominator mismatch');
  const hit = row => ({...row, percent_of_all_target_samples:row.hits/raw*100});
  const metrics = Object.fromEntries(Object.entries(patterns).map(([name,p]) => {
    const found = s.inclusive.filter(x => x.symbol.includes(p));
    if (found.length > 1) throw Error(`Ambiguous metric ${name}`);
    return [name, found.length ? hit(found[0]) : null];
  }));
  const frequency = fs.readFileSync(path.join(base, `round-${round}-frequency.txt`), 'utf8');
  if (!/FreqZone,\s+0,\s+\d+,\s+1000\.0/.test(frequency)) throw Error('Unexpected frequency');
  cpu.push({round,backend:report.rounds[round].signature_backend,pid,tid,thread_name:'novovm-native-tick',
    sample_period_us:1000,raw_sample_count:raw,xperf_stack_hits:s.total,
    samples_not_represented_in_stack_report:raw-s.total,stack_report_coverage_percent:s.total/raw*100,
    unresolved_leaf_hits:s.exclusive.filter(x => x.symbol.includes('***unknown***')).reduce((sum,x) => sum+x.hits,0),
    metrics,exclusive_top:s.exclusive.slice(0,20).map(hit),
    sha256_immediate_callers:s.functions.find(f => f.symbol.includes('x86_sha8compress')).callers.map(hit),
    system_send_immediate_callers:s.functions.find(f => f.symbol.includes('UdpSocket::send_to')).callers.map(hit),
    private_export_hashes:{stack_html:sha(`round-${round}-tick-fullsymbols.html`),sample_events:sha(`round-${round}-profile-events.csv`)}
  });
}
const windows = manifest.windows.filter(w => Number.isInteger(w.round)).map(w => {
  const r = report.rounds[w.round];
  const a = r.process_cpu_accounting.external_sample_alignment;
  const overlap = Math.max(0, Math.min(w.trace_end_unix_ms,a.after_read_started_unix_ms)-Math.max(w.trace_start_unix_ms,a.before_read_completed_unix_ms))/1000;
  if (w.lost_events !== 0 || w.lost_buffers !== 0 || overlap < (w.profile === 'NativeSchedule' ? 3 : 4)) throw Error('Trace not qualified');
  if (!r.passed || !r.cold_recovery_pass || !r.full_fee_state_audited || r.single_submission_node !== 0) throw Error('Workload failed');
  return {round:w.round,profile:w.profile,backend:w.backend,lost_events:w.lost_events,lost_buffers:w.lost_buffers,
    trace_start_unix_ms:w.trace_start_unix_ms,trace_end_unix_ms:w.trace_end_unix_ms,drive_start_unix_ms:a.before_read_completed_unix_ms,
    drive_end_unix_ms:a.after_read_started_unix_ms,overlap_seconds:overlap,etl_sha256:sha(`round-${w.round}.etl`),
    node_pids:r.process_cpu_accounting.nodes.map(n => n.pid),diagnostic_tps:r.successful_tps,p95_us:r.latency_us.p95,p99_us:r.latency_us.p99,
    successful_transactions:r.transactions,workload_passed:r.passed,cold_recovery_passed:r.cold_recovery_pass,fee_audit_passed:r.full_fee_state_audited};
});
const scheduling = readJson('schedule-attribution.json').map(s => {
  if (Object.values(s.state_us).reduce((a,b) => a+b,0) !== s.duration_us) throw Error('Schedule denominator');
  return {round:s.round,backend:s.backend,pid:s.pid,tid:s.tid,thread_name:s.thread_name,range_us:s.range_us,
    duration_us:s.duration_us,first_reliable_cswitch_us:s.first_reliable_cswitch_us,state_us:s.state_us,state_percent:s.state_percent,
    waiting_reason_us:s.waiting_reason_us,intervals:s.intervals,scope:s.scope,
    independent_xperf_running_us:s.round===2?4814785:4805213};
});
const result = {
  status:'QUALIFIED_BOUNDED_LOAD_WINDOWS_WITH_STATED_SYMBOL_AND_STACK_LIMITS',
  source_capture:path.relative(path.resolve(__dirname,'../../..'),base).replaceAll('\\','/'),
  scope:'Diagnostic only; frozen production, AOEM, tester and policies unchanged. No new TPS acceptance. No raw system ETL/PDB/all-system CSV included.',
  sample_denominator:'Every percentage uses all raw SampledProfile events for the selected original PID/TID, including samples absent from the xperf stack report. Inclusive scopes overlap: do not add them or add exclusive leaves to their parent. 1ms CPU samples are not end-to-end latency or elapsed wall stages.',
  symbol_limits:'Microsoft OS PDBs and the matching frozen native/AOEM PDBs resolve most leaves. Public-only optimized Rust PDB names can label unnamed/inline functions incorrectly; rpc::error is not evidence HTTP/JSON processing. Exact RVAs, unwind boundaries, direct calls and source callsites are used for the native admission interpretation. Raw-sample versus stack-hit difference is retained unassigned, not silently renormalized.',
  files:manifest.files, input_identity:report.rounds[0].input_identity,
  exact_address_symbol_review:readJson('symbol-attribution-caveats.json'),
  same_signed_corpus_and_initial_state:report.same_signed_corpus_and_initial_state,
  windows,cpu,scheduling,
  limits:['Single Windows machine with four native processes and loopback; kernel send cost includes local receive delivery, not wire latency or NIC bandwidth.',
    'CPU stack and scheduling profiles cover different rounds and short windows, not simultaneous full-round measurements.',
    'Running context-switch residency includes possible spin/poll and DPC/ISR time; it is not useful work. No waiting stacks or allocator event tracing.',
    'Low ML-DSA presence on the ingress thread does not measure worker/device cryptography or prove/disprove asset GPU execution.',
    'No full historical 12K-versus-current causal decomposition or performance improvement is established by this diagnosis.']
};
fs.writeFileSync(out, JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify({output:out,windows:windows.length,raw_samples:cpu.map(c => c.raw_sample_count),stack_coverage:cpu.map(c => c.stack_report_coverage_percent)}));
