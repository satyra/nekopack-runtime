// NekoPack host-only limits. No exception may leave the C ABI bridge.
#ifndef NEKO_LIMITS_H
#define NEKO_LIMITS_H
#include <stddef.h>
struct NekoBlock;
enum NekoLimitError { NekoMemoryLimit, NekoWorkLimit, NekoDepthLimit, NekoAllocationFailure };
struct NekoArena {
    NekoBlock* head = nullptr;
    size_t used = 0, peak = 0, limit = 64 * 1024 * 1024;
    size_t work = 0, depth = 0;
    size_t allocations_left = (size_t)-1; // Native fault-injection entry point only.
    bool metered = false, failed = false;
    ~NekoArena();
};
extern thread_local NekoArena* neko_arena;
extern thread_local size_t neko_live_bytes;
struct NekoScope {
    NekoArena* previous;
    explicit NekoScope(NekoArena* arena) : previous(neko_arena) { neko_arena=arena; }
    ~NekoScope() { neko_arena=previous; }
};
bool neko_failed();
void neko_work(size_t amount=1);
void neko_fail(NekoLimitError error);
struct NekoDepth {
    NekoDepth() { neko_work(); if(neko_arena && ++neko_arena->depth>128) neko_fail(NekoDepthLimit); }
    ~NekoDepth() { if(neko_arena) --neko_arena->depth; }
};
#endif
