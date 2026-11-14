"""Inspect privately copied dumps only; publish coverage and module identifiers, not memory."""
import hashlib
import json
import pathlib
import struct
import uuid

OUT = pathlib.Path(__file__).resolve().parent
ROOT = OUT.parents[4]
launch = json.loads((ROOT/'target/c-localservice-collection-launch-20261114.json').read_text('utf-8'))
private = pathlib.Path(launch['evidence_root'])
inventory = json.loads((private/'inventory-public.json').read_text('utf-8'))
prior = json.loads((OUT.parent/'C-failure-evidence-gate-20261114/dump-coverage-public.json').read_text('utf-8'))
previous = {x['pid']:x for x in prior['dumps']}
original_five = {13020,27928,31296,3228,37796}
rows=[]

def write(name,obj):
    (OUT/name).write_bytes((json.dumps(obj,indent=2,ensure_ascii=False)+'\n').encode())

for item in inventory['files']:
    if not item['dump']:
        continue
    path=private/item['private_copy']
    data=path.read_bytes()
    assert item['source_stable'] and hashlib.sha256(data).hexdigest()==item['sha256']
    pid=int(path.name.split('.')[-2])
    count,directory=struct.unpack_from('<II',data,8)
    streams={}
    for i in range(count):
        kind,size,rva=struct.unpack_from('<III',data,directory+12*i)
        assert rva+size<=len(data)
        if kind: streams[kind]=(rva,size)
    # MDMP memory descriptors reference private copies, never substitute disk EXEs.
    regions=[]
    if 5 in streams:
        at,_=streams[5]
        for i in range(struct.unpack_from('<I',data,at)[0]):
            va,size,rva=struct.unpack_from('<QII',data,at+4+16*i)
            assert rva+size<=len(data)
            regions.append((va,va+size))
    if 9 in streams:
        at,_=streams[9]; n,base=struct.unpack_from('<QQ',data,at)
        for i in range(n):
            va,size=struct.unpack_from('<QQ',data,at+16+16*i)
            regions.append((va,va+size))
    regions.sort()
    merged=[]
    for a,b in regions:
        if merged and a<=merged[-1][1]: merged[-1][1]=max(b,merged[-1][1])
        else: merged.append([a,b])
    def coverage(a,size):
        end=a+size; cursor=a; missing=[]; captured=0
        for x,y in merged:
            if y<=cursor or x>=end: continue
            if x>cursor: missing.append([cursor,min(x,end)])
            lo,hi=max(cursor,x),min(y,end)
            if hi>lo: captured+=hi-lo; cursor=hi
            if cursor>=end: break
        if cursor<end: missing.append([cursor,end])
        return {'bytes':size,'captured_bytes':captured,'complete':not missing,
                'missing_ranges':[{'start':hex(x),'end_exclusive':hex(y)} for x,y in missing]}
    at,_=streams[4]; modules=[]
    for i in range(struct.unpack_from('<I',data,at)[0]):
        pos=at+4+108*i
        base,size,_,timestamp,name=struct.unpack_from('<QIIII',data,pos)
        nchars=struct.unpack_from('<I',data,name)[0]
        label=data[name+4:name+4+nchars].decode('utf-16le').replace('\\','/').split('/')[-1]
        m={'module':label,'base':base,'image_bytes':size,'pe_timestamp':hex(timestamp)}
        cvsize,cvrva=struct.unpack_from('<II',data,pos+76)
        if cvsize>=24 and data[cvrva:cvrva+4]==b'RSDS':
            m['pdb_guid']=str(uuid.UUID(bytes_le=data[cvrva+4:cvrva+20])).upper()
            m['pdb_age']=struct.unpack_from('<I',data,cvrva+20)[0]
        modules.append(m)
    ex,_=streams[6]
    tid=struct.unpack_from('<I',data,ex)[0]
    code,_,_,ip,nparams=struct.unpack_from('<IIQQI',data,ex+8)
    params=list(struct.unpack_from('<15Q',data,ex+40)[:nparams])
    ctxsize,ctxrva=struct.unpack_from('<II',data,ex+160)
    assert ctxrva+ctxsize<=len(data)
    fault=next((m for m in modules if m['base']<=ip<m['base']+m['image_bytes']),None)
    threadat,_=streams[3]
    n=struct.unpack_from('<I',data,threadat)[0]
    contexts=[]
    for i in range(n):
        pos=threadat+4+48*i
        threadid=struct.unpack_from('<I',data,pos)[0]
        va=struct.unpack_from('<Q',data,pos+24)[0]
        stacksize,stackrva,csize,crva=struct.unpack_from('<IIII',data,pos+32)
        assert stackrva+stacksize<=len(data) and crva+csize<=len(data)
        contexts.append({'thread_id':threadid,'context_bytes':csize,'context_in_file':csize>0,
                         'context_flags':hex(struct.unpack_from('<I',data,crva+48)[0]) if csize>=52 else None,
                         'stack_bytes':stacksize})
    probes=[]
    if pid in previous:
        assert item['sha256']==previous[pid]['sha256'], 'Do not reuse addresses across runs'
        for probe in previous[pid]['probes']:
            va=int(probe['va'],16)
            probes.append({'label':probe['label'],'va':probe['va'],**coverage(va,probe['bytes']),
                'containing_4k_pages':[{'va':hex(page),**coverage(page,4096)} for page in range(va & ~4095,(va+probe['bytes']+4095)&~4095,4096)]})
    tables=[]
    if 13 in streams:
        tableat,tablesize=streams[13]
        hs,ds,ns,es,nt,pad=struct.unpack_from('<6I',data,tableat)
        assert hs>=24 and ds>=32 and es==12
        cursor=tableat+hs+pad
        for i in range(nt):
            assert cursor+ds+ns<=tableat+tablesize
            lo,hi,base,entries,align=struct.unpack_from('<QQQII',data,cursor)
            entryat=cursor+ds+ns
            assert entryat+entries*es+align<=tableat+tablesize
            ranges=[]
            for k in range(entries):
                a,b,u=struct.unpack_from('<III',data,entryat+k*es)
                ranges.append((base+a,base+b))
            aoem=next((m for m in modules if m['module']=='aoem_ffi.dll'),None)
            if aoem and lo<aoem['base']+aoem['image_bytes'] and hi>=aoem['base']:
                tables.append({'descriptor_index':i,'entry_count':entries,
                    'descriptor_overlaps_aoem':True,
                    'candidate_875_in_entry':any(a<=aoem['base']+0x6e7158<b for a,b in ranges),
                    'candidate_966_in_entry':any(a<=aoem['base']+0x6ee998<b for a,b in ranges),
                    'zero_entries_do_not_prove_no_live_registration':entries==0})
            cursor=entryat+entries*es+align
        assert cursor<=tableat+tablesize
        padding=data[cursor:tableat+tablesize]
        assert not any(padding), 'Unexpected unparsed function-table bytes'
        function_tables={'descriptors':nt,'parsed_bytes':cursor-tableat,'stream_bytes':tablesize,'trailing_zero_bytes':len(padding),'aoem_overlaps':tables,
                         'limit':'Only entries actually captured; not a complete proof of live runtime registration'}
    else: function_tables={'present':False}
    selected=[]
    for m in modules:
        if m['module'] in ['novovm-node.exe','aoem_ffi.dll','TTDRecordCPU.dll'] or m is fault:
            selected.append({k:v for k,v in m.items() if k!='base'})
    rows.append({'pid':pid,'sha256':item['sha256'],'dump_utc':item['dump']['timestamp_utc'],
                 'part_of_prior_five':pid in original_five,'known_ttd_recorder_run':pid==39992,
                 'full_memory_flag':item['dump']['full_memory_flag'],
                 'captured_memory_union_bytes':sum(y-x for x,y in merged),
                 'exception_code':hex(code),'exception_thread_id':tid,
                 'exception_context_bytes':ctxsize,'fault_module':fault['module'] if fault else None,
                 'fault_rva':hex(ip-fault['base']) if fault else None,
                 'access_kind':{0:'read',1:'write',8:'execute'}.get(params[0]) if code==0xc0000005 and params else None,
                 'exception_target_address':hex(params[1]) if code==0xc0000005 and len(params)>1 else None,
                 'fault_code_16_bytes':coverage(ip,16), 'threads':contexts,
                 'modules':selected,'same_failure_probes':probes,'function_table_stream':function_tables})

write('coverage-public.json', {'no_disk_image_fallback':True,'dumps':rows,'first_corrupting_instruction_found':False})
write('inventory-public.json',inventory)
write('collection-exit.json',json.loads((private/'exit.json').read_text('utf-8')))
print(json.dumps([{'pid':r['pid'],'time':r['dump_utc'],'module':r['fault_module'],'rva':r['fault_rva'],
                  'node':next(m for m in r['modules'] if m['module']=='novovm-node.exe'),
                  'threads':len(r['threads']),'contexts_complete':all(t['context_in_file'] for t in r['threads']),
                  'full':r['full_memory_flag']} for r in rows],indent=2))
