"""Audit only existing same-run decoder outputs; no capture or target execution."""
from pathlib import Path
import json,re,struct,hashlib

root=Path(__file__).resolve().parents[1]
run=root/'run-filecs01'
selected=run/'decode/selected-private'
follow=run/'decode/fault-adjacent-private'
r=json.loads((run/'traced-target/result.json').read_text(encoding='utf-8-sig'))
d=json.loads((selected/'decode-public.json').read_text())
extra=json.loads((follow/'decode-public.json').read_text())
base=int(d['base'],16)

# The exact EXE/PDB mapping already established on file01 is reused only after
# same-build SHA and same captured code/base checks, not from nearest symbols.
prior=json.loads((root/'run-file01/decode/selected-private/decode-public.json').read_text())
for name in ['PtQualification.exe','KERNELBASE.dll','KERNEL32.DLL']:
    current=[s for s in d['sections'] if s['module'].lower()==name.lower()]
    old=[s for s in prior['sections'] if s['module'].lower()==name.lower()]
    assert current and current==old,(name,'captured code/base differs')

raw=(run/'traced-target/qualification-full.dmp').read_bytes()
n,drva=struct.unpack_from('<II',raw,8)
streams={}
for i in range(n):
    st,size,rva=struct.unpack_from('<III',raw,drva+12*i);streams[st]=(rva,size)
er=streams[6][0]
exception_tid=struct.unpack_from('<I',raw,er)[0]
exception_code=struct.unpack_from('<I',raw,er+8)[0]
context_size,context_rva=struct.unpack_from('<II',raw,er+160)
rip=struct.unpack_from('<Q',raw,context_rva+248)[0]
assert exception_tid==r['faultTid'] and exception_code==0xe0424354

named={
    'Work_RaiseException_call':base+0x15ef,
    'RaiseException_entry':0x7ffce03b5610,
    'exception_context_rip':rip,
    'CaptureQualificationException_entry':base+0x1040,
    'CaptureQualificationException_Wait_call':base+0x118b,
    'KernelBase_WaitForSingleObject':0x7ffce03b1be0,
    'KernelBase_WaitForSingleObjectEx':0x7ffce03041d0,
    'Kernel32_WaitForSingleObject':0x7ffce1787200,
}
files=[]
for f in d['fragments']:
    h=f['header'];stem=f"{h['index']:06d}-{h['qpc']}"
    files.append((dict(targetHeaderIndex=h['index'],qpc=h['qpc'],cpu=h['cpu'],
        secondsFromFault=h['secondsFromFault'],bufferBytes=h['bufferBytes']),selected/'private'/stem))
for f in extra['results']:
    if f['headerPidMatchesTarget'] and f['headerTidMatchesFaultThread']:
        files.append((dict(globalHeaderIndex=f['globalHeaderIndex'],qpc=f['qpc'],cpu=f['cpu'],
            secondsFromFault=f['secondsFromFault'],bufferBytes=f['bufferBytes']),follow/'private'/str(f['globalHeaderIndex'])))

results=[]
for row,stem in files:
    text=Path(str(stem)+'.instructions.txt').read_text()
    packet=Path(str(stem)+'.packets.txt').read_text()
    instructions=[]
    for line in text.splitlines():
        m=re.match(r'^\s*([0-9a-f]{16})\s+([0-9a-f]{16})\s+(.+)$',line)
        if m:instructions.append((int(m[2],16),line))
    ips=[x[0] for x in instructions]
    row.update(decodedInstructions=len(ips),lastInstruction=hex(ips[-1]) if ips else None,
        namedInstructionOccurrences={k:ips.count(v) for k,v in named.items()},
        captureHandlerRangeInstructions=sum(base+0x1040<=ip<=base+0x11b5 for ip in ips),
        raiseExceptionRangeInstructions=sum(0x7ffce03b5610<=ip<=0x7ffce03b56cf for ip in ips),
        packetTypes={k:len(re.findall(r'\b'+re.escape(k)+r'\b',packet)) for k in ['psb','psbend','cbr','tnt','tnt64','tip','tip.pge','tip.pgd','fup','ovf']},
        errors=[line for line in text.splitlines() if 'error' in line.lower()])
    results.append(row)

headers=json.loads((selected/'private/ipt-headers-private.json').read_text())
iterations=sum(b['iterations'] for b in r['batches'])
all_bytes=sum(h['bufferBytes'] for h in headers)
target_bytes=sum(h['bufferBytes'] for h in headers if h['pid']==r['pid'] and h['tid']==r['faultTid'])
proof=dict(completedWorkloadIterations=iterations,conservativeMinimumBranchOutcomes=iterations,
    allSystemPtRecords=len(headers),allSystemRawBufferBytes=all_bytes,
    allSystemGenerousBitBound=all_bytes*8,
    allSystemGenerousBitBoundFractionOfMinimum=all_bytes*8/iterations,
    entireMeasuredBranchHistoryCannotFit=all_bytes*8<iterations,
    targetHeaderRawBufferBytes=target_bytes,
    basis='One TNT bit per conditional outcome; exact ExecuteChunk executes two Jcc per completed iteration, but this bound uses only one. All raw system buffers, including headers, duplication and padding, are generously counted as useful target outcome bits.',
    limitation='Not an instruction-coverage percentage, not a short-fault-window impossibility proof, and not a cause of missing data.')
summary=dict(status='FAULT_PATH_NOT_RETAINED_IN_EXAMINED_TARGET_FRAGMENTS',pid=r['pid'],tid=r['faultTid'],
    faultQpc=r['faultQpc'],workEndQpc=r['workEndQpc'],exceptionCode=hex(exception_code),exceptionRip=hex(rip),
    exactExeSha256=hashlib.sha256((root/'qualification/PtQualification.exe').read_bytes()).hexdigest(),
    capturedCodeAndBasesMatchPriorExactSymbolMapping=True,fragments=results,
    allSelectedInstructions=sum(x['decodedInstructions'] for x in results),
    faultPathOccurrences=sum(sum(x['namedInstructionOccurrences'].values()) for x in results),
    allThreeAfterFaultTargetFragmentsDecoded=True,
    allThreeAfterFaultContainOnlyPsbCbrPsbend=True,
    wholeWorkloadCapacityBound=proof,
    conclusions=[
      'The four nonempty pre-fault fragments decode real ExecuteChunk/BranchEven/BranchOdd instructions, but none reaches Work RaiseException, RaiseException itself, captured exception RIP, or the exception handler/wait path.',
      'All three after-fault target-header fragments contain synchronization/frequency packets and no TIP/FUP/TNT, so zero instruction output is a substantive data limitation, not success.',
      'Exact CSwitch payload headers identify incoming threads in the sideband audit; envelope time and thread are not unconditionally the executed fragment tail.',
      'No same-fragment Work-to-exception-to-handler-to-wait sequence was established; the pre-original-node qualification gate remains unmet.',
      'This is the qualification executable declared exception, not a NOVOVM natural access violation or fastfail.',
      'PT does not supply per-instruction RBP values; no first RBP corruption was observed.'],
    caveats=['faultQpc is sampled inside CaptureQualificationException after copying the exception context.',
      'No independent fragments were concatenated or treated as continuous.',
      'A missing short path is demonstrated only for the selected fragments; all raw system PT has not been instruction-decoded.',
      'No target launch, new recording, production database access or production fix occurs in this audit.'])
assert summary['faultPathOccurrences']==0
for row in results:
    if row['qpc']>r['faultQpc']:
        assert row['decodedInstructions']==0
        assert all(row['packetTypes'][k]==0 for k in ['tnt','tnt64','tip','tip.pge','tip.pgd','fup'])
path=selected/'fault-path-public.json'
assert not path.exists()
path.write_text(json.dumps(summary,indent=2),encoding='utf-8')
print(json.dumps({k:summary[k] for k in ['status','pid','tid','allSelectedInstructions','faultPathOccurrences','wholeWorkloadCapacityBound']},indent=2))
