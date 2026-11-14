from pathlib import Path
import hashlib,json,struct,sys,uuid
HERE=Path(__file__).resolve().parent;ROOT=Path('target/c-partitioned-writer-20261114')
old=HERE.parent/'C-lowimpact-capture-20261114/manifest.py'
code=old.read_text(encoding='utf-8');code=code[:code.index('records=[]')]
scope={'__file__':str(old)};exec(compile(code,str(old),'exec'),scope)
pdb_identity=scope['pdb_identity'];pefile=scope['pefile'];sha=scope['sha']
records=[]
for variant in ['baseline','instrumented']:
 exe=ROOT/variant/'c-lowimpact-capture.exe';pdb=exe.with_name('c_lowimpact_capture.pdb');ident=pdb_identity(pdb);pe=pefile.PE(str(exe))
 cvs=[]
 for e in pe.DIRECTORY_ENTRY_DEBUG:
  d=pe.get_data(e.struct.AddressOfRawData,e.struct.SizeOfData)
  if d[:4]==b'RSDS':cvs.append((str(uuid.UUID(bytes_le=d[4:20])),struct.unpack_from('<I',d,20)[0]))
 assert ident in cvs
 records.append(dict(variant=variant,exe_sha256=sha(exe),pdb_sha256=sha(pdb),exe_bytes=exe.stat().st_size,pdb_bytes=pdb.stat().st_size,guid=ident[0],age=ident[1],pdb_matches_exe=True,build=json.loads((ROOT/variant/'build-public.json').read_text())))
installed=Path('C:/Users/leadb/.rustup/toolchains/1.94.0-x86_64-pc-windows-msvc/lib/rustlib/src/rust/library')
checks=[];seen=set()
for patch in json.loads((ROOT/'std-patches.json').read_text()):
 name=patch['path']
 if name in seen:continue
 seen.add(name);actual=sha(installed/name);assert actual==patch['original_sha256']
 checks.append(dict(path=name,installed_unchanged=True,installed_sha256=actual,diagnostic_sha256=sha(ROOT/'library'/name)))
failures=[]
for d in sorted(ROOT.glob('instrumented-*-failed')):
 p=d/'build-public.json'
 failures.append(dict(directory=d.name,build=json.loads(p.read_text()) if p.exists() else {'not_started':'setup not yet ready; no compiler executed'},log_sha256=sha(d/'build.log') if (d/'build.log').exists() else None))
out=dict(source='a25452f4c49be37cd984e650f2079642e6c273b3',rust='1.94.0',variants=records,installed_std_checks=checks,local_failed_attempts=failures,
 cargo_encoded_rustflags=['-C','metadata=c_partition_callback_v2'],
 runtime_sha256=sha(Path('target/c-state-context-phase5-20261114/runtime/aoem_ffi.dll')),
 warning='Custom std changes ABI-observation code generation; this is not the original executable or an original-register trace',
 cargo_source_override_reference='https://github.com/rust-lang/cargo/blob/rust-1.94.0/src/cargo/core/compiler/standard_lib.rs#L208-L211')
(HERE/'build-public.json').write_bytes((json.dumps(out,indent=2)+'\n').encode())
print('EXE/PDB, runtime and unchanged installed std sources verified')
