; Isolated qualification only. No patching/injection into an existing process.
; CallCapture(record, target, four_integer_arguments).
; We intentionally save RBP so a controlled violating fixture cannot corrupt
; the qualification caller. Consumers must stop on mismatch, never use this
; as a production ABI repair shim.
option casemap:none
.code
CallCapture PROC FRAME
    push rbp
    .pushreg rbp
    sub rsp, 40h
    .allocstack 40h
    .endprolog
    mov [rsp+20h], rcx
    mov [rcx+0], rbp
    lea rax, [rsp+48h]
    mov [rcx+8], rax
    mov rax, [rsp+48h]
    mov [rcx+16], rax
    mov [rcx+24], rbp
    mov [rcx+32], rsp
    mov qword ptr [rcx+64], 1
    mov [rcx+72], rdx
    mov rax, rdx
    mov r10, r8
    mov rcx, [r10]
    mov rdx, [r10+8]
    mov r8, [r10+16]
    mov r9, [r10+24]
    call rax
PUBLIC CaptureReturn
CaptureReturn LABEL BYTE
    mov r10, [rsp+20h]
    mov [r10+40], rbp
    mov [r10+48], rsp
    lea r11, CaptureReturn
    mov [r10+56], r11
    mov qword ptr [r10+64], 2
    add rsp, 40h
    pop rbp
    ret
CallCapture ENDP

; Fixtures write only to their own four-word witness buffer.
; They are deliberately separate from the actual heap qualification workload.
LegalUse PROC FRAME
    push rbp
    .pushreg rbp
    .endprolog
    mov [rcx], rbp
    mov rbp, 12345678h
    mov [rcx+8], rbp
    ; Snapshot the saved restoration value before a contiguous pop/ret epilogue.
    ; The wrapper's post-call record independently proves actual restoration.
    mov rax, [rsp]
    mov [rcx+16], rax
    mov eax, 1
    pop rbp
    ret
LegalUse ENDP

BadUse PROC
    mov [rcx], rbp
    lea rax, BadWrite
    mov [rcx+24], rax
PUBLIC BadWrite
BadWrite LABEL BYTE
    xor rbp, 4000h
    mov [rcx+8], rbp
    mov [rcx+16], rbp
    mov eax, 1
    ret
BadUse ENDP
END
