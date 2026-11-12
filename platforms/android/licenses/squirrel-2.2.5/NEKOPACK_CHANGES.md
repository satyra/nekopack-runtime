# Squirrel 2.2.5 source subset

Upstream: Alberto Demichelis, Squirrel 2.2.5 stable.
Archive: https://downloads.sourceforge.net/project/squirrel/squirrel2/squirrel%202.2.5%20stable/squirrel_2.2.5_stable.tar.gz

Downloaded archive SHA-256: `8667e181ac2e0428d60d74a4fae6b5c15b8ed0f0933253cc7cf1aaa847d0ee25`.
The complete upstream license is retained in [COPYRIGHT](COPYRIGHT) and `include/squirrel.h`.
This is an altered source distribution, not an unmodified upstream release.

Included: include headers, squirrel core, sqstdstring/sqstdrex, COPYRIGHT and HISTORY.
Excluded: interpreter executable, examples, and unused standard-library implementations.

Changes against the extracted SQUIRREL2 archive:

- `include/squirrel.h`: non-MSVC 64-bit integer/hash/raw-value types use long long instead of long, which is 32 bits under Windows LLP64.
- `squirrel/squtils.h`: allocator declarations precede templates for modern C++ name lookup.
- `squirrel/sqvm.cpp`: math.h precedes the internal type macro to avoid system-header expansion conflicts.
- `squirrel/sqvm.cpp`: integer arithmetic reports overflow, comparison avoids subtraction, and shifts use checked counts and explicit unsigned bit operations. Minimum-integer remainder by -1 returns zero. This is an explicit NekoPack numeric policy, not verified original-engine overflow compatibility.
- `sqstdlib/sqstdstring.cpp`: format scanning stops at NUL, the format buffer includes modifiers and terminator in its capacity check, integer varargs match SQInteger width, and output uses bounded snprintf. Integer conversions require integer arguments instead of implicitly converting floats.
- `squirrel/sqvm.cpp`: root VM suspension is rejected before state mutation; coroutine suspension remains available. Each bytecode consumes the shared execution budget; exhausted execution skips script error hooks.
- `include/squirrel.h`, `squirrel/sqapi.cpp`, `squirrel/sqstate.h`, `squirrel/sqstate.cpp`: NekoPack-only `sq_setexecutionbudget`/`sq_executionexhausted` API and shared-state counters. These are local extensions, not upstream Squirrel ABI. The bridge treats budget exhaustion as terminal; the limit does not meter compilation, native-library loops, or heap allocation.
- `include/neko_limits.h`, `squirrel/sqmem.cpp`: per-VM tracked allocations, native work/depth limits, deterministic allocation-failure injection and tracked-live-byte diagnostics. Quota/allocation failures throw only within C++, are caught by the bridge, and terminate that VM. Arena metadata and temporary realloc copies count toward the heap quota.
- `squirrel/sqobject.h`, `squtils.h`, `sqlexer.cpp`, `sqfuncstate.cpp`, `sqpcheader.h`: on terminal native failure, local unwinding avoids traversing possibly partial VM objects; the owning arena releases their blocks. Vector byte-size multiplication is checked.
- `squirrel/sqcompiler.cpp`, `sqstdlib/sqstdrex.cpp`: syntax errors use typed C++ exceptions instead of longjmp, preserving depth-guard destructors. Parser/regex recursion is bounded; lexer reads and regex match visits consume work. Empty regex has nonzero initial node capacity. Compiler diagnostics use a bounded instance buffer instead of an unbounded static buffer.
- `squirrel/sqvm.cpp`, `sqbaselib.cpp`, `sqobject.cpp`: native-call argument admission, sort comparisons, worst-case substring-search admission, and GC mark recursion consume native work. These cooperative checks are not a wall-clock deadline or a complete hostile-input sandbox.

The host now compiles with C++ exceptions and links a C++ runtime (Android: NDK `c++_shared`, Apple: `c++`, GNU targets: `stdc++`). No C++ exception is allowed to cross the Rust ABI. Android hosts must package the matching NDK runtime; Android ARM64 execution is not yet verified. Terminal arena disposal deliberately does not invoke release hooks: this VM must not own external device/file handles through such hooks.

Other copied files match the downloaded archive byte-for-byte. The separate NekoPack C ABI bridge is in `crates/nekopack-script/src/squirrel_bridge.cpp`.
The original BLESS executable identifies Squirrel 2.2.1; using 2.2.5 does not establish complete version equivalence. Current verification is limited to the documented sample path and synthetic tests.
