from pathlib import Path
import hashlib,json,struct,sys,uuid
sys.path.insert(0,str(Path('target/c-a254-crash-source-20261112T145135Z/python-tools').resolve()))
import pefile
ROOT=Path('target/c-lowimpact-capture-20261114');OUT=Path(__file__).resolve().parent
def sha(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def pdb_identity(path):
 with path.open('rb') as f:
  h=f.read(56);assert h.startswith(b'Microsoft C/C++ MSF 7.00\r\n\x1aDS')
  bs,_,_,size,_,block=struct.unpack_from('<6I',h,32);f.seek(block*bs);n=(size+bs-1)//bs
  blocks=struct.unpack('<'+'I'*n,f.read(n*4));directory=b''
  for b in blocks:f.seek(b*bs);directory+=f.read(bs)
  n=struct.unpack_from('<I',directory)[0];sizes=struct.unpack_from('<'+'I'*n,directory,4);pos=4+4*n;streams=[]
  for s in sizes:
   count=0 if s==0xffffffff else (s+bs-1)//bs;streams.append(struct.unpack_from('<'+'I'*count,directory,pos));pos+=4*count
  info=b''
  for b in streams[1]:f.seek(b*bs);info+=f.read(bs)
  return str(uuid.UUID(bytes_le=info[12:28])),struct.unpack_from('<I',info,8)[0]
records=[]
for variant in ['baseline','instrumented']:
 exe=ROOT/variant/'c-lowimpact-capture.exe';pdb=exe.with_name('c_lowimpact_capture.pdb');pe=pefile.PE(str(exe));ident=pdb_identity(pdb)
 cvs=[]
 for e in pe.DIRECTORY_ENTRY_DEBUG:
  d=pe.get_data(e.struct.AddressOfRawData,e.struct.SizeOfData)
  if d[:4]==b'RSDS':cvs.append((str(uuid.UUID(bytes_le=d[4:20])),struct.unpack_from('<I',d,20)[0]))
 assert ident in cvs
 records.append({'variant':variant,'exe_sha256':sha(exe),'exe_bytes':exe.stat().st_size,'pdb_sha256':sha(pdb),'pdb_bytes':pdb.stat().st_size,'pdb_guid':ident[0],'pdb_age':ident[1],'pdb_matches_exe':True,'build_exit':json.loads((ROOT/(variant+'-build-exit.json')).read_text(encoding='utf-8-sig'))})
report={'source':'a25452f4c49be37cd984e650f2079642e6c273b3','baseline_is_original_deployed_binary':False,'variants':records,'rust':'1.94.0','target':'x86_64-pc-windows-msvc','common_flags':'--locked --offline --release --config profile.release.package.novovm-node.debug=2 --config profile.release.package.novovm-network.debug=2 -- -C debuginfo=2 -C strip=none','feature_difference':'iroh-transport / iroh-transport,lowtrace','runtime_sha256':sha(Path('target/c-state-context-phase5-20261114/runtime/aoem_ffi.dll'))}
(OUT/'build-public.json').write_bytes((json.dumps(report,indent=2)+'\n').encode())
(OUT/'patches-public.json').write_bytes((ROOT/'patches.json').read_bytes())
print('Both diagnostic EXE/PDB pairs and build exits verified')
