# NekoPack Cross-Platform Runtime

目标：共享 Rust 核心与独立平台宿主。Android ARM64（Kotlin + JNI）仍是首个交付目标，Android MVP 后验证 Windows，再推进 Linux/Steam Deck、macOS。跨平台方案已获审核。当前已有 Android 横屏验证壳及 APK，通过 SAF 读取原始 DAT 和 MPEG 视频；尚未形成完整游戏应用。

当前四部曲支持标题到有界剧情执行，`def` 和 `.nut` 变量访问共用 Rust 状态。Android 已验证图像、音频、部分演出与 movie 播放/跳过/恢复；BLESS/BLACK 在电影返回后仍停于未实现的 `.route`。这不代表完整游戏兼容，当前范围与真机/翻译执行结果见 [movie 增量](docs/Phase2_Movie_Batch14.md)，早期标题链路见 [执行链路验收](docs/Phase2_Title_Runtime.md)。目标架构、依赖审计、最小迁移与路线分别见根目录 [架构](CROSS_PLATFORM_ARCHITECTURE.md)、[审计](PLATFORM_DEPENDENCY_AUDIT.md)、[迁移](MIGRATION_PLAN.md)、[路线](DEVELOPMENT_ROADMAP.md)。

2026-11-06：完成公开源码调查后，新增无外部 crate 依赖的 `nekopack-archive` 和 `nekopack-cli`。Rust 资源层通过四部曲 22 个 DAT、51,201 个条目的差分验证；索引、全部条目元数据和前 16 字节与旧 Python 参考解码器一致。完整资源仅分层抽样，不代表所有图像/音频都已解码或游戏可运行。结果见 [Phase 1 验收](docs/Phase1_Archive_Increment.md)。

后续增量新增名称字节哈希、`nekopack-resource` 来源/覆盖分析和 `nekopack-image` MNG 元数据解析。四部曲全部 84 个 MNG、27,712 个块核对通过；第三部 32 对补丁资源均确认内容变化，第一部一组同优先级候选明确报告歧义。使用与验收见 [名称与 MNG 增量](docs/Phase1_Metadata_Increment.md)。

像素增量加入 PNG/RGBA 解码与单图导出：四部曲 MNG 内全部 3,072 张图像、约 528 MB RGBA 与 Pillow 一致，84 次 PNG 导出复核通过。图像模块使用 `png` crate。见 [像素解码验收](docs/Phase1_Pixel_Increment.md)。最新增量支持素材坐标与指定图层画布合成，尚未实现动画播放或通过原版显示一致验收，见 [画布增量](docs/Phase1_Canvas_Increment.md)。

## 使用

静态入口 `parse-bootstrap FILE ENTRY` 保留原有字节与来源信息；历史 12 份启动/补丁脚本、91 条语句、53 次直接依赖验证见 [启动脚本增量](docs/Phase2_Bootstrap_Parser.md)。执行入口 `run-title DECODED_FIXTURE_ROOT CHOICE` 使用私有预处理样本树；构建新增 C++ 工具链要求，见下方链接。

```text
cargo run -p nekopack-cli -- inspect-archive path/to/system.dat
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --workspace
```

`inspect-archive` 输出 TSV：第一行是 name seed、priority、索引长度、条目数和尾随字节数；后续行列出条目序号、目录/文件哈希、块头偏移、长度、key 和解密前缀。`read-entry FILE ORDINAL` 与 `read-index FILE` 向 stdout 输出原始二进制，供程序调用；错误走 stderr 并返回非零状态。二进制输出不要经过会转码的文本管道。

库使用 `Archive::open(Read + Seek, max_index_bytes)`；资源通过 `copy_entry(ordinal, Write)` 用 64 KiB 缓冲流式读取。CLI 的索引预算是 16 MiB，不是 DAT 格式的固有限制。打开后源文件必须保持不变。重复哈希保留原始序号，不做隐式覆盖。此版本显式面向 /1 布局，不实现 /2、/3 自动探测、未知名称自动还原或原版动态挂载模拟。

新增命令：`hash-name SEED_HEX NAME_BYTES_HEX`、`lookup-name DIR_BYTES_HEX NAME_BYTES_HEX FILE...`、`inspect-overlays FILE...`、`inspect-mng FILE`、`inspect-mng-archive FILE`。名称由调用方按实际编码提供 hex 字节，包的 source ID 对应参数列表顺序；最高 priority 并列时返回歧义。MNG inspect 命令只检查元数据，未知块不会被当成已经支持的动画命令。

像素命令：`decode-png FILE`、`decode-mng-archive FILE` 输出二进制 RGBA 记录流；`export-mng-image FILE ENTRY IMAGE` 输出单图 PNG。详细协议及支持范围见像素验收文档。批量输出必须检查进程退出码；导出图不含原始 MNG 引擎元数据。

画布命令：`inspect-sprites FILE ENTRY` 输出名称与坐标；`compose-mng FILE ENTRY:IMAGE [ENTRY:IMAGE ...]` 按给定顺序合成透明 PNG；`compose-mng-on FILE RRGGBB ENTRY:IMAGE [ENTRY:IMAGE ...]` 从指定不透明底色开始合成。它不自动播放 MNG，也不自动选择界面状态。

本机 Rust 工具链位于用户目录，必要时在当前 PowerShell 会话设置：

```powershell
$env:RUSTUP_HOME="$env:LOCALAPPDATA/RustGNU/rustup"
$env:CARGO_HOME="$env:LOCALAPPDATA/RustGNU/cargo"
$env:Path="$env:CARGO_HOME/bin;$env:Path"
```

其他已安装 Rust 的环境使用自己的工具链即可。当前系统解释器桥还需要 C++ 编译器/归档器；已验证 Windows GNU + Zig 0.14.1，参数和复现步骤见 [构建与验证](docs/Phase2_Title_Runtime.md)。Android ARM64 使用 NDK r27d 交叉编译；PGKM10 真机与 x86_64/libnb 翻译执行模拟器的结果分别记录，见上述 movie 增量。

## 调查文档

- [技术研究](docs/NekoPack_Technical_Research.md)
- [封包规格](docs/NekoPack_Archive_Specification.md)
- [脚本分析](docs/NekoPack_Script_Analysis.md)
- [运行时架构](docs/NekoPack_Runtime_Architecture.md)
- [兼容性矩阵](docs/NekoPack_Compatibility_Matrix.md)
- [开发路线](docs/NekoPack_Development_Roadmap.md)
- [证据与版本](docs/Sources.md)
- [本地样本元数据](research/sample-audit.json)
- [Phase 1 实施与验收](docs/Phase1_Archive_Increment.md)
- [名称、覆盖关系与 MNG 验收](docs/Phase1_Metadata_Increment.md)
- [PNG/RGBA 像素解码验收](docs/Phase1_Pixel_Increment.md)
- [素材坐标与画布合成验收](docs/Phase1_Canvas_Increment.md)
- [原版显示差异定位与修正](docs/Phase1_Display_Correction.md)
- [启动脚本 AST 与依赖验证](docs/Phase2_Bootstrap_Parser.md)
- [第三方算法来源声明](THIRD_PARTY_NOTICES.md)

## 调查复现

Rust 库与 CLI 可独立编译。只有真实样本研究/差分脚本依赖本机旧移植项目的 `inspect_neko.py`、`ArcNEKO.cs` 和 `TDED_analysis/GARbro/nekopack.lst`。这些研究依赖没有复制进本仓库；其他机器需自行提供样本和参考工具才能重跑真实数据差分。输出记录了解码器 SHA-256。

```powershell
python research/audit_samples.py --games 'D:\codex\电脑移植安卓\游戏' --decoder-dir 'D:\codex\电脑移植安卓\research' --output research/sample-audit.json
python -m unittest discover -s research -p 'test_*.py' -v
python research/verify_rust_archive.py --games 'D:\codex\电脑移植安卓\游戏' --decoder-dir 'D:\codex\电脑移植安卓\research' --cli target/release/nekopack-cli.exe --output research/rust-archive-verification.json
python research/verify_metadata.py --games 'D:\codex\电脑移植安卓\游戏' --decoder-dir 'D:\codex\电脑移植安卓\research' --cli target/release/nekopack-cli.exe --output research/metadata-verification.json
python research/verify_pixels.py --games 'D:\codex\电脑移植安卓\游戏' --decoder-dir 'D:\codex\电脑移植安卓\research' --cli target/release/nekopack-cli.exe --output research/pixel-verification.json
```

脚本只读取游戏文件，输出文件哈希、计数和格式元数据，不导出剧情、图像、音频或引擎二进制。文件名中的汉化版本标记不等于已确认的原版版本号。完整资源解码、字符编码、脚本语义、启动流程、原版行为对照及真机测试仍待验证。

资源与显示增量已完成素材坐标、显式图层合成及主要对照错误修正，逐像素原版一致性仍待无损帧验证。当前工作优先推进脚本执行主线：启动 AST、变量/调用状态、`.nut` 系统命令注册与标题菜单调用链。不继续围绕有损截图拟合颜色，也不提前搭建空壳 VM 或臆造指令；未知指令必须报告，不能静默跳过。
