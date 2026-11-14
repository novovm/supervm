"""Read existing xperf/tracerpt exports only. Do not start tracing or targets."""
from pathlib import Path
import csv, collections, datetime, hashlib, json, re, xml.etree.ElementTree as ET
ROOT=Path(__file__).resolve().parents[1]
RUN=ROOT/"run-01"
OUT=Path(__file__).resolve().parent/"coverage-public-v3.json"
if OUT.exists(): raise SystemExit("Refusing to overwrite coverage evidence")
target=json.loads((RUN/"traced-target/result.json").read_text())
q0=target["workStartQpc"]; fault=target["faultQpc"]; freq=target["qpcFrequency"]
tid=str(target["workTid"])
headers={}; in_header=False; context=None; first_raw=None
all_stats=collections.defaultdict(list); by_cpu=collections.defaultdict(list)
target_events=[]; unknown_counts=collections.Counter(); pt_context={}; target_pid_events=[]
csv_path=RUN/"decode/events-private.csv"
rawre=re.compile(r"^#\s*(\d+)\s*#\s*(0x[0-9a-fA-F]+)\s*#\s*(\d+)")
# xperf text is not UTF-8. Latin-1 preserves every byte; only ASCII event names,
# numeric fields, and the target's ASCII process field are interpreted.
with csv_path.open(encoding="latin-1",errors="strict",newline="") as f:
    for line in f:
        if line.strip()=="BeginHeader": in_header=True; continue
        if line.strip()=="EndHeader": in_header=False; continue
        match=rawre.match(line)
        if match:
            context=dict(cpu=int(match[1]),qpc=int(match[2],16),relativeNs=int(match[3]))
            if first_raw is None: first_raw=context["qpc"]
            continue
        if not line.strip() or line.startswith("#"): continue
        row=[x.strip() for x in next(csv.reader([line],skipinitialspace=True))]
        if in_header:
            headers[row[0]]=row
            continue
        if len(row)<2 or not context or not row[1].isdigit(): continue
        kind=row[0]
        # Raw comment QPC is authoritative; xperf formatted TimeStamp is microseconds.
        event=dict(kind=kind,rawQpc=context["qpc"],relativeNs=context["relativeNs"],cpu=context["cpu"],formattedTimestampUs=int(row[1]))
        if kind.startswith("Sampled"):
            event["instructionPointer"]=row[4]
            event["resolvedThreadId"]=row[3]
        if kind.startswith(("P-","I-")) and len(row)>2 and re.search(r"\(\s*"+str(target["pid"])+r"\)\s*$",row[2]):
            target_pid_events.append(event)
        all_stats[kind].append(event)
        by_cpu[context["cpu"]].append(event)
        if kind=="UnknownEvent/Classic":
            unknown_counts[(row[5],row[6])]+=1
            pt_context.setdefault(context["qpc"],[]).append(dict(event,provider=row[5],opcode=row[6]))
        wanted=False
        if kind in ("SampledProfile","SampledProfileNmi","T-Start","T-End","T-DCStart","T-DCEnd") and len(row)>3 and row[3]==tid:
            wanted=True; event["processField"]=row[2]
            if kind.startswith("Sampled"): event["instructionPointer"]=row[4]
        if kind=="CSwitch" and len(row)>9 and (row[3]==tid or row[9]==tid):
            wanted=True; event["targetSwitchDirection"]="in" if row[3]==tid else "out"
            event["targetProcessField"]=row[2] if row[3]==tid else row[8]
            event["switchCpuField"]=int(row[16]); event["oldState"]=row[12]; event["waitReason"]=row[13]
        if wanted:
            event["secondsAfterWorkStart"]=(event["rawQpc"]-q0)/freq
            event["secondsBeforeFault"]=(fault-event["rawQpc"])/freq
            target_events.append(event)
def summary(events):
    if not events: return dict(count=0)
    qs=[e["rawQpc"] for e in events]
    return dict(count=len(events),firstQpc=min(qs),lastQpc=max(qs),
                firstAfterWorkSeconds=(min(qs)-q0)/freq,lastAfterWorkSeconds=(max(qs)-q0)/freq,
                cpus=sorted(set(e["cpu"] for e in events)))
ipt_path=RUN/"decode/ipt-headers-private.json"
ipt_text=ipt_path.read_bytes()
ipt=json.loads(ipt_text)
target_pt=[r for r in ipt if r["tid"]==int(tid) and r["pid"]==target["pid"]]
for r in target_pt:
    r["secondsAfterWorkStart"]=(r["qpc"]-q0)/freq
    r["secondsBeforeFault"]=(fault-r["qpc"])/freq
    r["xperfUnknownRecordsAtSameQpc"]=pt_context.get(r["qpc"],[])
event_cpu_summaries={str(cpu):{kind:summary([e for e in events if e["kind"]==kind]) for kind in ["CSwitch","SampledProfile","SampledProfileNmi","UnknownEvent/Classic"]} for cpu,events in sorted(by_cpu.items())}
# xperf sometimes reports Unknown/-1 when lifetime sideband is absent, even though
# raw SampledProfile payload ThreadId is present. Use tracerpt payload, then match
# the same relative QPC+IP in xperf, not a process-name guess.
ns={"e":"http://schemas.microsoft.com/win/2004/08/events/event"}
def wall100ns(s):
    match=re.fullmatch(r"(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)\.(\d+)(.*)",s)
    assert match
    dt=datetime.datetime.fromisoformat(match[1]).replace(tzinfo=datetime.timezone.utc)
    return int(dt.timestamp())*10_000_000+int((match[2]+"0000000")[:7]),match[3]
header_wall=None; raw_samples=[]
xml_path=RUN/"decode/tracerpt-private.xml"
for _,element in ET.iterparse(xml_path,events=["end"]):
    if element.tag!="{"+ns["e"]+"}Event": continue
    data={e.attrib.get("Name"):(e.text or "").strip() for e in element.findall("e:EventData/e:Data",ns)}
    timestamp=element.find("e:System/e:TimeCreated",ns)
    if "PerfFreq" in data and "StartTime" in data and header_wall is None:
        header_wall=wall100ns(timestamp.attrib["SystemTime"])
        assert int(data["PerfFreq"])==freq
    if data.get("ThreadId")==tid and "InstructionPointer" in data:
        event_wall=wall100ns(timestamp.attrib["SystemTime"])
        assert header_wall and event_wall[1]==header_wall[1]
        raw_qpc=first_raw+(event_wall[0]-header_wall[0])*freq//10_000_000
        ip=int(data["InstructionPointer"],16)
        sample_matches=[]
        for e in all_stats["SampledProfile"]:
            if e["rawQpc"]==raw_qpc:
                sample_matches.append(e)
        assert len(sample_matches)==1
        assert int(sample_matches[0]["instructionPointer"],16)==ip
        raw_samples.append(dict(rawPayloadThreadId=int(tid),instructionPointer=hex(ip),rawQpc=raw_qpc,
                                cpu=int(element.find("e:System/e:Execution",ns).attrib["ProcessorID"]),
                                secondsAfterWorkStart=(raw_qpc-q0)/freq,
                                secondsBeforeFault=(fault-raw_qpc)/freq,
                                xperfSameQpcEvent=sample_matches[0]))
    element.clear()
result=dict(
    purpose="read_only_same_run_coverage_not_root_cause",
    targetPid=target["pid"],targetTid=int(tid),qpcFrequency=freq,workStartQpc=q0,faultQpc=fault,
    faultAfterWorkSeconds=(fault-q0)/freq,
    firstXperfRawQpc=first_raw,
    timestampConversion=dict(formula="(rawQpc-workStartQpc)/qpcFrequency",xperfFormattedUnit="microseconds",commentRelativeUnit="nanoseconds",xmlWallClockNotUsed=True),
    targetEventKinds={kind:summary([e for e in target_events if e["kind"]==kind]) for kind in sorted(set(headers)|set(e["kind"] for e in target_events)) if kind.startswith(("Sampled","T-","CSwitch"))},
    targetEvents=target_events,targetPt=target_pt,rawPayloadTargetSamples=raw_samples,targetResolvedProcessImageEvents=target_pid_events,
    sourceSha256={"xperfEvents":hashlib.sha256(csv_path.read_bytes()).hexdigest(),"tracerptXml":hashlib.sha256(xml_path.read_bytes()).hexdigest(),"iptHeaderSnapshot":hashlib.sha256(ipt_text).hexdigest()},
    ptPacketCount=len(target_pt),ptFirstAfterWorkSeconds=min((r["secondsAfterWorkStart"] for r in target_pt),default=None),
    ptLastAfterWorkSeconds=max((r["secondsAfterWorkStart"] for r in target_pt),default=None),
    lastPtToFaultSeconds=(fault-max(r["qpc"] for r in target_pt))/freq if target_pt else None,
    globalEventKinds={kind:summary(events) for kind,events in all_stats.items() if kind in ("CSwitch","SampledProfile","SampledProfileNmi","T-Start","T-End","T-DCEnd","T-DCStart","P-Start","P-End","P-Zombie")},
    perCpuEventRanges=event_cpu_summaries,
    noClaim=["xperf targetEventKinds describes resolved CSV fields only; rawPayloadTargetSamples contains 6 raw ThreadId matches not resolved by xperf.","No reconstruction of scheduling outside retained CSwitch records.","No conclusion that missing target events were caused specifically by ring overwrite.","ETW counters zero do not prove continuous target PT or TID-to-process sideband retained.","PT raw decoding and per-packet loss status are the independent decoder task; this script only reviews event envelope timing."])
OUT.write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
print(json.dumps({k:result[k] for k in ["targetEventKinds","targetEvents","ptPacketCount","ptFirstAfterWorkSeconds","ptLastAfterWorkSeconds","lastPtToFaultSeconds","globalEventKinds"]},indent=2))
