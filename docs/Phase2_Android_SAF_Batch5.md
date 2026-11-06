# 第五批：Android SAF 原始 DAT 标题链路

基线：ba61b8cf34f51ff6a4c10c014cad160ab78caa06。日期：2026-11-06。

## 结论

系统目录选择器授权 → ContentResolver 只读 PFD → JNI 借用 FD → Rust 复制为自有 File → 同一套 Archive/Catalog/脚本 VM，完成四部曲标题菜单到首段剧情调用。SAF 执行不读取解包或转码 fixture，没有将已知剧情终点替代执行结果。

设备是 API 28、x86_64、libnb.so 的 **ARM64 翻译执行**，不是 ARM64 真机。APK 只带 arm64-v8a native 库。仍是 headless 验证宿主，不是可玩游戏应用。

## 实现与边界

- 新增小型 nekopack-runtime crate，抽取已有标题驱动，让 CLI fixture、CLI DAT、Android DAT 共用执行循环，没有重写 VM。
- DatGame 接受 Read + Seek，核心不认识 Android URI、JNI 或平台路径。索引上限 16 MiB，单资源 64 MiB，回调结果 1 MiB。按需解密请求的资源，不整包解密到内存。
- Catalog 使用各包 seed 和显式最高 priority，最高优先级歧义直接失败。大小写沿用 DAT 哈希规则，不依赖外部文件名表。
- 脚本、数据表、INI、MNG 标签在 Rust 内解码。Assets/Data/Ini 通过同步回调按需读取，替代 assets.nut。MNG 仅取 FRAM/DEFI/IHDR 元数据供既有 headless Sprite 使用，不是动画播放器。
- 固定 encoding_rs 0.8.35 SHIFT_JIS 的 CP932-compatible profile，解码错误直接报告。不声称完整 Windows CP932 等价或汉化文字显示正确，不自动猜测编码。本次执行路径与 Python CP932 fixture 结果一致。
- host/dat.nut 保留原 fixture 的 34 个输入 ID。这是测试宿主策略，不是确认过的原引擎 ABI；菜单结果来自实际脚本。
- ResourceProvider 只在同步 eval 期间绑定，成功、Squirrel 错误和被捕获的 Rust panic 后解除绑定。返回字符串由 Rust binding 保留，C++ 在回调返回后复制，分配异常不穿越 Rust 回调。资源回调 panic 使 VM 永久停止，不能被 Squirrel catch 转成成功。
- Kotlin 保留原 PFD 到 JNI 返回并 finally 关闭；Rust 副本随资源实例销毁。一次 VM 生命周期处于工作线程的一次同步调用，不保留跨 JNI VM 指针。复制 FD 共享 seek offset，因此访问明确串行。
- 仅支持可 seek 的档案描述符；管道或不支持随机访问的 provider 返回 IO 错误，不隐式复制。云 provider、撤销授权、真 ARM64、16 KiB 页面未验收。
- 验证 APK 接受仅含目标 NEKOPACK 档案的目录，或含 0/1/2/3 子目录的回归集合。非 NEKOPACK 的同扩展名文件会被拒绝；复杂导入 UI 留待正式宿主。

SAF 接口参考 [Android 官方文档](https://developer.android.com/training/data-storage/shared/documents-files)。应用不申请全盘文件访问权限。

## 测试

原始 DAT 共六个，94,647,024 字节：BLESS system.dat；BIRTH system.dat + patch101.dat；BRAVE system.dat；BLACK system.dat + script.dat。前三款剧情在 system.dat。设备 SHA-256 与本地原包副本核对；未修改 DAT。未复制 image/sound/voice 大包，不声称验证媒体播放。

系统选择器授权后四款通过；强制停止应用、复用持久 URI 授权重启后再次通过。每次四个有效选择、四个无效选择。Android、桌面 DAT、既有 fixture 的有效执行日志逐字节一致，包含剧情位置、完成计数和变量槽。fixture 仅作桌面对照及旧入口回归。

同一 APK 的原入口通过 32 轮 FFI/预算失败销毁、238+74+97=409 个分配故障点、四部曲和无效选择，受管 VM live bytes 回到 0。这不是全进程 RSS 泄漏测试。初次误用更早的 title-verification.json 被校验器拒绝；改用上一批 title-android-preparation-verification.json 后通过，没有改历史报告或放宽断言。

桌面 workspace 测试通过；新增资源回调子协程、错误解除绑定、panic 终止，以及 DAT 优先级、歧义、缺失、解码错误测试。Clippy -D warnings、fmt、Python research 8 项通过。不是全剧情或完整 SAF provider 覆盖。

证据：research/android-saf-picker-verification.json、research/android-saf-restart-verification.json、research/android-saf-ffi-regression.json。

## 复现

1. 使用 build_probe.py，沿用第四批 SDK/NDK/JDK/Kotlin 配置。
2. 复制上述原始档案到设备 Documents 回归目录并安装 APK。
3. 启动 MainActivity，传入 `--ez saf true --es run_id UNIQUE_ID`，在系统选择器选择目录。
4. 运行 platforms/android/verify_dat.py，提供 --adb、--serial、--cli、--raw、--fixtures、--device-root、--run-id、--output。校验已安装 APK、档案哈希、run ID 和三路完整日志；该脚本不授予 URI 权限。
5. 新 run ID 加 `--ez reuse_grant true` 重启并校验，只能使用先前保存且已授权的 URI。
6. 旧入口使用 verify_probe.py，reference 为 title-android-preparation-verification.json。

## 许可证和下一步

encoding_rs 0.8.35 声明 `(Apache-2.0 OR MIT) AND BSD-3-Clause`。选择 MIT 并保留 WHATWG/BSD 条款，原文位于 crates/nekopack-runtime/licenses，APK 内 assets/licenses 同步保留。不引入私有运行时，不变更 Squirrel 版本或许可归属。

下一步沿 Android 主线接入最小呈现和输入：先消费背景、文本等待请求和点击推进，再扩展图层、音频、动画。当前首段调用边界不等于已执行剧情演出；真 ARM64 的 FFI/析构复核仍须独立完成。
