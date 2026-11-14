/* WER qualification only. No NOVOVM data, handler, debugger or dump writer. */
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
static HANDLE ready, release_event;
static DWORD WINAPI worker(void *p) {
    SetEvent(ready);
    WaitForSingleObject(release_event, INFINITE);
    return 0;
}
__declspec(noinline) static void fault_site(void) {
    ULONG_PTR args[2] = {1, 1};
    RaiseException(EXCEPTION_ACCESS_VIOLATION, EXCEPTION_NONCONTINUABLE, 2, args);
}
int wmain(int argc, wchar_t **argv) {
    if (argc != 2) return 10;
    unsigned char *heap = HeapAlloc(GetProcessHeap(), 0, 65536);
    if (!heap) return 11;
    for (size_t i=0;i<65536;i++) heap[i]=(unsigned char)((i*17+93)&255);
    ready=CreateEventW(NULL,TRUE,FALSE,NULL);
    release_event=CreateEventW(NULL,TRUE,FALSE,NULL);
    DWORD worker_id=0;
    HANDLE thread=CreateThread(NULL,0,worker,NULL,0,&worker_id);
    if (!thread || WaitForSingleObject(ready,5000)!=WAIT_OBJECT_0) return 12;
    FILE *f=NULL;
    if (_wfopen_s(&f,argv[1],L"wb") || !f) return 13;
    fprintf(f,"{\"pid\":%lu,\"main_thread\":%lu,\"worker_thread\":%lu,\"heap\":\"0x%llx\",\"heap_bytes\":65536,\"code\":\"0x%llx\",\"code_hex\":\"",GetCurrentProcessId(),GetCurrentThreadId(),worker_id,(unsigned long long)(uintptr_t)heap,(unsigned long long)(uintptr_t)fault_site);
    for(int i=0;i<32;i++) fprintf(f,"%02x",((unsigned char *)(uintptr_t)fault_site)[i]);
    fprintf(f,"\"}\n"); fflush(f); fclose(f);
    fault_site();
    return 14;
}
