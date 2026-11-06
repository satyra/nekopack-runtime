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

Other copied files match the downloaded archive byte-for-byte. The separate NekoPack C ABI bridge is in `crates/nekopack-script/src/squirrel_bridge.cpp`.
The original BLESS executable identifies Squirrel 2.2.1; using 2.2.5 does not establish complete version equivalence. Current verification is limited to the documented sample path and synthetic tests.
