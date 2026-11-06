/* see copyright notice in squirrel.h */
// NekoPack: per-VM tracked heap, terminal failure before returning a null pointer.
#include "sqpcheader.h"
#include <cstddef>
#include <cstring>
struct alignas(std::max_align_t) NekoBlock {
    NekoBlock *previous, *next;
    NekoArena* owner;
    size_t size;
};
thread_local NekoArena* neko_arena = nullptr;
thread_local size_t neko_live_bytes = 0;
bool neko_failed() { return neko_arena && neko_arena->failed; }
void neko_fail(NekoLimitError error) {
    if(neko_arena) neko_arena->failed=true;
    throw error;
}
void neko_work(size_t amount) {
    if(!neko_arena || !neko_arena->metered) return;
    if(amount>neko_arena->work) neko_fail(NekoWorkLimit);
    neko_arena->work-=amount;
}
NekoArena::~NekoArena() {
    while(head) { NekoBlock* next=head->next; neko_live_bytes-=head->size; free(head); head=next; }
}
void *sq_vm_malloc(SQUnsignedInteger size) {
    NekoArena* a=neko_arena;
    if(a && a->allocations_left!=(size_t)-1) {
        if(a->allocations_left==0) neko_fail(NekoAllocationFailure);
        --a->allocations_left;
    }
    if(size>((size_t)-1)-sizeof(NekoBlock)) neko_fail(NekoMemoryLimit);
    size_t total=(size_t)size+sizeof(NekoBlock);
    if(a && (a->used>a->limit || total>a->limit-a->used)) neko_fail(NekoMemoryLimit);
    neko_work(1+size/64);
    NekoBlock* block=(NekoBlock*)malloc(total);
    if(!block) neko_fail(NekoAllocationFailure);
    neko_live_bytes+=total;
    block->owner=a; block->size=total; block->previous=nullptr;
    block->next=a ? a->head : nullptr;
    if(a) {
        if(a->head) a->head->previous=block;
        a->head=block; a->used+=total;
        if(a->used>a->peak) a->peak=a->used;
    }
    return block+1;
}
void sq_vm_free(void *p, SQUnsignedInteger) {
    if(!p) return;
    NekoBlock* block=((NekoBlock*)p)-1;
    NekoArena* a=block->owner;
    if(a) {
        if(block->previous) block->previous->next=block->next;
        else a->head=block->next;
        if(block->next) block->next->previous=block->previous;
        a->used-=block->size;
    }
    neko_live_bytes-=block->size;
    free(block);
}
void *sq_vm_realloc(void *p, SQUnsignedInteger, SQUnsignedInteger size) {
    // Peak includes both buffers during copying; old allocation survives failure.
    void* next=sq_vm_malloc(size);
    if(p) {
        size_t previous=(((NekoBlock*)p)-1)->size-sizeof(NekoBlock);
        memcpy(next,p,previous<(size_t)size ? previous : (size_t)size);
        sq_vm_free(p,0);
    }
    return next;
}
