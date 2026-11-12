import pathlib,struct,json
r=pathlib.Path(__file__).parent
out=[]
for pid in (13020,27928):
 b=(r/'dumps-local'/f'novovm-node.exe.{pid}.dmp').read_bytes()
 n,d=struct.unpack_from('<II',b,8); ss={struct.unpack_from('<III',b,d+12*i)[0]:struct.unpack_from('<III',b,d+12*i)[1:] for i in range(n)}
 e=ss[6][1]; size,at=struct.unpack_from('<II',b,e+160)
 regs={k:hex(struct.unpack_from('<Q',b,at+off)[0]) for k,off in [('rax',120),('rcx',128),('rdx',136),('rbx',144),('rsp',152),('rbp',160),('rsi',168),('rdi',176),('r8',184),('r9',192),('r10',200),('r11',208),('r12',216),('r13',224),('r14',232),('r15',240),('rip',248)]}
 row={'pid':pid,'context_size':size,'exception_registers':regs};out.append(row)
(r/'exception-context-local.json').write_text(json.dumps(out,indent=2));print(json.dumps(out,indent=2))
