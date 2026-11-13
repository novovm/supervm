from pathlib import Path
root = Path(__file__).parent
(root/'src').mkdir(exist_ok=True)
regs = ['rbx','rbp','rsi','rdi','r12','r13','r14','r15']
lines = ['.text','.globl abi_probe','abi_probe:']
lines += [f'push {r}' for r in regs]
lines += ['sub rsp, 280','mov [rsp+72], rcx','mov [rsp+64], r8','mov r11, rdx']
for i in range(6,16): lines += [f'movdqu [rsp+{80+16*(i-6)}], xmm{i}']
for i in range(4,8): lines += [f'mov rax, [r11+{i*8}]',f'mov [rsp+{i*8}], rax']
for i,r in enumerate(regs): lines += [f'mov {r}, {0x123456789abcdef+i}']
lines += ['mov rax, 0x76543210fedcba98','movq xmm0, rax','punpcklqdq xmm0, xmm0']
for i in range(6,16): lines += [f'movdqa xmm{i}, xmm0']
lines += ['mov rcx, [r11]','mov rdx, [r11+8]','mov r8, [r11+16]','mov r9, [r11+24]',
          'call qword ptr [rsp+72]','mov r10, [rsp+64]','mov [r10], eax','xor eax, eax']
for i,r in enumerate(regs):
    lines += [f'mov r10, {0x123456789abcdef+i}',f'cmp {r}, r10',f'je ok_{i}',f'or rax, {1<<i}',f'ok_{i}:']
lines += ['mov r10, 0x76543210fedcba98','movq xmm0, r10','punpcklqdq xmm0, xmm0']
for i in range(6,16):
    lines += [f'movdqa xmm1, xmm{i}','pcmpeqb xmm1, xmm0','pmovmskb r10d, xmm1','cmp r10d, 65535',
              f'je ok_x{i}',f'or rax, {1<<(i+2)}',f'ok_x{i}:']
for i in range(6,16): lines += [f'movdqu xmm{i}, [rsp+{80+16*(i-6)}]']
lines += ['add rsp, 280']+[f'pop {r}' for r in reversed(regs)]+['ret']
# Deliberate clobber controls prove the probe observes both register classes.
lines += ['.globl clobber_control','clobber_control:', 'xor ebx, ebx','pxor xmm6, xmm6','xor eax, eax','ret']
(root/'src/probe.S').write_text('\n'.join(lines)+'\n')
