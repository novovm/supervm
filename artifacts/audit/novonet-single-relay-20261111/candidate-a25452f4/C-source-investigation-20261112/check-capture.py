import pathlib,struct,json
r=pathlib.Path(__file__).parent
out=[]
for pid in (13020,27928):
 b=(r/'dumps-local'/f'novovm-node.exe.{pid}.dmp').read_bytes(); n,d=struct.unpack_from('<II',b,8);ss={struct.unpack_from('<III',b,d+12*i)[0]:struct.unpack_from('<III',b,d+12*i)[1:] for i in range(n)}
 ranges=[]
 if 5 in ss:
  at=ss[5][1]
  for i in range(struct.unpack_from('<I',b,at)[0]):
   va,size,fo=struct.unpack_from('<QII',b,at+4+i*16);ranges.append((va,size,fo))
 if 9 in ss:
  at=ss[9][1];count,fo=struct.unpack_from('<QQ',b,at)
  for i in range(count):
   va,size=struct.unpack_from('<QQ',b,at+16+i*16);ranges.append((va,size,fo));fo+=size
 def read(va,size):
  for lo,sz,fo in ranges:
   if lo<=va and va+size<=lo+sz:return b[fo+va-lo:fo+va-lo+size]
  return None
 ctx=next(x['exception_registers'] for x in json.loads((r/'exception-context-local.json').read_text()) if x['pid']==pid)
 rip=int(ctx['rip'],16);rbp=int(ctx['rbp'],16);rsp=int(ctx['rsp'],16)
 raw=read(rip,16)
 out.append({'pid':pid,'instruction_captured_in_dump':raw is not None,'instruction_bytes':raw.hex() if raw else None,'rbp_minus_rsp':rbp-rsp,'rbp_plus_32_mod16':(rbp+32)%16,'operand_16_bytes_present':read(rbp+32,16) is not None})
(r/'capture-consistency-public.json').write_text(json.dumps(out,indent=2));print(json.dumps(out,indent=2))
