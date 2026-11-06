# 第四批：Android ARM64 交叉编译与最小宿主

日期：2026-11-06。基线：`a0429dfb1792b87857f1f4e2fd67d0d8a6e9d344`。

## 结论及验证等级

已完成 NDK ARM64 交叉编译、APK 安装、Kotlin/JNI 实际调用及四部曲 headless 回归。当前可用设备的 kernel machine 为 x86_64，Android API 28，通过 `libnb.so` 翻译执行 ARM64；用户确认先使用该环境。**这是 ARM64 翻译执行验证，不是 ARM64 真机验收。**

安装包 primaryCpuAbi 为 arm64-v8a，Rust 宿主自报 `aarch64`、64 位。Java 系统接口测得页面大小 4096。三份 native 库均为 AArch64，所有 ELF LOAD 段对齐至少 0x4000，但没有在 16 KiB 页面设备上运行。

## 实现范围

- `platforms/android`：Kotlin Activity、JNI C++ 桥、Rust 验证 cdylib、离线构建和 ADB 验证脚本。测试应用只显示报告，没有新增游戏演出系统。
- Rust/C++ 核心增加显式 PIC 编译；Android 分支继续链接与构建所用 NDK 相同的 libc++_shared。NDK r27d（27.3.13750724），目标 aarch64-linux-android，native min API 23。
- JNI 使用 UTF-8 byte arrays，避免把普通 UTF-8 错当 JNI modified UTF-8。输入只借用到同步调用结束，输出由 Rust 分配并由对应 Rust free 入口按精确长度释放。
- VM 完整生命周期位于后台线程的一次调用内；JNI 不持有 VM、Variables、回调或跨线程句柄。外层 Rust panic 被捕获并形成 FAIL 报告。
- 原有 native live-byte/failure-injection 接口通过默认关闭的 Rust `validation` feature 提供给测试宿主。新增 crate 不属于普通 workspace 默认构建成员。
- CLI headless 适配器改为支持输出到 Write；Android probe 复用同一份源码和现有 `.nut` headless host，不维护另一套 VM 或游戏脚本实现。

16 KiB ELF 链接参数参考 [Android 官方页面大小说明](https://developer.android.com/guide/practices/page-sizes)，使用 `-Wl,-z,max-page-size=16384`。文档建议不是运行证据；本批只对产物做静态段对齐检查。

## 实际暴露并修复的问题

第一次宿主运行已越过 FFI/故障注入阶段，但 BLESS 请求 `P01` 时无法打开解包 fixture 的 `p01.txt`。Windows 文件系统隐含的不区分大小写掩盖了这一耦合。

修复限定在解包 UTF-8 fixture 适配器：各平台均按 ASCII 不区分大小写匹配 basename，出现多个匹配明确拒绝。没有改名游戏 fixture、没有修改 DAT 哈希规则、没有把已知剧情结果硬编码成执行结果。补充对应单元测试；大小写歧义测试分支仅在 Unix 文件系统测试构建中执行，本次 Android 正向链路实际覆盖 P01/p01 查找。

验证工具另发现旧 adbd 的 exec-out 可以在 cat 报文件不存在时仍返回 0。轮询改为检查唯一 run ID 和报告内容，宿主采用临时文件原子改名发布结果；不会把退出码 0 当成执行通过。页面大小通过应用内 Os.sysconf 获取，不依赖该设备缺失的 shell getconf 命令。

## 测试结果

Android 翻译执行环境：

- 32 轮变量注册、子协程写入、挂起、Rust 修改、恢复及未绑定访问检查。
- 每轮实际注入 Rust 回调 panic，验证 Squirrel catch 后仍向宿主报告错误，并阻止故障 VM 再执行。
- 每轮内存、原生工作量、字节码和递归深度失败后销毁 VM，检查受管 live bytes 回到 0。
- 数组/表、正则、类/格式化逐分配点故障注入分别为 238、74、97 点，共 409 点；每类都运行至无故障完整成功，并检查返回值及销毁记账。没有通过耗尽操作系统内存模拟失败。
- BLESS/BIRTH/BRAVE/BLACK 4 条标题菜单到首个剧情子调用路径通过；4 个无效菜单选择正确拒绝。
- 每个 fixture 文件从应用私有目录回读，与本地副本逐字节比较。四部曲最终变量哈希、剧情边界及完成计数与桌面参考一致。

桌面回归：原有 61 项 Rust/CLI 测试和新增 fixture 查找测试通过；后者在 CLI 和 probe 两个 test target 中各执行一次，因此 workspace 输出为 63 次测试执行，不解释为 63 项独立覆盖。Clippy（-D warnings）、fmt、release CLI 构建通过，Python research 8 项通过，桌面四部曲 4 条路径及 4 个无效选择检查通过。

记录：`research/android-arm64-probe-verification.json`、`research/title-android-preparation-verification.json`。Android JSON 包含 NDK 版本、APK/三个库的 SHA-256、ELF headers/segments/DT_NEEDED、设备 ABI、原始测试摘要和四部曲比较结果。仓库不保存 APK、签名或游戏资源。

## 仍未验收

- ARM64 真机、API 23 实际运行、16 KiB 页面设备；本次实际运行是 API 28/4 KiB/libnb。
- 原生 ASan/HWASan、全进程内存/RSS 泄漏检查。live-byte 断言只覆盖 VM 受管原生堆；JNI 报告、Kotlin/Rust 分配和异常 payload 不在该计数内。
- 完整 Android 游戏宿主、SAF、输入生命周期、后台恢复、存档、图像/音频演出以及完整剧情。测试仍使用已解包、已解码的本地 fixture，不能等同 Android 直接导入 DAT 并运行游戏。
- 上一批已记录的协作预算与对象图析构边界仍存在，不因成功交叉编译或翻译执行而关闭。

下一步应优先用相同测试 APK 在 ARM64 真机复核 C++ 异常与 VM 销毁，再接入最小 Android 资源访问适配；不扩展其他平台宿主。
