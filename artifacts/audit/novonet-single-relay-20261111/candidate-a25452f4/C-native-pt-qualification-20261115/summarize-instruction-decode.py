"""Emit whitelist-only public decode result from existing offline artifacts."""
from pathlib import Path
import hashlib,json,struct
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[1]
j=json.loads((HERE/'instruction-modules-public.json').read_text())
target=json.loads((HERE/'target-pt-public.json').read_text())
exe=(ROOT/'qualification/PtQualification.exe').read_bytes();pe=struct.unpack_from('<I',exe,0x3c)[0]
n=struct.unpack_from('<H',exe,pe+6)[0];opt=struct.unpack_from('<H',exe,pe+20)[0]
disk_text=None
for i in range(n):
    p=pe+24+opt+40*i
    if exe[p:p+8].split(b'\0')[0]==b'.text':
        vs,va,rawsize,rawat=struct.unpack_from('<IIII',exe,p+8);disk_text=exe[rawat:rawat+vs]
dump_text=(HERE/'instructions-modules-private/PtQualification.exe-text.bin').read_bytes()
assert disk_text==dump_text
fragments=j['fragments']
summary=dict(
    status='ACTUAL_RETAINED_PT_INSTRUCTION_DECODE_PASSED_FAULT_WINDOW_INCOMPLETE',
    pid=28712,tid=42560,moduleBase=j['moduleBase'],
    targetExeSha256=hashlib.sha256(exe).hexdigest(),targetPdbSha256=hashlib.sha256((ROOT/'qualification/PtQualification.pdb').read_bytes()).hexdigest(),
    sameRunDumpTextMatchesOriginalExe=True,sameRunDumpTextBytes=len(dump_text),sameRunDumpTextSha256=hashlib.sha256(dump_text).hexdigest(),
    fragmentCount=len(fragments),decodeExitCodes=[r['exitCode'] for r in fragments],decodeDiagnosticErrors=[r['errors'] for r in fragments],
    decodedInstructionLines=sum(r['decodedInstructionLines'] for r in fragments),
    functionEntryOccurrences={fn:sum(r['functionEntryOccurrences'][fn] for r in fragments) for fn in ['BranchEven','BranchOdd','ExecuteChunk']},
    mappedExecutableSections=len(j['sections']),missingExecutableSections=sum(not s['mapped'] for s in j['sections']),
    targetPtFirstQpc=min(r['qpc'] for r in target['target']),targetPtLastQpc=max(r['qpc'] for r in target['target']),
    faultQpc=2982258850210,qpcFrequency=10000000,faultMinusLastTargetPtSeconds=(2982258850210-max(r['qpc'] for r in target['target']))/10000000,
    actualInstructionExcerpt=fragments[0]['firstInstructions'][:14],
    tools={
       'libipt':dict(url='https://github.com/intel/libipt',commit='7f8575ce9bf25e48cf03983c12859e1edb592008',version='2.3.0-devel',
        ptxedSha256=j['ptxedSha256'],ptdumpSha256=hashlib.sha256((ROOT/'libipt/build-packet/bin/ptdump.exe').read_bytes()).hexdigest()),
       'xed':dict(url='https://github.com/intelxed/xed',commit='0bcb6237345c5066726dcc08b3d87928df3b5b26',version='v2026.08.23',librarySha256=hashlib.sha256((ROOT/'xed-install/lib/xed.lib').read_bytes()).hexdigest()),
       'mbuild':dict(url='https://github.com/intelxed/mbuild',commit='1b437e409221a2b5703b4d8896baa20d43e4ba1a')},
    buildFailuresPreserved=[
       'XED initial command output GBK decode failure; original build-isolated.log preserved.',
       'XED second attempt exposed MSVC C4819 source encoding warning treated as error; original log preserved.',
       'XED compiled with process-local CL=/utf-8 and PYTHONUTF8=1, no upstream source modifications.',
       'ptxed initial include path ended in include/xed; corrected to include as required by xed/xed-interface.h; both logs preserved.',
       'Initial EXE-only decode had four unmapped kernel32 addresses despite ptxed exit 0; added same-dump captured module code pages and all seven final diagnostics are clear.'],
    limitations=[
       'This is real WPR/PT data for an explicitly isolated qualification executable, not original NOVOVM crash evidence.',
       'Decoder output counts are retained fragment instruction occurrences, not a continuous full execution count.',
       'The final 7.7426714 seconds before the qualification exception have no target PT records in this ETL.',
       'ETW lost-event and lost-buffer counters of zero do not establish full hardware trace history.',
       'PT control flow does not supply each instruction RBP values; first RBP mismatch remains unlocated.',
       'The reason for the missing target interval is not proven; do not attribute it solely to ring overwrite.'])
(HERE/'decoder-final-public.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
print(json.dumps({k:summary[k] for k in ['status','fragmentCount','decodeExitCodes','decodedInstructionLines','functionEntryOccurrences','sameRunDumpTextMatchesOriginalExe','faultMinusLastTargetPtSeconds']},indent=2))
