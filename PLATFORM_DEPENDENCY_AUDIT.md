# 平台依赖与复用审计

本文件保留实施前审计快照。方案通过后的 def/双向变量状态及四部曲复测结果见 [M1 执行链路](docs/Phase2_Title_Runtime.md)，不以新结果覆盖原审计证据。

日期：2026-11-06。范围：`e6c38e3` 已提交基线及当前未提交工作区。**本次审计未修改实现代码。** 检查 Cargo 清单、五个 crate、现有测试/研究报告，以及未提交的 C++/Squirrel/无窗口宿主；未将 `.local` 工具链或商业游戏资源作为项目源代码。

## 1. 结论与证据等级

未发现现有核心直接调用 Android API、JNI、Kotlin 或 Win32。仓库没有 AndroidManifest、Kotlin/Java/Gradle 客户端，也没有原生图形/音频宿主。因此“Android 耦合较少”是代码事实，但**不是 Android 已运行或跨平台已验证**。

平台独立性分三层记录：源代码不依赖平台 API；能为目标平台编译/链接；能在目标设备正确运行。当前大多数纯 Rust 代码只满足第一层和 Windows GNU 上的既有验证，不能自动升级为所有平台通过。

已提交部分可以大幅复用。近期最需要处理的是未提交原生构建脚本、FFI 生命周期和目录式研究输入，而非 DAT/MNG 算法。

## 2. 模块逐项审计

### A01 — DAT：可直接复用，低迁移成本

证据：`crates/nekopack-archive/src/lib.rs:122` 的 `Archive<R>`，`:133` 的 `open`，`:243` 的 `copy_entry`。输入是 `Read + Seek`，输出是 `Write`，偏移使用 u64；无路径、URI 或设备句柄。`std::io` 是跨平台 Rust 抽象，不是 Android/Windows 专属 API。

处理：保留解析/解密实现，由适配器提供可 seek 的源。非 seek SAF、权限撤销、资源变化与多线程读取由资源/宿主边界处理；不要让 archive crate 管理 Android 权限或全包临时复制。

### A02 — 资源目录：可直接复用，需扩充资源访问边界

证据：`crates/nekopack-resource/src/lib.rs` 的 Catalog、Candidate、resolve。只使用标准集合与 archive 类型；最高 priority 是显式策略，等优先级保留歧义。

处理：继续保留来源记录和原始名称字节。新增 GameSource/ResourceProvider 时，包枚举次序与物理路径只属于宿主输入。source ordinal 不是跨会话稳定身份，存档需绑定包指纹。跨平台文件名大小写差异不能改变名称哈希算法。

### A03 — 图像：可直接复用，不能等同完整 Renderer

证据：`crates/nekopack-image/src/lib.rs` 接受字节；`pixels.rs:44` 保存宽高及 RGBA；`canvas.rs:58` 根据传入尺寸分配，`:84` 做带坐标裁切的 source-over。

没有核心写死 1280×720，也没有 Android Bitmap/OpenGL/Metal 依赖。部分 Python 设置页对照工具固定样本尺寸，是研究配方限制，不是核心画布限制。PNG/MNG 元数据和 CPU 合成可复用；动画时序、文本布局和 GPU Renderer 仍缺实现。

### A04 — 纯脚本解析：可复用，但工作区新增了不必要的原生构建耦合

证据：`crates/nekopack-script/src/lib.rs` 保留字节、行号和 span；已提交版本无 C++ 依赖。当前未提交版本导出 `system`，并新增 `build.rs`，使只使用 parser 的调用方也需 C++ 工具链。

处理优先级：高。隔离系统脚本桥到独立 crate，或在短期内使用显式 feature 隔离；禁止借迁移重写既有 parser。保留原始字节到解码文本的位置映射，不能让 UTF-8 临时目录成为正式游戏输入格式。

### A05 — 未提交状态机：无 OS API，行为仍需验收

证据：`crates/nekopack-script/src/runtime.rs` 中 Variables、Machine、Scripts、Request、complete。调用栈、变量和等待请求保存在 Rust，外部效果需完成确认，这一方向可复用。

限制：它是启动/标题子集，不是完整 VM；分支、返回、重载、变量重声明及错误后恢复不能因“平台无关”就视为原版语义已证实。公开可变 names/slots、usize PC、通用字符串效果均不适合作为最终存档/宿主 ABI。后续移入 VM 边界时应以状态和重放测试约束，不同时改变语义。

### A06 — C++ 构建脚本：高风险，Android 编译前必须处理

证据：未提交 `crates/nekopack-script/build.rs` 调用 `CXX`/`AR`，固定 GNU/Clang 风格参数，产出 `.o`/`.a`，使用 `-w`，没有根据 Cargo TARGET 选择 Android NDK 编译器、API level、ABI 或 PIC 参数。

本机通过 `.local/toolchain/.../zig.exe` 配合 `NEKO_CXX_ARGS='c++ -D__MSVCRT_VERSION__=0x700'` 构建。这是会话环境，不是项目内可复现的跨平台构建配置。额外参数按空白拆分，复杂带空格参数有风险；MSVC 无法直接复用现有命令。Android `.so` 链接、macOS 工具链也尚无证据。

处理：按 target 显式选择工具链并检测 host/target 混编；以 Android ARM64 NDK 和当前桌面测试环境验证。库的 target 输出分离，避免误链接宿主静态库。构建警告不得长期整体关闭。仅修改构建桥，不改资源算法。

### A07 — FFI/ABI：高风险，需专项验收

证据：`src/system.rs:10` 的裸指针封装及线程标记，`src/squirrel_bridge.cpp` 的全局 new/delete、`__cxa_pure_virtual`、`abort`、Context 字符串结果；`vendor/squirrel2/include/squirrel.h:42` 起的 `_SQ64`/编译器类型分支。

已为 Windows LLP64 修改部分 typedef，但这不是所有目标 ABI 已验证。必须验证 SQInteger/SQHash/指针宽度、size_t 转换、malloc/free 配对、setjmp/longjmp 与所链接 CRT、一致编译宏、动态库符号可见性，以及全局 C++ 运算符覆盖对宿主其他库的影响。

结果目前通过 NUL 终止字符串返回并做 lossy UTF-8 转换，不能作为任意二进制结果协议；后续需要带长度的严格数据/错误传递。Rust 步数预算不约束一次 Squirrel eval 内部的无限循环；需执行预算、挂起或中断机制。OOM 直接 abort 不能伪装成可恢复脚本错误。线程亲和标记应保留，未证明前不得 unsafe Send。

### A08 — Squirrel 来源与许可证：发布前门槛

未提交目录为公开 Squirrel 2.2.5 源码；本地 COPYRIGHT 是带来源声明、修改标识和保留声明要求的许可，不能误标为本轮搜索到的现代 Squirrel MIT 版本。已改 typedef、模板声明顺序和头文件顺序，且新增桥接代码。

当前尚缺正式收录到项目第三方声明的版本/下载包哈希/补丁清单与构建说明。应在发布前补齐并审查所选依赖许可，不移除原声明。BLESS 原版二进制出现 2.2.1 版本字符串；2.2.5 的兼容性必须实测，不能按同一大版本自动认定。

### A09 — CLI 文件系统：宿主耦合，可保留在 CLI

证据：`crates/nekopack-cli/src/main.rs` 使用 `std::fs::File`；`title.rs:13` 的 Store 拼接 `script/`、`system/` 和 basename，`:29` 读 `assets.nut`。

这些是桌面目录适配，不是 Win32 API，本身可在多个 OS 使用，但不能接收 SAF URI，也不等价于正式 ResourceManager。当前标题研究输入先用 Python 将样本转为 UTF-8 目录。后续应让运行器通过 DAT 来源解析直接供给资源；CLI/研究导出保留作为工具，不将其路径规则搬进 VM。

Store 的脚本大小检查在读取后执行，assets.nut 没有同等输入预算。拒绝分隔符也不等于完整的目录隔离或 symlink 防护。正式导入接口应使用宿主签发的资源标识和读取预算，而非让脚本任意构造 OS 路径。

### A10 — 无窗口宿主：只作测试，不能作为生产后端

证据：`host/headless.nut:16` 文件接口始终报错，`:25` 使用虚拟时间，`:30` 总是返回确认事件，`:31` 只记录原生效果，Screen 记录图层计划，`:131` 用 InputChoice 选择预设项。`title.rs:41` 在首个菜单完成后的下一请求处结束测试。

它不包含真实输入、设备呈现、音频播放或持久化。缺失 spot 的容忍和符号 ID 仍是测试宿主合同，需要与原版命中/启用规则另行验证。不得让 Android 宿主复用“恒为确认”的事件实现，也不能把记录一帧认作显示了一帧。

### A11 — 状态桥与保存：尚未完成，不应各平台各补一份

CLI 每次请求前将 Rust 变量写入 `.nut` 的 var 表，未形成完整的双向状态归属和系统声明接口。BIRTH/BRAVE 在 `x_rooms.nut` 初始化所需 `def` 上失败，表明宿主注册仍不完整。应在共享系统桥中补齐，不能在 Android/Windows 分别实现差异补丁。

没有正式 save crate、稳定 snapshot schema、存档安全点或 C++ 回调续体重建。平台独立存档是新增能力，不是简单地替换文件路径。

### A12 — 研究与构建环境：保留证据，解除开发机依赖

`research` 脚本接受本地游戏与外部参考解码器目录；部分旧移植工程模块、PNG/Pillow、CP932 转换和源码参考位于仓库外。`.local/prepare_title.py` 与四个预处理样本树尚非已提交的可复现测试流程。Windows GNU、PowerShell、本机 Python/Zig 路径属于开发环境，不应成为核心运行条件。

保留所有历史 JSON 的 CLI/资源哈希。未来将合成 fixture 测试纳入公开 CI，商业四部曲仅在有授权的本地数据上跑，不上传素材。不要因缺少样本让 CI 静默报告兼容成功。

## 3. Android 专属依赖清单

当前实现中未发现 Kotlin/JNI/Android API 直接依赖；没有 Android 宿主，所以不存在需要“剥离”的既有 Activity/SAF 代码。现有架构文档里的 Kotlin、SAF、JNI 是计划，不是实现依赖。

未来 Android 专属部分必须限定在宿主：目录授权和持久 URI、描述符所有权、Activity/Surface 生命周期、音频焦点、IME、触摸输入、应用私有目录和打包。它们不能进入解析器、VM 或存档载荷。

## 4. 验证基线与本次复核

以下为已提交历史结果，未被本次审计重新生成或覆盖：

- `research/rust-archive-verification.json`：22 DAT、51,201 条目，完整资源为分层抽样。
- `research/metadata-verification.json`：84 MNG、27,712 块。
- `research/pixel-verification.json`：3,072 张嵌入 PNG、约 528 MB RGBA 差分。
- `research/canvas-verification.json`：252 组透明/黑色/彩色底合成。
- `research/config-scene-verification.json`：四份静态配置页配方，非四个原版窗口。
- `research/bootstrap-verification.json`：12 份启动/补丁脚本、91 条语句、53 次静态依赖。
- 提交基线验收为 41 项 Rust/CLI、8 项 Python 测试通过。它不能代替当前未提交改动的完整回归。

本次只读执行当前已有 release 二进制 `run-title .local/title/{0..3} 1`，逐个检查退出码：BLESS、BLACK 返回 0，产生菜单选择 1 和一份绘制计划，推进至 story1 下一请求；BIRTH、BRAVE 返回 1，均为 `module x_rooms.nut: the index 'def' does not exist`。因此当前执行链路是 **2/4 headless 场景通过、2/4 明确失败**，不是四部曲游戏可运行。

这些结果来自本地预处理输入与现有二进制，不是重新构建后的全量测试、Android 测试或原版行为一致验收。本次未重跑耗时的全量资源差分，也未重新执行未提交代码的全部 Rust 测试。Android/Linux/macOS/Steam Deck 均没有本仓库的设备运行证据。

## 5. 处理优先级

P0（Android 接入前）：原生 target 构建/ABI、纯 parser 与系统桥隔离、系统状态双向归属与执行预算、实际资源输入边界。

P1（Android MVP 内）：统一输入/时钟、Renderer/Audio 效果合同、生命周期、存档安全点和可复现 headless 测试。

P2（Android MVP 后）：Windows 图形宿主、Linux/Deck 与 macOS 设备适配及打包。平台构建烟测可提前做，完整桌面产品不提前并行启动。
