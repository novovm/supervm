// Offline xperf reduction only. Raw traces and system-wide exports stay private.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { parseStack } = require('../native-ingress-hot-thread-20261101/analyze-qualified.cjs');
const base = path.resolve(process.argv[2]);
const output = path.resolve(process.argv[3] || path.join(base, 'sync-send-attribution.json'));
const read = name => fs.readFileSync(path.join(base, name), 'utf8');
const json = name => JSON.parse(read(name));
const shaFile = name => crypto.createHash('sha256').update(fs.readFileSync(name)).digest('hex');
const sha = name => shaFile(path.join(base, name));
const manifest = json('capture.json');
const candidate = JSON.parse(fs.readFileSync(path.join(__dirname, 'candidate.json'), 'utf8'));
const reportPath = path.resolve(read('load.stderr.log').match(/NUAS paired report=(.+)/)[1].trim());
const report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
if (manifest.failure || manifest.load_exit !== 0 || report.rounds.length !== 5 ||
    !report.same_signed_corpus_and_initial_state || !report.business_backend_unchanged) throw Error('Failed/mixed workload');
for (const expected of candidate.files) {
  const actual = manifest.files.find(f => f.role === expected.role);
  if (!actual || actual.sha !== expected.sha256 || shaFile(actual.path) !== expected.sha256) throw Error(`Image mismatch: ${expected.role}`);
}
const headers = manifest.windows.map(w => {
  const text = read(`${w.tag}-header.txt`);
  if (w.failure || w.start_exit !== 0 || w.stop_exit !== 0 || w.header_exit !== 0 ||
      w.lost_events !== 0 || w.lost_buffers !== 0 ||
      !/Total # Lost Buffers\s*:\s*0\b/.test(text) || !/Total # Lost Events\s*:\s*0\b/.test(text)) throw Error(`Lost/failed trace: ${w.tag}`);
  return {tag:w.tag, lost_events:0, lost_buffers:0, header_sha256:sha(`${w.tag}-header.txt`)};
});
const windows = manifest.windows.filter(w => Number.isInteger(w.round)).map(w => {
  const r = report.rounds[w.round], a = r.process_cpu_accounting.external_sample_alignment;
  if (!r.passed || !r.cold_recovery_pass || !r.full_fee_state_audited || r.single_submission_node !== 0 ||
      r.transactions !== 65536 || r.counts.successful_transactions !== 65536 || r.real_machines !== 1 ||
      r.transport !== 'native_novorudp_apfl' || r.signed_workload_sha256 !== report.rounds[0].signed_workload_sha256 ||
      r.genesis_commitment !== report.rounds[0].genesis_commitment || r.initial_root !== report.rounds[0].initial_root ||
      r.binary_sha256 !== `0x${candidate.files.find(f => f.role === 'node').sha256}` ||
      r.aoem_sha256 !== `0x${candidate.files.find(f => f.role === 'aoem').sha256}`) throw Error(`Workload mismatch: ${w.round}`);
  if (w.trace_start_unix_ms < a.before_read_completed_unix_ms || w.trace_end_unix_ms > a.after_read_started_unix_ms) throw Error('Trace outside drive bracket');
  return {round:w.round, backend:r.signature_backend, profile:w.profile,
    trace_start_unix_ms:w.trace_start_unix_ms, trace_end_unix_ms:w.trace_end_unix_ms,
    drive_start_unix_ms:a.before_read_completed_unix_ms, drive_end_unix_ms:a.after_read_started_unix_ms,
    trace_seconds:(w.trace_end_unix_ms-w.trace_start_unix_ms)/1000, node_pids:r.process_cpu_accounting.nodes.map(n => n.pid),
    etl_sha256:sha(`round-${w.round}.etl`), successful_transactions:r.transactions,
    diagnostic_tps:r.successful_tps, p95_us:r.latency_us.p95, p99_us:r.latency_us.p99,
    four_node_cpu_us_per_transaction:r.process_cpu_accounting.four_node_processes.cpu_us_per_successful_transaction,
    entry_process_cpu_us_per_transaction:r.process_cpu_accounting.nodes[0].cpu.cpu_us_per_successful_transaction,
    workload_passed:r.passed, cold_recovery_passed:r.cold_recovery_pass, fee_audit_passed:r.full_fee_state_audited};
});
const patterns = {
  native_io:'NativeLifecycle7poll_io', system_send:'UdpSocket::send_to', winsock_send:'ws2_32.dll!sendto',
  winsock_provider:'mswsock.dll!WSPSendTo', afd_datagram:'afd.sys!AfdFastDatagramSend',
  udp_send:'tcpip.sys!UdpSendMessages$', udp_send_path:'tcpip.sys!UdpSendMessagesOnPath',
  loopback:'tcpip.sys!IppLoopbackEnqueue', receive_indication:'tcpip.sys!WfpProcessInTransportStackIndication$'
};
function metric(stack, pattern) {
  const found = stack.inclusive.filter(x => pattern.endsWith('$') ? x.symbol.endsWith(pattern.slice(0,-1)) : x.symbol.includes(pattern));
  if (found.length > 1) throw Error(`Ambiguous symbol: ${pattern}`);
  return found[0] || {symbol:pattern, hits:0};
}
const cpu = windows.filter(w => w.profile === 'NativeCpuStacks').map(w => {
  const round = w.round, pid = w.node_pids[0];
  const names = read(`round-${round}-thread-events.csv`).split(/\r?\n/).map(l => l.split(',').map(c => c.trim()))
    .filter(c => /^T-DC(Start|End)$/.test(c[0]) && c[2] === `novovm-node.exe (${pid})` && c.at(-1) === 'novovm-native-tick');
  const tids = [...new Set(names.map(c => Number(c[3])))];
  if (tids.length !== 1) throw Error('Missing/ambiguous original tick thread');
  const tid = tids[0];
  const samples = read(`round-${round}-profile-events.csv`).split(/\r?\n/)
    .filter(l => /^\s*SampledProfile,/.test(l)).map(l => l.split(',').map(c => c.trim()))
    .filter(c => c[2] === `novovm-node.exe (${pid})` && Number(c[3]) === tid);
  if (!samples.length || samples.some(c => Number(c[8]) !== 1) ||
      !/FreqZone,\s+0,\s+\d+,\s+1000\.0/.test(read(`round-${round}-frequency.txt`))) throw Error('Invalid sample weights/frequency');
  const raw = samples.length;
  const stack = parseStack(read(`round-${round}-tick-fullsymbols.html`));
  const send = parseStack(read(`round-${round}-send-only.html`));
  const loop = parseStack(read(`round-${round}-loopback-only.html`));
  for (const s of [stack, send, loop]) if (s.exclusive.reduce((sum,x) => sum+x.hits,0) !== s.total || s.total > raw) throw Error('Stack denominator mismatch');
  if (send.total !== metric(stack,patterns.system_send).hits || loop.total !== metric(stack,patterns.loopback).hits) throw Error('Filtered stack mismatch');
  const pct = x => ({...x, percent_of_all_target_samples:x.hits/raw*100});
  const modules = {};
  for (const row of send.exclusive) {
    const module = row.symbol.split('!')[0];
    modules[module] = (modules[module] || 0) + row.hits;
  }
  const kernel = Object.entries(modules).filter(([m]) => /\.sys$|^ntkrnlmp\.exe$/i.test(m)).reduce((sum,[,n]) => sum+n,0);
  const intersection = metric(send,patterns.loopback).hits;
  return {round, backend:w.backend, pid, tid, thread_name:'novovm-native-tick', sample_period_us:1000,
    raw_sample_count:raw, xperf_stack_hits:stack.total, samples_not_represented_in_stack_report:raw-stack.total,
    stack_report_coverage_percent:stack.total/raw*100,
    unresolved_leaf_hits:stack.exclusive.filter(x => x.symbol.includes('***unknown***')).reduce((sum,x) => sum+x.hits,0),
    metrics:Object.fromEntries(Object.entries(patterns).map(([key,p]) => [key,pct(metric(stack,p))])),
    send_filtered:{total:send.total, loopback_intersection:intersection, loopback_percent_of_send_samples:intersection/send.total*100,
      loopback_hits_without_send_frame:loop.total-intersection, exclusive_leaf_modules:modules,
      known_kernel_leaf_hits:kernel, known_kernel_leaf_percent_of_send_samples:kernel/send.total*100,
      immediate_callers:send.functions.find(f => f.symbol.includes('UdpSocket::send_to')).callers},
    loopback_filtered:{total:loop.total, receive_chain:Object.fromEntries(['UdpDeliverDatagrams','InetInspectReceiveDatagram',
      'WfpProcessInTransportStackIndication','AfdTLDgramReceiveEventHandler','AfdReceiveDatagramEventCommonHandler'].map(p => [p,pct(metric(loop,p+'$'))]))},
    private_export_sha256:Object.fromEntries(['tick-fullsymbols.html','profile-events.csv','thread-events.csv','send-only.html','loopback-only.html','frequency.txt'].map(name => [name,sha(`round-${round}-${name}`)]))};
});
const scheduling = json('schedule-attribution.json');
for (const s of scheduling) {
  if (s.pid !== windows.find(w => w.round === s.round).node_pids[0] || s.thread_name !== 'novovm-native-tick' ||
      Object.values(s.state_us).reduce((a,b) => a+b,0) !== s.duration_us) throw Error('Schedule identity/denominator mismatch');
}
const result = {
  status:'QUALIFIED_CURRENT_IMAGE_BOUNDED_CPU_AND_SCHEDULING_WINDOWS',
  source_capture:path.relative(path.resolve(__dirname,'../../..'),base).replaceAll('\\','/'),
  scope:'Offline diagnosis only; no production changes, new captures, remote operations or performance improvement acceptance.',
  source_report_sha256:shaFile(reportPath), files:manifest.files, headers,
  input_identity:{signed_workload_sha256:report.rounds[0].signed_workload_sha256,
    genesis_commitment:report.rounds[0].genesis_commitment, initial_root:report.rounds[0].initial_root},
  same_signed_corpus_and_initial_state:report.same_signed_corpus_and_initial_state, windows, cpu, scheduling,
  exact_address_symbol_review:json('send-symbol-caveat.json'), raw_address_closure:json('send-symbol-raw-closure.json'),
  private_evidence_sha256:Object.fromEntries(['capture.json','schedule-attribution.json','analyze-schedule.cjs','send-symbol-caveat.json',
    'send-symbol-raw-closure.json','round-0-raw-stacks.csv','round-0-raw-stacks.log'].map(name => [name,sha(name)])),
  limits:[
    'CPU percentages use ALL raw PID/TID samples, including missing stack reports; inclusive parents/children overlap and must not be added.',
    'Send-selected exclusive module leaves partition sampled on-CPU work, not API wall latency or a complete cycles breakdown. Interrupt/DPC work is not separately subtracted.',
    'Loopback/send intersection is counted from send-filtered stacks; 1/2/1 loopback hits lack a send frame and remain separate.',
    'CPU and schedule windows are different rounds. Running residency can include polling/spin and DPC/ISR; no wait callsite stacks were captured.',
    'Optimized public Rust names can be misleading. Round0 exact raw-PC closure verifies the private batch-send return, not NativePollTimer cost. Other rounds were not independently raw-PC closed.',
    'The separate raw dumper has 24 retained InvalidEvent warnings and 8345 target SampledProfile rows versus 8348 in the profile-specific export; only the exact 1001-row address intersection is closed by it.',
    'No controlled segmented-vs-single batch experiment or physical-NIC comparison; a sparse UdpSegmentMessage symbol is not total segmentation cost.',
    'WFP/WinNAT/NDIS presence does not establish faulty or removable security work. No firewall, driver or network configuration was changed.',
    'Single Windows host, four processes, pre-generated same-byte corpus. No continuous-supply performance acceptance or full historical 12K loss attribution.'
  ]
};
fs.writeFileSync(output, JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify({output,windows:windows.length,raw_samples:cpu.map(c => c.raw_sample_count),send_hits:cpu.map(c => c.send_filtered.total)}));
