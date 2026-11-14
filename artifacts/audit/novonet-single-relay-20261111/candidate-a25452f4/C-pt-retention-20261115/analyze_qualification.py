"""Offline-only qualification evidence check; no process or database access."""
from pathlib import Path
import hashlib, json, statistics, struct

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / "run-file01"
OUT = Path(__file__).resolve().parent / "qualification-analysis-public.json"
if OUT.exists():
    raise SystemExit("Refusing to overwrite existing analysis")

def load(path): return json.loads(path.read_text(encoding="utf-8-sig"))
base=load(RUN/"baseline-target/result.json")
trace=load(RUN/"traced-target/result.json")
def perf(result):
    frequency=result["qpcFrequency"]
    batches=result["batches"]
    rates=[b["iterations"] / ((b["endQpc"]-b["beginQpc"])/frequency) for b in batches]
    seconds=(result["workEndQpc"]-result["workStartQpc"])/frequency
    cpu=sum(b["kernel100ns"]+b["user100ns"] for b in batches)/1e7
    iterations=sum(b["iterations"] for b in batches)
    return dict(pid=result["pid"],tid=result["workTid"],batchCount=len(batches),wallSeconds=seconds,
                iterations=iterations,iterationsPerWallSecond=iterations/seconds,
                threadCpuSeconds=cpu,threadCpuSecondsPerWallSecond=cpu/seconds,
                nsThreadCpuPerIteration=cpu*1e9/iterations,
                cyclesPerIteration=sum(b["cycles"] for b in batches)/iterations,
                batchRateMin=min(rates),batchRateMedian=statistics.median(rates),batchRateMax=max(rates),
                batchIterationsPerSecond=rates)
bperf,tperf=perf(base),perf(trace)

path=RUN/"traced-target/qualification-full.dmp"
raw=path.read_bytes()
u32=lambda off:struct.unpack_from("<I",raw,off)[0]
u64=lambda off:struct.unpack_from("<Q",raw,off)[0]
assert raw[:4]==b"MDMP"
stream_count,dir_rva=u32(8),u32(12)
flags=u64(24)
streams={}
for i in range(stream_count):
    kind,size,rva=struct.unpack_from("<III",raw,dir_rva+12*i)
    assert rva+size<=len(raw)
    streams[kind]=(rva,size)
assert 3 in streams and 6 in streams and 9 in streams and flags&2
ex=streams[6][0]
ex_tid=u32(ex)
ex_code=u32(ex+8)
ex_address=u64(ex+24)
ctx_size,ctx_rva=struct.unpack_from("<II",raw,ex+160)
assert ctx_rva+ctx_size<=len(raw) and ctx_size>=256
regs={name:hex(u64(ctx_rva+off)) for name,off in (("rsp",152),("rbp",160),("rip",248))}
thread_rva=streams[3][0]
thread_count=u32(thread_rva)
threads=[]
for i in range(thread_count):
    off=thread_rva+4+48*i
    tid=u32(off)
    size,rva=struct.unpack_from("<II",raw,off+40)
    threads.append(dict(tid=tid,contextBytes=size,contextInFile=(rva+size<=len(raw))))
mem_rva=streams[9][0]
mem_count,mem_base=u64(mem_rva),u64(mem_rva+8)
ranges=[struct.unpack_from("<QQ",raw,mem_rva+16+i*16) for i in range(mem_count)]
mapped_bytes=sum(length for _,length in ranges)
assert mem_base+mapped_bytes<=len(raw)
def covered(address): return any(start<=address<start+length for start,length in ranges)
modules=[]
if 4 in streams:
    rva=streams[4][0]
    for i in range(u32(rva)):
        off=rva+4+i*108
        start,size,name_rva=u64(off),u32(off+8),u32(off+20)
        length=u32(name_rva)
        name=raw[name_rva+4:name_rva+4+length].decode("utf-16le")
        if start<=int(regs["rip"],16)<start+size:
            modules.append(dict(name=name.replace("\\","/").rsplit("/",1)[-1],base=hex(start),bytes=size))
matches={name:int(regs[name],16)==int(trace[name],16) for name in regs}
assert all(matches.values())
assert ex_tid==trace["faultTid"]==trace["workTid"]
assert ex_code==int(trace["exceptionCode"],16)==0xe0424354
assert any(t["tid"]==ex_tid for t in threads)
ident=load(RUN/"traced-target/identity.json")
misc=streams.get(15)
dump_pid=None
if misc and misc[1]>=12 and u32(misc[0]+4)&1:
    dump_pid=u32(misc[0]+8)
assert dump_pid==trace["pid"]==ident["pid"]
result=dict(
    kind="qualification_only_not_novovm_root_cause",
    baseline=bperf,traced=tperf,
    comparison=dict(
        throughputChangePercent=(tperf["iterationsPerWallSecond"]/bperf["iterationsPerWallSecond"]-1)*100,
        medianBatchThroughputChangePercent=(tperf["batchRateMedian"]/bperf["batchRateMedian"]-1)*100,
        threadCpuPerIterationChangePercent=(tperf["nsThreadCpuPerIteration"]/bperf["nsThreadCpuPerIteration"]-1)*100,
        cyclesPerIterationChangePercent=(tperf["cyclesPerIteration"]/bperf["cyclesPerIteration"]-1)*100),
    dump=dict(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest(),flags=hex(flags),fullMemory=bool(flags&2),
              threadInfo=bool(flags&0x1000),fullMemoryInfo=bool(flags&0x800),streamTypes=sorted(streams),
              memory64RangeCount=mem_count,memory64Bytes=mapped_bytes,threadCount=thread_count,threads=threads,
              pid=dump_pid,exceptionTid=ex_tid,exceptionCode=hex(ex_code),exceptionAddress=hex(ex_address),
              exceptionRegisters=regs,jsonRegisterMatches=matches,exceptionRipModule=modules,
              exceptionRipMemoryCovered=covered(int(regs["rip"],16)),exceptionRspMemoryCovered=covered(int(regs["rsp"],16))),
    limitations=[
        "Sequential single baseline and traced runs; 15 one-second batches are not independent long-term samples.",
        "No P95/P99 or production/NOVONET low-interference qualification is claimed.",
        "Thread CPU only excludes system-wide trace overhead; clock/power/scheduler order may confound the difference.",
        "Fault is declared qualification exception 0xE0424354 after measured work, not natural RBP corruption.",
        "Dump ExceptionStream matches original filter CONTEXT. ThreadList contexts reflect dump time and are not substituted for exception context.",
        "RBP=0 in this optimized qualification program is not a register-preservation violation.",
        "This analysis does not decode PT or prove ETL thread association; main-task decoder must check it independently."
    ])
OUT.write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
print(json.dumps({"comparison":result["comparison"],"dump":result["dump"]},indent=2))
