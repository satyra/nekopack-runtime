# NekoPack Android Runtime

目标：Rust + Kotlin 实现独立、通用的 Android ARM64 NekoPack 游戏运行环境，由玩家导入自己的游戏。Phase 0 已获用户审核，当前进入 **Phase 1：独立 Rust /1 DAT 只读资源层**；没有游戏 VM、Android 工程或可安装 APK。

2026-11-06：完成公开源码调查后，新增无外部 crate 依赖的 `nekopack-archive` 和 `nekopack-cli`。Rust 资源层通过四部曲 22 个 DAT、51,201 个条目的差分验证；索引、全部条目元数据和前 16 字节与旧 Python 参考解码器一致。完整资源仅分层抽样，不代表所有图像/音频都已解码或游戏可运行。结果见 [Phase 1 验收](docs/Phase1_Archive_Increment.md)。

## 使用

```text
cargo run -p nekopack-cli -- inspect-archive path/to/system.dat
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --workspace
```

`inspect-archive` 输出 TSV：第一行是 name seed、priority、索引长度、条目数和尾随字节数；后续行列出条目序号、目录/文件哈希、块头偏移、长度、key 和解密前缀。`read-entry FILE ORDINAL` 与 `read-index FILE` 向 stdout 输出原始二进制，供程序调用；错误走 stderr 并返回非零状态。二进制输出不要经过会转码的文本管道。

库使用 `Archive::open(Read + Seek, max_index_bytes)`；资源通过 `copy_entry(ordinal, Write)` 用 64 KiB 缓冲流式读取。CLI 的索引预算是 16 MiB，不是 DAT 格式的固有限制。打开后源文件必须保持不变。重复哈希保留原始序号，不做隐式覆盖。此版本显式面向 /1 布局，不实现 /2、/3 自动探测、名称还原或补丁挂载。

本机 Rust 工具链位于用户目录，必要时在当前 PowerShell 会话设置：

```powershell
$env:RUSTUP_HOME="$env:LOCALAPPDATA/RustGNU/rustup"
$env:CARGO_HOME="$env:LOCALAPPDATA/RustGNU/cargo"
$env:Path="$env:CARGO_HOME/bin;$env:Path"
```

其他已安装 Rust 的环境使用自己的工具链即可。本次验证工具链：Rust 1.99.1，Windows GNU；尚未验证 Android target。

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
- [第三方算法来源声明](THIRD_PARTY_NOTICES.md)

## 调查复现

Rust 库与 CLI 可独立编译。只有真实样本研究/差分脚本依赖本机旧移植项目的 `inspect_neko.py`、`ArcNEKO.cs` 和 `TDED_analysis/GARbro/nekopack.lst`。这些研究依赖没有复制进本仓库；其他机器需自行提供样本和参考工具才能重跑真实数据差分。输出记录了解码器 SHA-256。

```powershell
python research/audit_samples.py --games 'D:\codex\电脑移植安卓\游戏' --decoder-dir 'D:\codex\电脑移植安卓\research' --output research/sample-audit.json
python -m unittest discover -s research -p 'test_*.py' -v
python research/verify_rust_archive.py --games 'D:\codex\电脑移植安卓\游戏' --decoder-dir 'D:\codex\电脑移植安卓\research' --cli target/release/nekopack-cli.exe --output research/rust-archive-verification.json
```

脚本只读取游戏文件，输出文件哈希、计数和格式元数据，不导出剧情、图像、音频或引擎二进制。文件名中的汉化版本标记不等于已确认的原版版本号。完整资源解码、字符编码、脚本语义、启动流程、原版行为对照及真机测试仍待验证。

下一增量是名称哈希/包来源与补丁关系调查，以及 MNG 控制块与资源元数据解析。不提前搭建空壳 VM 或臆造指令；未来未知指令必须报告，不能静默跳过。
