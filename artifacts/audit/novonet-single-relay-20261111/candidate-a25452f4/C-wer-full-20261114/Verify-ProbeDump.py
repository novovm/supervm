"""Read only the synthetic qualification dump; publish no memory contents."""
import hashlib,json,pathlib,struct,sys,time
root=pathlib.Path(sys.argv[1]);out=pathlib.Path(sys.argv[2])
m=json.loads((root/'probe-metadata.json').read_text('utf-8-sig'))
dumps=list((root/'dumps').glob('*.dmp'));assert len(dumps)==1
p=dumps[0];before=p.stat();data=p.read_bytes();sha=hashlib.sha256(data).hexdigest()
assert data[:4]==b'MDMP';flags=struct.unpack_from('<Q',data,24)[0];assert flags&2
n,dr=struct.unpack_from('<II',data,8)
s={}
for i in range(n):
    k,z,a=struct.unpack_from('<III',data,dr+12*i);assert a+z<=len(data);s[k]=(a,z)
assert 9 in s and 3 in s and 6 in s
a,z=s[9];count,off=struct.unpack_from('<QQ',data,a);assert 16+16*count<=z
regions=[]
for i in range(count):
    va,size=struct.unpack_from('<QQ',data,a+16+16*i);assert off+size<=len(data)
    regions.append((va,va+size,off));off+=size
def read(va,size):
    buf=bytearray();end=va+size
    while va<end:
        r=next((r for r in regions if r[0]<=va<r[1]),None);assert r is not None,'Missing memory'
        take=min(end,r[1])-va;buf.extend(data[r[2]+va-r[0]:r[2]+va-r[0]+take]);va+=take
    return bytes(buf)
heap=read(int(m['heap'],16),m['heap_bytes'])
assert heap==bytes((i*17+93)&255 for i in range(65536))
assert read(int(m['code'],16),32)==bytes.fromhex(m['code_hex'])
code_page=int(m['code'],16)&~4095;assert len(read(code_page,4096))==4096
a,z=s[3];threads=[]
for i in range(struct.unpack_from('<I',data,a)[0]):
    pos=a+4+48*i;tid=struct.unpack_from('<I',data,pos)[0]
    size,rva=struct.unpack_from('<II',data,pos+40);assert size>=1232 and rva+size<=len(data)
    threads.append(tid)
assert m['main_thread'] in threads and m['worker_thread'] in threads
a,z=s[6];code=struct.unpack_from('<I',data,a+8)[0];assert code==0xc0000005
time.sleep(1);after=p.stat();assert before.st_size==after.st_size and before.st_mtime_ns==after.st_mtime_ns
assert hashlib.sha256(p.read_bytes()).hexdigest()==sha
r={'qualified':True,'dump_name':p.name,'bytes':len(data),'sha256':sha,'flags':hex(flags),
   'memory64_ranges':count,'thread_count':len(threads),'all_listed_contexts_in_file':True,
   'expected_threads_present':True,'heap_pattern_bytes_verified':len(heap),'probe_code_bytes_verified':32,
   'probe_code_page_captured':True,'stable_two_hashes':True,'exception_code':hex(code),
   'method':'LocalService isolated probe; intentional unhandled RaiseException; no debugger or dump writer',
   'novovm_failure_captured':False}
out.write_bytes((json.dumps(r,indent=2)+'\n').encode());print(json.dumps(r))
