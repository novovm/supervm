#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* No NOVOVM code, database, keys, service tasks or original dump is opened. */
typedef uintptr_t (__fastcall *Fn4)(uintptr_t,uintptr_t,uintptr_t,uintptr_t);
typedef struct Record {
    uint64_t entry_rbp,entry_rsp,return_address,pre_rbp,pre_rsp;
    uint64_t post_rbp,post_rsp,post_ip,state,target;
    uint64_t seq,parent,thread_id,thread_created,process_created,depth;
    uint64_t stack_low,stack_high,exception_code,exception_rip;
    uint64_t exception_rsp,exception_rbp,context_gap,reserved;
} Record;
typedef struct ThreadData {
    Record *records;
    size_t used,capacity;
    uint64_t seq,active,depth,tid,created,process_created,low,high;
    uint64_t rejected,mismatch,unwind,gap;
} ThreadData;
_Static_assert(offsetof(Record,state)==64,"assembly state offset");
_Static_assert(offsetof(Record,target)==72,"assembly target offset");
_Static_assert(sizeof(Record)==192,"binary record v1");
__declspec(thread) static ThreadData *tls;
extern uintptr_t CallCapture(Record*,Fn4,const uintptr_t*);
extern uintptr_t LegalUse(uintptr_t,uintptr_t,uintptr_t,uintptr_t);
extern uintptr_t BadUse(uintptr_t,uintptr_t,uintptr_t,uintptr_t);
extern unsigned char BadWrite, CaptureReturn;
#define CHECK(x) do {if(!(x)){fprintf(stderr,"check failed line %d: %s\n",__LINE__,#x);ExitProcess(9);}} while(0)
static uint64_t ft(FILETIME f){return ((uint64_t)f.dwHighDateTime<<32)|f.dwLowDateTime;}
static void init_thread(ThreadData *t,size_t cap){
    FILETIME c,e,k,u; ULONG_PTR low,high;
    memset(t,0,sizeof(*t));
    t->records=(Record*)calloc(cap,sizeof(Record));CHECK(t->records);
    t->capacity=cap;t->tid=GetCurrentThreadId();
    CHECK(GetThreadTimes(GetCurrentThread(),&c,&e,&k,&u));t->created=ft(c);
    CHECK(GetProcessTimes(GetCurrentProcess(),&c,&e,&k,&u));t->process_created=ft(c);
    GetCurrentThreadStackLimits(&low,&high);t->low=low;t->high=high;tls=t;
}
static int except_record(EXCEPTION_POINTERS *ep,Record *r){
    r->exception_code=ep->ExceptionRecord->ExceptionCode;
    r->exception_rip=ep->ContextRecord->Rip;r->exception_rsp=ep->ContextRecord->Rsp;
    r->exception_rbp=ep->ContextRecord->Rbp;
    /* A filter snapshot is not complete RtlRestoreContext/NtContinue history. */
    r->context_gap=1;tls->gap++;
    return ep->ExceptionRecord->ExceptionCode==0xe0424242 ? EXCEPTION_EXECUTE_HANDLER : EXCEPTION_CONTINUE_SEARCH;
}
__declspec(noinline) static uintptr_t capture(Fn4 fn,const uintptr_t a[4]){
    ThreadData *t=tls; Record *r; uintptr_t result=0;uint64_t parent=t->active;
    if(t->used==t->capacity){t->rejected++;return 0;} /* no silent overwrite or unobserved execution */
    r=&t->records[t->used++];
    r->seq=++t->seq;r->parent=parent;r->thread_id=t->tid;
    r->thread_created=t->created;r->process_created=t->process_created;
    r->depth=t->depth++;r->stack_low=t->low;r->stack_high=t->high;
    t->active=r->seq;
    __try {
        __try {result=CallCapture(r,fn,a);}
        __except(except_record(GetExceptionInformation(),r)){r->state=3;t->unwind++;}
    } __finally {t->active=parent;t->depth--;}
    if(r->state==2){
        CHECK(r->post_ip==(uintptr_t)&CaptureReturn);
        CHECK(r->entry_rsp>=t->low && r->entry_rsp<t->high);
        CHECK(r->pre_rsp==r->post_rsp);
        if(r->pre_rbp!=r->post_rbp)t->mismatch++;
    }
    return result;
}
static uintptr_t raise_test(uintptr_t a,uintptr_t b,uintptr_t c,uintptr_t d){
    (void)a;(void)b;(void)c;(void)d;RaiseException(0xe0424242,0,0,NULL);return 0;
}
static uintptr_t recursive(uintptr_t witness,uintptr_t depth,uintptr_t c,uintptr_t d){
    uintptr_t a[4]={witness,depth-1,0,0};(void)c;(void)d;
    if(depth)return capture(recursive,a)+1;
    return capture(LegalUse,a);
}
typedef struct Worker {ThreadData data;uint64_t witness[4];} Worker;
static DWORD WINAPI thread_worker(void *ctx){
    Worker *w=(Worker*)ctx;uintptr_t a[4]={(uintptr_t)w->witness,5,0,0};
    init_thread(&w->data,4096);
    for(int i=0;i<100;i++) CHECK(capture(recursive,a)==6);
    CHECK(w->data.used==700 && !w->data.mismatch && w->data.active==0 && w->data.depth==0);
    /* Parent sequence must refer to an earlier active-depth record, even when stack slots repeat. */
    for(size_t i=0;i<w->data.used;i++){
        Record *r=&w->data.records[i];CHECK(r->seq==i+1 && r->state==2);
        if(r->depth){CHECK(r->parent && r->parent<r->seq);CHECK(w->data.records[r->parent-1].depth+1==r->depth);}
        else CHECK(!r->parent);
    }
    return 0;
}
static uint64_t qpc(void){LARGE_INTEGER x;QueryPerformanceCounter(&x);return (uint64_t)x.QuadPart;}
static uint64_t cpu(void){FILETIME c,e,k,u;CHECK(GetProcessTimes(GetCurrentProcess(),&c,&e,&k,&u));return ft(k)+ft(u);}
typedef struct Batch {uint64_t ticks,cpu100ns,queries;int captured;} Batch;
static Fn4 realloc_fn;
static void heap_work(ThreadData *t,HANDLE heap,void **ptr,int captured,unsigned steps){
    for(unsigned j=0;j<steps;j++){
        uintptr_t a[4]={(uintptr_t)heap,0,(uintptr_t)*ptr,(j&1)?728:1000};
        uintptr_t p=captured?capture(realloc_fn,a):realloc_fn(a[0],a[1],a[2],a[3]);
        CHECK(p);*ptr=(void*)p;((volatile unsigned char*)*ptr)[0]=(unsigned char)j;
    }
    CHECK(!t->mismatch && !t->unwind && !t->rejected);
}
static void save_records(const char *path,ThreadData *t){
    FILE *f=NULL;CHECK(fopen_s(&f,path,"wb")==0);CHECK(fwrite(t->records,sizeof(Record),t->used,f)==t->used);CHECK(!fclose(f));
}
typedef struct ConcurrentWorker {
    ThreadData data;
    HANDLE heap,prepare,ready,go,done;
    volatile LONG mode,stop;
    uint64_t ticks;
} ConcurrentWorker;
static DWORD WINAPI concurrent_worker(void *ctx){
    ConcurrentWorker *w=(ConcurrentWorker*)ctx;init_thread(&w->data,4096);
    void *ptr=HeapAlloc(w->heap,0,728);CHECK(ptr);
    for(;;){
        CHECK(WaitForSingleObject(w->prepare,10000)==WAIT_OBJECT_0);
        if(w->stop)break;
        w->data.used=0;memset(w->data.records,0,w->data.capacity*sizeof(Record));
        CHECK(SetEvent(w->ready));CHECK(WaitForSingleObject(w->go,10000)==WAIT_OBJECT_0);
        uint64_t begin=qpc();heap_work(&w->data,w->heap,&ptr,(int)w->mode,4096);w->ticks=qpc()-begin;
        CHECK(SetEvent(w->done));
    }
    CHECK(HeapFree(w->heap,0,ptr));return 0;
}
static int concurrent_benchmark(void){
    realloc_fn=(Fn4)(uintptr_t)GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"RtlReAllocateHeap");CHECK(realloc_fn);
    HANDLE heap=HeapCreate(0,0,0);CHECK(heap);
    ConcurrentWorker w[4]={0};HANDLE threads[4],ready[4],done[4];
    for(int i=0;i<4;i++){
        w[i].heap=heap;w[i].prepare=CreateEventW(NULL,FALSE,FALSE,NULL);w[i].ready=ready[i]=CreateEventW(NULL,FALSE,FALSE,NULL);
        w[i].go=CreateEventW(NULL,FALSE,FALSE,NULL);w[i].done=done[i]=CreateEventW(NULL,FALSE,FALSE,NULL);
        CHECK(w[i].prepare&&w[i].ready&&w[i].go&&w[i].done);
        threads[i]=CreateThread(NULL,0,concurrent_worker,&w[i],0,NULL);CHECK(threads[i]);
    }
    FILE *f=NULL;CHECK(!fopen_s(&f,"concurrent.csv","w"));
    fprintf(f,"round,captured,wall_ticks,cpu100ns,frequency,operations,worker0_ticks,worker1_ticks,worker2_ticks,worker3_ticks\n");
    LARGE_INTEGER freq;
    QueryPerformanceFrequency(&freq);
    for(int n=-8;n<240;n++){
        int mode=n<0?((-n)&1):(((n/2)&1)?!(n&1):(n&1));
        for(int i=0;i<4;i++){w[i].mode=mode;CHECK(SetEvent(w[i].prepare));}
        CHECK(WaitForMultipleObjects(4,ready,TRUE,10000)==WAIT_OBJECT_0);
        uint64_t c0=cpu(),begin=qpc();
        for(int i=0;i<4;i++)CHECK(SetEvent(w[i].go));
        CHECK(WaitForMultipleObjects(4,done,TRUE,10000)==WAIT_OBJECT_0);
        uint64_t elapsed=qpc()-begin,cp=cpu()-c0;
        if(n==238){
            for(int i=0;i<4;i++){char path[64];sprintf_s(path,sizeof(path),"concurrent-thread-%d.bin",i);save_records(path,&w[i].data);}
        }
        if(n>=0)fprintf(f,"%d,%d,%llu,%llu,%lld,16384,%llu,%llu,%llu,%llu\n",n,mode,(unsigned long long)elapsed,(unsigned long long)cp,freq.QuadPart,(unsigned long long)w[0].ticks,(unsigned long long)w[1].ticks,(unsigned long long)w[2].ticks,(unsigned long long)w[3].ticks);
    }
    fclose(f);
    for(int i=0;i<4;i++){w[i].stop=1;CHECK(SetEvent(w[i].prepare));}
    CHECK(WaitForMultipleObjects(4,threads,TRUE,10000)==WAIT_OBJECT_0);
    for(int i=0;i<4;i++){
        DWORD code;CHECK(GetExitCodeThread(threads[i],&code)&&code==0);
        CHECK(!w[i].data.mismatch&&!w[i].data.unwind&&!w[i].data.rejected);
        CloseHandle(threads[i]);CloseHandle(w[i].prepare);CloseHandle(w[i].ready);CloseHandle(w[i].go);CloseHandle(w[i].done);free(w[i].data.records);
    }
    CHECK(HeapDestroy(heap));
    printf("CONCURRENT_COMPLETE threads=4 shared_private_heap=1 measured_operations=3932160 original_fault=not_reproduced\n");return 0;
}
static int single_benchmark(void){
    ThreadData t;FILE *f=NULL;
    init_thread(&t,8192);
    realloc_fn=(Fn4)(uintptr_t)GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"RtlReAllocateHeap");CHECK(realloc_fn);
    HANDLE heap=HeapCreate(0,0,0);CHECK(heap);void *ptr=HeapAlloc(heap,0,728);CHECK(ptr);
    Batch batches[120];LARGE_INTEGER freq;QueryPerformanceFrequency(&freq);
    /* Warm both paths. Reset preallocated storage OUTSIDE timing. No per-call clock or file writes. */
    for(int i=0;i<4;i++){t.used=0;memset(t.records,0,t.capacity*sizeof(Record));heap_work(&t,heap,&ptr,i&1,2048);}
    for(int i=0;i<120;i++){
        t.used=0;memset(t.records,0,t.capacity*sizeof(Record));
        int captured=((i/2)&1)?!(i&1):(i&1); /* paired AB / BA */
        uint64_t c0=cpu(),start=qpc();heap_work(&t,heap,&ptr,captured,4096);uint64_t end=qpc();
        batches[i]=(Batch){end-start,cpu()-c0,4096,captured};
        if(i==118){CHECK(captured);save_records("heap-last-batch.bin",&t);}
    }
    CHECK(HeapFree(heap,0,ptr));CHECK(HeapDestroy(heap));
    CHECK(!fopen_s(&f,"batches.csv","w"));fprintf(f,"batch,captured,ticks,cpu100ns,operations,frequency\n");
    for(int i=0;i<120;i++)fprintf(f,"%d,%d,%llu,%llu,%llu,%lld\n",i,batches[i].captured,(unsigned long long)batches[i].ticks,(unsigned long long)batches[i].cpu100ns,(unsigned long long)batches[i].queries,freq.QuadPart);
    fclose(f);
    printf("SINGLE_BENCHMARK_COMPLETE original_fault=not_reproduced heap_operations=491520\n");
    free(t.records);return 0;
}

int main(int argc,char **argv){
    CHECK(argc==2||argc==3);CHECK(SetCurrentDirectoryA(argv[1]));
    if(argc==3){if(!strcmp(argv[2],"--single"))return single_benchmark();CHECK(!strcmp(argv[2],"--concurrent"));return concurrent_benchmark();}
    ThreadData t;uint64_t good[4]={0},bad[4]={0};uintptr_t a[4]={(uintptr_t)good,0,0,0};
    init_thread(&t,8192);
    CHECK(capture(LegalUse,a)==1);CHECK(good[0]==good[2] && good[0]!=good[1]);CHECK(!t.mismatch);
    a[0]=(uintptr_t)bad;CHECK(capture(BadUse,a)==1);
    CHECK(t.mismatch==1 && bad[0]==t.records[1].pre_rbp && bad[1]==t.records[1].post_rbp);
    CHECK((bad[0]^bad[1])==0x4000 && bad[3]==(uintptr_t)&BadWrite);
    CHECK(capture(raise_test,a)==0);CHECK(t.records[2].state==3 && t.unwind==1 && t.gap==1);
    a[0]=(uintptr_t)good;CHECK(capture(LegalUse,a)==1);CHECK(!t.active&&!t.depth);
    save_records("fixture-records.bin",&t);
    FILE *f=NULL;CHECK(!fopen_s(&f,"fixture.json","w"));
    fprintf(f,"{\"schema\":1,\"record_bytes\":192,\"legal_preserved\":true,\"controlled_violation_count\":1,\"unwind_count\":1,\"context_gap_count\":1,\"bad_write_rva\":%llu,\"bad_before\":%llu,\"bad_after\":%llu,\"original_fault_reproduced\":false}\n",
      (unsigned long long)((uintptr_t)&BadWrite-(uintptr_t)GetModuleHandleW(NULL)),(unsigned long long)bad[0],(unsigned long long)bad[1]);fclose(f);
    /* Capacity exhaustion refuses an unrecorded call, without overwriting evidence. */
    t.used=t.capacity;uint64_t oldseq=t.seq;CHECK(capture(LegalUse,a)==0 && t.rejected==1 && t.seq==oldseq);
    free(t.records);
    Worker w[4]={0};HANDLE handles[4];
    for(int i=0;i<4;i++){handles[i]=CreateThread(NULL,0,thread_worker,&w[i],0,NULL);CHECK(handles[i]);}
    CHECK(WaitForMultipleObjects(4,handles,TRUE,10000)==WAIT_OBJECT_0);
    for(int i=0;i<4;i++){char path[64];DWORD code;CHECK(GetExitCodeThread(handles[i],&code)&&code==0);CloseHandle(handles[i]);sprintf_s(path,sizeof(path),"thread-%d.bin",i);save_records(path,&w[i].data);free(w[i].data.records);}
    printf("LIFECYCLE_QUALIFICATION_COMPLETE original_fault=not_reproduced\n");return 0;
}
