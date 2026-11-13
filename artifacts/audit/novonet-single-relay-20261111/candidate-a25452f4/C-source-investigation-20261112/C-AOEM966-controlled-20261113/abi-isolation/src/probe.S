.text
.globl abi_probe
abi_probe:
push rbx
push rbp
push rsi
push rdi
push r12
push r13
push r14
push r15
sub rsp, 280
mov [rsp+72], rcx
mov [rsp+64], r8
mov r11, rdx
movdqu [rsp+80], xmm6
movdqu [rsp+96], xmm7
movdqu [rsp+112], xmm8
movdqu [rsp+128], xmm9
movdqu [rsp+144], xmm10
movdqu [rsp+160], xmm11
movdqu [rsp+176], xmm12
movdqu [rsp+192], xmm13
movdqu [rsp+208], xmm14
movdqu [rsp+224], xmm15
mov rax, [r11+32]
mov [rsp+32], rax
mov rax, [r11+40]
mov [rsp+40], rax
mov rax, [r11+48]
mov [rsp+48], rax
mov rax, [r11+56]
mov [rsp+56], rax
mov rbx, 81985529216486895
mov rbp, 81985529216486896
mov rsi, 81985529216486897
mov rdi, 81985529216486898
mov r12, 81985529216486899
mov r13, 81985529216486900
mov r14, 81985529216486901
mov r15, 81985529216486902
mov rax, 0x76543210fedcba98
movq xmm0, rax
punpcklqdq xmm0, xmm0
movdqa xmm6, xmm0
movdqa xmm7, xmm0
movdqa xmm8, xmm0
movdqa xmm9, xmm0
movdqa xmm10, xmm0
movdqa xmm11, xmm0
movdqa xmm12, xmm0
movdqa xmm13, xmm0
movdqa xmm14, xmm0
movdqa xmm15, xmm0
mov rcx, [r11]
mov rdx, [r11+8]
mov r8, [r11+16]
mov r9, [r11+24]
call qword ptr [rsp+72]
mov r10, [rsp+64]
mov [r10], eax
xor eax, eax
mov r10, 81985529216486895
cmp rbx, r10
je ok_0
or rax, 1
ok_0:
mov r10, 81985529216486896
cmp rbp, r10
je ok_1
or rax, 2
ok_1:
mov r10, 81985529216486897
cmp rsi, r10
je ok_2
or rax, 4
ok_2:
mov r10, 81985529216486898
cmp rdi, r10
je ok_3
or rax, 8
ok_3:
mov r10, 81985529216486899
cmp r12, r10
je ok_4
or rax, 16
ok_4:
mov r10, 81985529216486900
cmp r13, r10
je ok_5
or rax, 32
ok_5:
mov r10, 81985529216486901
cmp r14, r10
je ok_6
or rax, 64
ok_6:
mov r10, 81985529216486902
cmp r15, r10
je ok_7
or rax, 128
ok_7:
mov r10, 0x76543210fedcba98
movq xmm0, r10
punpcklqdq xmm0, xmm0
movdqa xmm1, xmm6
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x6
or rax, 256
ok_x6:
movdqa xmm1, xmm7
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x7
or rax, 512
ok_x7:
movdqa xmm1, xmm8
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x8
or rax, 1024
ok_x8:
movdqa xmm1, xmm9
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x9
or rax, 2048
ok_x9:
movdqa xmm1, xmm10
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x10
or rax, 4096
ok_x10:
movdqa xmm1, xmm11
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x11
or rax, 8192
ok_x11:
movdqa xmm1, xmm12
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x12
or rax, 16384
ok_x12:
movdqa xmm1, xmm13
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x13
or rax, 32768
ok_x13:
movdqa xmm1, xmm14
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x14
or rax, 65536
ok_x14:
movdqa xmm1, xmm15
pcmpeqb xmm1, xmm0
pmovmskb r10d, xmm1
cmp r10d, 65535
je ok_x15
or rax, 131072
ok_x15:
movdqu xmm6, [rsp+80]
movdqu xmm7, [rsp+96]
movdqu xmm8, [rsp+112]
movdqu xmm9, [rsp+128]
movdqu xmm10, [rsp+144]
movdqu xmm11, [rsp+160]
movdqu xmm12, [rsp+176]
movdqu xmm13, [rsp+192]
movdqu xmm14, [rsp+208]
movdqu xmm15, [rsp+224]
add rsp, 280
pop r15
pop r14
pop r13
pop r12
pop rdi
pop rsi
pop rbp
pop rbx
ret
.globl clobber_control
clobber_control:
xor ebx, ebx
pxor xmm6, xmm6
xor eax, eax
ret
