# 第一批原生执行安全修复

日期：2026-11-06。基线：`7d4d624c447db0415080c62a2ced8b3529bdd64f`。

本批处理技术审计 F01、F02、F03，保留原始审计报告及历史验证文件。项目仍为部分脚本运行时/headless 原型，不是完整游戏运行时或 Android 应用。

## 变更与证据

### F01：协程变量桥

`squirrel_bridge.cpp` 将 Context 存在同一 Squirrel 实例共享的 registry 中，子 VM 和根 VM 均从这里取得上下文。registry 的私有 userpointer 键不通过脚本 API 暴露。Context 在 `sq_close` 返回之后才释放；Rust Variables 仍仅在同步 eval 调用期间绑定，未将借用延长到协程生命周期。

变量代理 `_get`/`_set` 显式通过 `::NekoVariableGet`/`::NekoVariableSet` 访问根表，避免未限定的名字查找再次进入代理 `_get`。修复上下文后，新增回归曾暴露这一递归：变量读取重复触发，最终报 alias 不存在；显式根表查找消除了该路径。

新增测试在独立子进程中执行，基线协程场景以 Windows 访问违例退出（3221225477）。修复后测试覆盖 16 次创建/销毁、子 VM 注册别名、挂起后由 Rust 改变量、恢复后双向同步、未绑定调用报错及脚本异常后的再次执行。

本批支持的是根 eval 内驱动的子协程；不表示根 VM 的 suspend/resume 协议已完成。F04 仍待修复。

### F02：format 边界与整数宽度

格式扫描遇到 NUL 停止；20 字节格式缓冲区的容量检查包含 `%`、整数长度修饰符、转换字符和 NUL。64 位 SQInteger 使用对应的 `ll` 格式，区分有符号、无符号、字符参数，输出采用带容量的 snprintf。

基线测试实测 `%--` 被接受，64 位 `4294967297` 格式化不正确。修复后覆盖残缺格式、过长 flags/宽度/精度、非法转换、正常字符串/浮点/字符格式、64 位十进制及十六进制输出，以及错误后的正常格式化。边界越界的原始判断来自审计代码证据；不宣称每个非法格式都在基线实测造成崩溃。

兼容性变化：整数格式参数要求 OT_INTEGER，不再隐式接受浮点转整数；超容量输出返回错误。当前验证为窄字符构建，未验证 SQUNICODE。

### F03：整数运算

加减乘、最小整数除以 -1、最小整数取负在执行有符号运算之前检查溢出并报错；最小整数对 -1 取余为零。比较使用关系运算而非相减。移位检查位数，使用无符号运算和显式符号扩展，避免负数左移、越界移位及别名指针转换。

基线测试实测极值比较错误。修复后验证极值、溢出错误、移位与错误恢复。当前采用明确报错的数值策略，原版 2.2.1 的溢出语义尚未确认；不能宣称 2.2.5 与原版等价。本批也不表示 vendor 全部数值转换或内存路径已无 UB。

## 验证结果

- `cargo test --workspace`：50 项通过（原有 46 项 + 4 项原生安全测试）。`native_probe` 是被父测试启动的子进程入口，在直接测试枚举中标为 ignored，不另计一项通过；每个父测试检查子进程退出状态，15 秒超时会杀进程并失败。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、release 构建通过。
- `python -m unittest discover -s research -p 'test_*.py'`：8 项通过。
- `verify_title.py`：BLESS/BIRTH/BRAVE/BLACK 4 条路径通过，4 个非法菜单选择拒绝。结果及 CLI、fixture、状态哈希见 `research/title-native-safety-verification.json`。
- 原生代码使用 Zig 0.14.1，`NEKO_CXX_ARGS='c++ -D__MSVCRT_VERSION__=0x700 -fsanitize=undefined -fsanitize-trap=undefined'`，独立 `CARGO_TARGET_DIR=target/ubsan`，执行 `cargo test -p nekopack-script`：9 项原有脚本测试及 4 项新增测试通过，未触发 trap。该检查不是 ASan、泄漏检测，也不是所有路径的内存安全证明。

普通构建使用本机 RustGNU 工具链，CXX/AR 指向 `.local/toolchain/zig-x86_64-windows-0.14.1/zig.exe`，`NEKO_CXX_ARGS='c++ -D__MSVCRT_VERSION__=0x700'`、`NEKO_AR_ARGS='ar'`。

四部曲复现命令：

```powershell
D:/app/python/python.exe research/verify_title.py --fixtures .local/title-verified --cli target/release/nekopack-cli.exe --output research/title-native-safety-verification.json
```

本次验证覆盖菜单选项 1 到首个剧情子调用请求，未执行完整剧情、真实渲染、音频、输入或 Android ARM64。本地游戏 fixture 不纳入提交。

## 后续顺序

1. 修复 F04 根 VM 挂起被误认为执行完成，明确拒绝或恢复协议，并验证错误后不会确认 pending request。
2. 修复 V01 嵌套 map 分支边界，独立测试控制流及调用栈。
3. 处理 F05 原生执行预算/取消/分配失败与 F06 变量状态不变量，再推进 Android ARM64 最小宿主。

本批测试的进程超时只保护测试进程，不是运行器执行预算。JNI、平台宿主及渲染保持未改动。
