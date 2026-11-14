#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <dbghelp.h>
#include <stdint.h>
#include <stdio.h>
#include <wchar.h>

// Standalone PT/dump-chain qualification only. Never a NOVOVM reproducer.
// No injected register violation, debugger, registry change, or node state.
static const DWORD kQualificationException = 0xE0424354;
static const unsigned kBatchCount = 15;
static const unsigned kChunkIterations = 16384;
static wchar_t gOutput[MAX_PATH];
static HANDLE gDumpRequested, gDumpComplete;
static EXCEPTION_RECORD gException;
static CONTEXT gContext;
static EXCEPTION_POINTERS gPointers = { &gException, &gContext };
static DWORD gFaultTid;
static LARGE_INTEGER gFaultQpc;
static DWORD gDumpError;
static BOOL gDumpOk;
static volatile uint64_t gSink;
struct Batch { LONGLONG begin, end; ULONGLONG kernel100ns, user100ns, cycles, iterations; };
static Batch gBatches[kBatchCount];
static LARGE_INTEGER gFrequency, gWorkStart, gWorkEnd;
static DWORD gWorkTid;
static BOOL gFaultMode;

static ULONGLONG FileTicks(const FILETIME& f) {
    ULARGE_INTEGER v; v.LowPart=f.dwLowDateTime; v.HighPart=f.dwHighDateTime; return v.QuadPart;
}
__declspec(noinline) uint64_t BranchEven(uint64_t x) { return (x * 0x9e3779b97f4a7c15ULL) ^ (x >> 17); }
__declspec(noinline) uint64_t BranchOdd(uint64_t x) { return (x * 0xbf58476d1ce4e5b9ULL) + (x << 13); }
__declspec(noinline) uint64_t ExecuteChunk(uint64_t x) {
    for (unsigned i=0; i<kChunkIterations; ++i) {
        x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
        if (x & 1) x=BranchOdd(x); else x=BranchEven(x);
    }
    return x;
}
static bool OutputPath(wchar_t* out, size_t count, const wchar_t* name) {
    return swprintf_s(out,count,L"%s\\%s",gOutput,name)>0;
}
static DWORD WINAPI DumpWriter(void*) {
    if (WaitForSingleObject(gDumpRequested,20000)!=WAIT_OBJECT_0) { gDumpError=WAIT_TIMEOUT; SetEvent(gDumpComplete); return 1; }
    wchar_t path[MAX_PATH]; OutputPath(path,MAX_PATH,L"qualification-full.dmp");
    HANDLE file=CreateFileW(path,GENERIC_WRITE,0,nullptr,CREATE_NEW,FILE_ATTRIBUTE_NORMAL,nullptr);
    if (file==INVALID_HANDLE_VALUE) { gDumpError=GetLastError(); SetEvent(gDumpComplete); return 2; }
    MINIDUMP_EXCEPTION_INFORMATION ex={gFaultTid,&gPointers,FALSE};
    const MINIDUMP_TYPE flags=static_cast<MINIDUMP_TYPE>(MiniDumpWithFullMemory|MiniDumpWithThreadInfo|MiniDumpWithFullMemoryInfo|MiniDumpWithUnloadedModules);
    gDumpOk=MiniDumpWriteDump(GetCurrentProcess(),GetCurrentProcessId(),file,flags,&ex,nullptr,nullptr);
    gDumpError=gDumpOk?ERROR_SUCCESS:GetLastError();
    FlushFileBuffers(file); CloseHandle(file); SetEvent(gDumpComplete); return gDumpOk?0:3;
}
static LONG CaptureQualificationException(EXCEPTION_POINTERS* ex) {
    gException=*ex->ExceptionRecord; gException.ExceptionRecord=nullptr;
    gContext=*ex->ContextRecord;
    gFaultTid=GetCurrentThreadId(); QueryPerformanceCounter(&gFaultQpc);
    SetEvent(gDumpRequested);
    const DWORD waitResult=WaitForSingleObject(gDumpComplete,15000);
    if (waitResult!=WAIT_OBJECT_0) { gDumpError=waitResult==WAIT_TIMEOUT?WAIT_TIMEOUT:GetLastError(); }
    return EXCEPTION_EXECUTE_HANDLER;
}
static DWORD WINAPI Work(void*) {
    gWorkTid=GetCurrentThreadId(); QueryPerformanceCounter(&gWorkStart);
    uint64_t value=0xa254966ULL;
    for (unsigned i=0; i<kBatchCount; ++i) {
        FILETIME c,e,k0,u0,k1,u1; ULONGLONG cycles0=0,cycles1=0;
        GetThreadTimes(GetCurrentThread(),&c,&e,&k0,&u0); QueryThreadCycleTime(GetCurrentThread(),&cycles0);
        LARGE_INTEGER begin,now; QueryPerformanceCounter(&begin);
        ULONGLONG iterations=0;
        do { value=ExecuteChunk(value); iterations+=kChunkIterations; QueryPerformanceCounter(&now); }
        while (now.QuadPart-begin.QuadPart<gFrequency.QuadPart);
        QueryThreadCycleTime(GetCurrentThread(),&cycles1); GetThreadTimes(GetCurrentThread(),&c,&e,&k1,&u1);
        gBatches[i]={begin.QuadPart,now.QuadPart,FileTicks(k1)-FileTicks(k0),FileTicks(u1)-FileTicks(u0),cycles1-cycles0,iterations};
    }
    gSink=value; QueryPerformanceCounter(&gWorkEnd);
    if (gFaultMode) {
        // A declared qualification exception after measured work, not a memory corruption.
        __try { RaiseException(kQualificationException,EXCEPTION_NONCONTINUABLE,0,nullptr); }
        __except(CaptureQualificationException(GetExceptionInformation())) { return kQualificationException; }
    }
    return 0;
}
static bool WriteIdentity(const FILETIME& createTime) {
    wchar_t path[MAX_PATH]; OutputPath(path,MAX_PATH,L"identity.json");
    FILE* f=nullptr; if (_wfopen_s(&f,path,L"wx") || !f) return false;
    fprintf(f,"{\n  \"kind\":\"isolated_pt_qualification_not_novovm\",\n  \"pid\":%lu,\n  \"processCreateFileTime\":%llu,\n  \"qpcFrequency\":%lld,\n  \"mode\":\"%s\",\n  \"workSeconds\":15,\n  \"declaredExceptionCode\":\"0xE0424354\"\n}\n",GetCurrentProcessId(),FileTicks(createTime),gFrequency.QuadPart,gFaultMode?"fault":"normal");
    fclose(f); return true;
}
static bool WriteResult(DWORD workExit) {
    wchar_t path[MAX_PATH]; OutputPath(path,MAX_PATH,L"result.json");
    FILE* f=nullptr; if (_wfopen_s(&f,path,L"wx") || !f) return false;
    fprintf(f,"{\n  \"kind\":\"isolated_pt_qualification_not_novovm\",\n  \"pid\":%lu,\n  \"workTid\":%lu,\n  \"qpcFrequency\":%lld,\n  \"workStartQpc\":%lld,\n  \"workEndQpc\":%lld,\n  \"workThreadExit\":%lu,\n  \"sink\":\"0x%016llx\",\n  \"batches\":[\n",GetCurrentProcessId(),gWorkTid,gFrequency.QuadPart,gWorkStart.QuadPart,gWorkEnd.QuadPart,workExit,gSink);
    for(unsigned i=0;i<kBatchCount;++i) {
        const Batch& b=gBatches[i];
        fprintf(f,"    {\"index\":%u,\"beginQpc\":%lld,\"endQpc\":%lld,\"kernel100ns\":%llu,\"user100ns\":%llu,\"cycles\":%llu,\"iterations\":%llu}%s\n",i,b.begin,b.end,b.kernel100ns,b.user100ns,b.cycles,b.iterations,i+1==kBatchCount?"":",");
    }
    fprintf(f,"  ],\n  \"faultMode\":%s,\n  \"exceptionCaptured\":%s,\n  \"exceptionCode\":\"0x%08lx\",\n  \"faultTid\":%lu,\n  \"faultQpc\":%lld,\n  \"rip\":\"0x%016llx\",\n  \"rsp\":\"0x%016llx\",\n  \"rbp\":\"0x%016llx\",\n  \"dumpSucceeded\":%s,\n  \"dumpError\":%lu\n}\n",gFaultMode?"true":"false",gFaultTid?"true":"false",gException.ExceptionCode,gFaultTid,gFaultQpc.QuadPart,gContext.Rip,gContext.Rsp,gContext.Rbp,gDumpOk?"true":"false",gDumpError);
    fclose(f); return true;
}
int wmain(int argc,wchar_t** argv) {
    if(argc!=3 || (wcscmp(argv[1],L"--normal") && wcscmp(argv[1],L"--fault"))) { fwprintf(stderr,L"Usage: PtQualification.exe --normal|--fault NEW_OUTPUT_DIRECTORY\n"); return 2; }
    gFaultMode=wcscmp(argv[1],L"--fault")==0;
    if(wcslen(argv[2])>MAX_PATH-40 || wcscpy_s(gOutput,argv[2])) return 3;
    if(!CreateDirectoryW(gOutput,nullptr)) { fwprintf(stderr,L"Output directory must be new: %lu\n",GetLastError()); return 4; }
    QueryPerformanceFrequency(&gFrequency);
    FILETIME create,exit,kernel,user; GetProcessTimes(GetCurrentProcess(),&create,&exit,&kernel,&user);
    if(!WriteIdentity(create)) return 5;
    gDumpRequested=CreateEventW(nullptr,TRUE,FALSE,nullptr); gDumpComplete=CreateEventW(nullptr,TRUE,FALSE,nullptr);
    if(!gDumpRequested || !gDumpComplete) return 6;
    HANDLE writer=nullptr;
    if(gFaultMode) { writer=CreateThread(nullptr,0,DumpWriter,nullptr,0,nullptr); if(!writer) return 7; }
    HANDLE worker=CreateThread(nullptr,0,Work,nullptr,0,nullptr); if(!worker) return 8;
    if(WaitForSingleObject(worker,35000)!=WAIT_OBJECT_0) ExitProcess(9);
    DWORD workExit=0; GetExitCodeThread(worker,&workExit); CloseHandle(worker);
    if(writer) { WaitForSingleObject(writer,1000); CloseHandle(writer); }
    const bool written=WriteResult(workExit); CloseHandle(gDumpRequested); CloseHandle(gDumpComplete);
    if(!written) return 10;
    if(gFaultMode && !gDumpOk) return 11;
    ExitProcess(workExit); // Parent must retain the real process handle/exit code.
}
