import json, pathlib, struct, uuid
root=pathlib.Path(__file__).parent
out=[]
for pid in (13020,27928):
    b=(root/'dumps-local'/f'novovm-node.exe.{pid}.dmp').read_bytes()
    if b[:4]!=b'MDMP': raise ValueError('Not a minidump')
    count,directory=struct.unpack_from('<II',b,8)
    streams={}
    for i in range(count):
        kind,size,rva=struct.unpack_from('<III',b,directory+12*i)
        if rva+size>len(b): raise ValueError('Stream bounds')
        streams[kind]=(rva,size)
    m,_=streams[4]
    modules=[]
    for i in range(struct.unpack_from('<I',b,m)[0]):
        pos=m+4+108*i
        base,size,checksum,stamp,name=struct.unpack_from('<QIIII',b,pos)
        length=struct.unpack_from('<I',b,name)[0]
        path=b[name+4:name+4+length].decode('utf-16le')
        entry={'module':path.replace('\\','/').split('/')[-1], 'base':base,'size':size}
        cvsize,cvrva=struct.unpack_from('<II',b,pos+76)
        if cvsize>=24 and b[cvrva:cvrva+4]==b'RSDS':
            entry['pdb_guid']=str(uuid.UUID(bytes_le=b[cvrva+4:cvrva+20])).upper()
            entry['pdb_age']=struct.unpack_from('<I',b,cvrva+20)[0]
            entry['pdb_name']=b[cvrva+24:cvrva+cvsize].split(b'\0')[0].decode(errors='replace').replace('\\','/').split('/')[-1]
        modules.append(entry)
    e,_=streams[6]
    tid=struct.unpack_from('<I',b,e)[0]
    code,flags,record,address,nparams=struct.unpack_from('<IIQQI',b,e+8)
    params=list(struct.unpack_from('<15Q',b,e+40)[:nparams])
    module=next((x for x in modules if x['base']<=address<x['base']+x['size']),None)
    row={'pid':pid,'exception_thread_id':tid,'exception_code':hex(code),'instruction_module':module['module'] if module else None,'instruction_rva':hex(address-module['base']) if module else None,'access_kind':{0:'read',1:'write',8:'execute'}.get(params[0]) if params else None,'fault_address':hex(params[1]) if len(params)>1 else None,'node_pdb':{k:v for k,v in next(x for x in modules if x['module']=='novovm-node.exe').items() if k.startswith('pdb_')},'aoem_module_present':any(x['module']=='aoem_ffi.dll' for x in modules),'root_cause_confirmed':False}
    out.append(row)
    (root/f'modules-{pid}-local.json').write_text(json.dumps(modules,indent=2),encoding='utf-8')
(root/'exceptions-public.json').write_text(json.dumps(out,indent=2)+'\n',encoding='utf-8')
print(json.dumps(out,indent=2))
