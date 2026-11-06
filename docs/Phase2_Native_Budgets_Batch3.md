# 第三批：变量不变量、FFI panic 与原生预算

日期：2026-11-06。基线：`e522864c40a64d9fff0c02ac402229684c7b2940`。

本批继续执行安全修复，不改动 Android 优先级、渲染、音频或平台宿主。原有审计和历史验证报告保持原样。

## 变量与回调（F06）

`Variables.names/slots` 改为私有；宿主通过受检声明、读写方法及只读 `slots()` 访问。常量不能覆盖别名，别名冲突不会创建额外槽位。get 使用受检查找，内部状态损坏也返回错误，不通过 map 索引 panic。CLI 使用只读访问器。

Rust 原生回调移除了 `unreachable!` 和诊断 CString 的 `expect`。错误写入固定 256 字节缓冲区，保留诊断、截断过长内容并保证 NUL 结尾。回调主体在 `catch_unwind` 内运行；即使 Squirrel 捕获了脚本错误，Rust 宿主仍报告回调 panic，并拒绝该 VM 后续执行。绑定按同步 eval 生命周期解绑。

构建要求 `panic=unwind`，`panic=abort` 会在编译时明确拒绝。测试仅在 cfg(test) 下注入真实 Rust panic，并实际经过 C++ → Rust 回调链。为避免 panic payload 的 Drop 再次 panic，该异常 payload 被遗忘；这是每个故障 VM 一次的故障处理代价，不属于原生 arena 计数。Rust 分配器 OOM、panic hook 自身 abort 不受 catch_unwind 保证。

增加 `SystemVm::try_new()` 返回初始化错误；CLI 改用这个接口。保留的 `new()` 是方便调用接口，初始化失败仍会在 Rust 层 panic，不适合作为 Android 宿主的可恢复初始化入口。

## 原生资源合同（F05 的编译、原生调用和堆配额部分）

默认每 VM 64 MiB 受管原生堆、每次 eval 4,000,000 原生工作单位；上一批的 1,000,000 字节码预算独立保留。可用 `set_native_limits(memory_bytes, work_units)` 调整，`memory_used()` 查看当前受管占用。

受管堆覆盖 `sq_malloc/sq_realloc` 路径、VM/子协程/编译器/正则对象和返回字符串副本，包括每块对齐后的记账头。realloc 采用先分配再复制，峰值按新旧两份同时存在计费。越限或分配失败不会返回供旧引擎解引用的 NULL，而是在原生内部抛出特定故障，并在最外层桥捕获。错误传播不穿过 Rust 栈。

VM 原生故障后不可恢复；局部析构不再遍历可能半更新的对象图，VM 销毁时直接释放 arena 中剩余块。正常关闭仍走 sq_close，然后回收残留受管块。线程局部 active arena 在每次原生进入/退出时保存恢复，多个 VM 分开记账；SystemVm 仍不可 Send/Sync。

工作量约束包含：

- 单个源文本上限 1 MiB，源长度准入、lexer 字符读取和分配量计费。
- 原生调用按字符串/数组实参长度准入；排序逐比较计费；substring find 按最坏比较次数准入。
- 正则递归匹配节点计费；编译器、正则、原生嵌套调用及 GC 标记使用 128 层深度限制。
- 预算/深度/分配错误不能被 Squirrel catch 吞掉后继续执行。

编译器和正则原有 longjmp 改为局部类型化 C++ 异常，避免跨过新 RAII 深度守卫。编译器错误文本同时改为实例内的有界缓冲区，消除静态共享缓冲区及长常量诊断写越界。所有原生异常在初始化、执行、关闭的 C ABI 边界处理。

## 验证

- Rust/CLI 全工作区：61 项通过；子进程入口 helper 标记 ignored，不重复计数。
- fmt、Clippy（`-D warnings`）、release 构建通过；Python research 测试 8 项通过。
- 三类场景逐分配点注入失败，直到完整成功：数组/表 238 点、正则 74 点、类/格式化 97 点，共 409 点。每次都检查错误、拒绝再执行和销毁后受管 live bytes 回到基线。这是确定性故障注入，不是假装操作系统真的耗尽了内存。
- 多 VM 隔离验证：一个 VM 越限并销毁不影响另一个的变量或堆计数。
- 子进程验证：大数组、持续扩容、子协程分配、源长度、编译工作量、正则工作量、深层表达式、GC 深层标记、排序/查找预算、长编译错误诊断、语法错误后的正常恢复。
- UBSan trap：24 项脚本/原生安全测试通过；四部曲 4 条标题路径及 4 个非法选择检查通过。未运行 ASan；受管计数不等于全进程泄漏检查。
- 普通 release 四部曲结果见 `research/title-native-budget-verification.json`；UBSan 结果见 `research/title-native-budget-ubsan-verification.json`。与第二批相比，fixture 哈希、最终变量状态哈希、剧情边界一致。

测试使用既有 RustGNU/Zig 0.14.1 环境；UBSan 参数仍为 `-fsanitize=undefined -fsanitize-trap=undefined`，独立输出 `target/ubsan`。没有提交游戏资源。

## Android 落地与尚未验证部分

为实现原生内部错误展开，本批启用 C++ exceptions 并显式链接 C++ runtime。Android 分支链接 NDK `c++_shared`；后续 ARM64 宿主需使用同一 NDK 编译这些源文件并打包匹配的 `libc++_shared.so`，通过 JNI 前先验证失败注入和 VM 销毁。不能用 Windows 测试替代这一验收。本机没有完成 Android NDK/设备验证。

arena 的终止回收不执行 release hooks，因此未来 Android 文件、音频、图形设备句柄必须由宿主拥有，不能交给 VM 的终止析构处理。当前注册的正则资源均在受管堆中，无这类设备句柄。

本批不是全进程内存沙箱：Rust Variables/资源图像、C++ 异常运行库、固定 Context 和操作系统开销不计入 VM 堆配额。工作单位是协作计数，不是时钟期限或外部取消；原生库内部、析构路径和恶意对象图仍需进一步压力审计。特别是正常引用计数释放的递归路径没有据本批测试宣称完整栈安全。审计 F05 的资源控制已有可执行约束，但不应将整个安全问题标为无条件关闭。

本轮仍只验证标题菜单到首个剧情子调用的 headless 路径；完整剧情、实际演出及 Android ARM64 应用尚未完成。
