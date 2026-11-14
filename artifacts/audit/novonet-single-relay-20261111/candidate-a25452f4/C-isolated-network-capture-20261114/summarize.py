"""Summarize the completed private capture without exporting payloads or addresses."""
from pathlib import Path
import collections, datetime, hashlib, json, re, struct

ROOT = Path('target/c-joint-network-capture-20261114').resolve()
OUT = Path(__file__).resolve().parent / 'result-public.json'

def sha(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()

def trace(p):
    counts = collections.Counter()
    threads = set()
    live = {}
    issues = collections.Counter()
    sent, received = set(), set()
    queries = {}
    phase = 'before_first_query'
    phase_counts = collections.defaultdict(collections.Counter)
    seq = last_ns = 0
    with p.open('rb') as f:
        assert f.read(8) == b'NCAP0001'
        while header := f.read(64):
            if len(header) != 64:
                issues['partial_header'] += 1
                break
            n, kind, tid, ns, address, generation, count, size = struct.unpack('<8Q', header)
            if size > 128 * 1024**2:
                issues['invalid_payload_length'] += 1
                break
            payload = f.read(size)
            if len(payload) != size:
                issues['partial_payload'] += 1
                break
            if n != seq + 1:
                issues['sequence_gap'] += 1
            if ns < last_ns:
                issues['clock_regressed'] += 1
            seq, last_ns = n, ns
            counts[str(kind)] += 1
            phase_counts[phase][str(kind)] += 1
            if kind == 5:
                phase = 'during_query'
            elif kind == 6:
                phase = 'after_query'
            threads.add(tid)
            if kind == 2:
                if address in live:
                    issues['duplicate_observed_live_address'] += 1
                live[address] = (generation, payload)
                sent.add(payload)
            elif kind == 3:
                old = live.pop(address, None)
                if generation == 0:
                    issues['drop_without_observed_construction'] += 1
                elif old != (generation, payload):
                    issues['observed_generation_or_id_mismatch'] += 1
            elif kind == 4:
                received.add(payload)
            elif kind in (5, 6):
                queries.setdefault(str(count), {})['begin_ns' if kind == 5 else 'end_ns'] = ns
    return ({'name': p.name, 'bytes': p.stat().st_size, 'sha256': sha(p),
             'records': seq, 'event_counts': dict(counts), 'logical_thread_count': len(threads),
             'last_elapsed_ns': last_ns, 'observed_live_at_end': len(live),
             'observations': dict(issues), 'queries': queries,
             'phase_event_counts': {k: dict(v) for k, v in phase_counts.items()},
             'unique_sent_ids': len(sent), 'unique_received_ids': len(received)}, sent, received)

def boundaries(p):
    counts = collections.Counter()
    groups = {}
    query_events = []
    stops = 0
    target_exit = []
    error_lines = []
    rows = 0
    with p.open(errors='replace') as f:
        for line in f:
            if line.strip() == 'FIRST_CAPTURE_STOP':
                stops += 1
            if line.startswith(('QUERY_BEGIN ', 'QUERY_END ')):
                # Only identifiers/counters, not object addresses.
                query_events.append(dict(re.findall(r'(tid|query|calls)=([0-9a-f]+)', line)))
                query_events[-1]['event'] = line.split()[0]
            if line.startswith('Last event:') and 'Exit process' in line:
                target_exit.append(line.strip())
            if line.startswith(('Syntax error', "Couldn't resolve", 'Unable to resolve')):
                error_lines.append(line.strip())
            if not line.startswith('BND '):
                continue
            fields = dict(re.findall(r'(\w+)=([\w`]+)', line))
            stage = fields.pop('stage')
            key = tuple(fields.get(k) for k in ('tid', 'query', 'call'))
            groups.setdefault(key, []).append((stage, fields))
            rows += 1
            counts[stage] += 1
    complete = complete_call = partial = receiver_mismatch = text_mismatch = 0
    expected_stages = ['FMT_BEGIN', 'FMT_CALL', 'PAD_ENTRY', 'TAIL_JUMP', 'WRITER_ENTRY', 'COPY_CALL']
    for records in groups.values():
        stages = [s for s, _ in records]
        call_stages = expected_stages.copy()
        call_stages[3] = 'PAD_CALL'
        if stages not in (expected_stages, call_stages):
            partial += 1
            continue
        if stages == expected_stages:
            complete += 1
        else:
            complete_call += 1
        if len({f['receiver'] for _, f in records}) != 1:
            receiver_mismatch += 1
        if len({(f['text'], f['len']) for _, f in records[2:]}) != 1:
            text_mismatch += 1
    return {'log_bytes': p.stat().st_size, 'log_sha256': sha(p), 'rows': rows,
            'stage_counts': dict(counts), 'complete_tail_groups': complete,
            'complete_call_groups': complete_call,
            'other_or_partial_groups': partial, 'receiver_mismatch_complete_groups': receiver_mismatch,
            'text_or_length_mismatch_complete_groups': text_mismatch,
            'guard_or_exception_stops': stops, 'query_events': query_events,
            'target_exit_events': target_exit, 'debugger_error_lines': error_lines}

def main():
    assert (ROOT / 'run-exit.json').is_file(), 'do not finalize a live capture'
    assert not OUT.exists(), 'preserve previous report'
    traces = [trace(p) for p in sorted((ROOT / 'run-state-private').glob('trace-*-private.bin'))]
    all_sent = set().union(*(s for _, s, _ in traces))
    all_received = set().union(*(r for _, _, r in traces))
    report = {'generated_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'start': json.loads((ROOT / 'capture-start.json').read_text()),
              'gate': json.loads((ROOT / 'capture-gate-public.json').read_text()),
              'exit': json.loads((ROOT / 'run-exit.json').read_text()),
              'boundaries': boundaries(ROOT / 'boundary-private.log'),
              'traces': [r for r, _, _ in traces],
              'cross_process_observed_sent_and_received_ids': len(all_sent & all_received),
              'failure_dump_exists': (ROOT / 'failure-private.dmp').exists(),
              'original_fault_reproduced': False, 'first_corrupting_instruction_found': False,
              'production_code_fixed': False, 'double_relay_resumed': False,
              'limitations': ['fresh loopback test identities and databases; current user, not LocalService',
                              'diagnostic hooks and debugger stops perturb scheduling',
                              'logical trace thread labels are not Windows debugger thread ids',
                              'observed Arc creation/drop is not complete allocation/reference history',
                              'no original packet sequence, NOVONET discovery or real relay workload',
                              'normal records cannot prove original safety or reconstruct past failure']}
    OUT.write_bytes((json.dumps(report, indent=2) + '\n').encode())
    print(json.dumps({'exit': report['exit'], 'boundaries': report['boundaries'],
                      'matching_object_ids': report['cross_process_observed_sent_and_received_ids']}))

if __name__ == '__main__':
    main()
