# 证据与复现边界

调查日期：2026-11-06。文档中的来源编号指向以下固定版本；“设计”是工程提案，“未知”表示没有得到足够证据。工具作者陈述、源码行为、本地静态观测与原版动态行为必须分开。

- **N1**：[Niflheim NEKOPACK 说明](https://github.com/Yggdrasill-Moe/Niflheim/blob/987626932c30809ed1621de4eb252c7f5353fdbe/NEKOPACK/README.md)。作者报告针对「ヒマワリと恋の記憶」制作；name.txt 不是通用文件名全集。
- **N2**：[NEKOPACK.h](https://github.com/Yggdrasill-Moe/Niflheim/blob/987626932c30809ed1621de4eb252c7f5353fdbe/NEKOPACK/NEKOPACK/NEKOPACK.h) 与 [NEKOPACK.cpp](https://github.com/Yggdrasill-Moe/Niflheim/blob/987626932c30809ed1621de4eb252c7f5353fdbe/NEKOPACK/NEKOPACK/NEKOPACK.cpp)：ReadIndex、Export、GetKey、Decode、GetNameHash、ParityCheck、Make。
- **N3**：[NEKOPACK_text.py](https://github.com/Yggdrasill-Moe/Niflheim/blob/987626932c30809ed1621de4eb252c7f5353fdbe/NEKOPACK/NEKOPACK_text.py)：文本提取/回写，不是解释器。
- **N4**：[NEKOPACK_mng.cpp](https://github.com/Yggdrasill-Moe/Niflheim/blob/987626932c30809ed1621de4eb252c7f5353fdbe/NEKOPACK/NEKOPACK_mng/NEKOPACK_mng.cpp)：CheckHeader、Export、Import；仓库另有 libmng 回调实验文件。
- **X1**：[NekoPackExtract README](https://github.com/xmoezzz/NekoPackExtract/blob/5c0365f56336be39d5f6ec88057c238e22439376/README.md)、[MIT 许可证](https://github.com/xmoezzz/NekoPackExtract/blob/5c0365f56336be39d5f6ec88057c238e22439376/LICENSE)。仓库带 EXE、PDB、构建中间产物；本轮没有运行这些二进制，也没有完成二进制逆向。
- **X2**：[UnpackDataPack.cs](https://github.com/xmoezzz/NekoPackExtract/blob/5c0365f56336be39d5f6ec88057c238e22439376/Tools/source/NekoPackExtractTool/NekoPackExtract/UnpackDataPack.cs)：ReadBuffer、Decode、Unpack。
- **X3**：[MNG.cs](https://github.com/xmoezzz/NekoPackExtract/blob/5c0365f56336be39d5f6ec88057c238e22439376/Tools/source/NekoPackExtractTool/NekoPackExtract/MNG.cs)、[PackMNG.cpp](https://github.com/xmoezzz/NekoPackExtract/blob/5c0365f56336be39d5f6ec88057c238e22439376/Tools/source/PackMNG/PackMNG/PackMNG.cpp)：嵌入 PNG 提取和 FRAM/tEXt 保留逻辑。
- **G1**：[GARbro ArcNEKO.cs](https://github.com/morkt/GARbro/blob/b09ee4570ccb1daf6ac56710ee8934dc0b8baeb0/ArcFormats/Nekopack/ArcNEKO.cs)：Pak1Opener、IndexReader、NekoEncryption32bit。
- **G2**：[ArcNEKO2.cs](https://github.com/morkt/GARbro/blob/b09ee4570ccb1daf6ac56710ee8934dc0b8baeb0/ArcFormats/Nekopack/ArcNEKO2.cs)、[ArcNEKO3.cs](https://github.com/morkt/GARbro/blob/b09ee4570ccb1daf6ac56710ee8934dc0b8baeb0/ArcFormats/Nekopack/ArcNEKO3.cs)。/1、/2、/3 是 GARbro 格式标签，不是已确认的官方引擎版本号。源文件包含 MIT 授权。
- **M1**：[MNG 1.0 规范](https://www.libpng.org/pub/mng/spec/mng-1.0-20010209-pdg.pdf)：标准层面的动画语义参考；不能据此推定游戏实际使用的所有特性。
- **A1**：[Android SAF 官方说明](https://developer.android.com/training/data-storage/shared/documents-files)：目录选择、URI、持久授权及访问限制。
- **L1**：[sample-audit.json](../research/sample-audit.json)，由 [audit_samples.py](../research/audit_samples.py) 本次读取本地四部曲生成。完整文件 SHA-256、解密索引 SHA-256、文本候选哈希、计数和抽样 MNG CRC 均在报告中。没有运行 Windows 游戏或 Android 应用。

本地研究工具来自 `D:\codex\电脑移植安卓\research\inspect_neko.py`。其解密来自 GARbro 算法；本轮不是独立双实现差分测试。仅对所有 payload 解密前 16 字节以分类，完整读取 script/system 文本候选并对每包最多 3 个 MNG 做块结构与 CRC 检查。没有完整解码所有 PNG/Ogg/MNG，没有验证 DAT parity，没有验证所有脚本文字编码。

以上段落描述 Phase 0。**L2（Phase 1 新增）**：[rust-archive-verification.json](../research/rust-archive-verification.json)，由 [verify_rust_archive.py](../research/verify_rust_archive.py) 生成。Rust 按逐字节/u16 lane 反馈实现，旧 Python 按 u64 分组实现；二者独立代码路径，但依据相同公开格式，不是原版引擎 oracle。逐包核对原始文件 SHA-256、完整解密索引、全部条目元数据和前缀，并对选中资源做完整字节相等比较。Rust 另外校验全部非零 parity。报告记录参考工具与 CLI 的 SHA-256；没有验证完整游戏行为。

Niflheim 克隆中未发现独立 LICENSE 文件，因此只作证据引用，不把其源码或专用词表直接纳入本项目。仓库当前不分发游戏数据、参考工具二进制或上游源码。

**L3（名称与元数据增量）**：[metadata-verification.json](../research/metadata-verification.json)，由 [verify_metadata.py](../research/verify_metadata.py) 生成。包含全部单字节与实际 seed 的名称哈希向量、逐包来源和内容指纹、33 组重叠关系、全部 84 个 MNG 的块统计与哈希。MNG 原始块以独立 Python struct/zlib 校验；补丁内容以旧解密器交叉验证。没有原版动态播放验证。

本增量已将 G1 的 MIT 授权 ShiftMap 数值表移入 names.rs，许可保留在 [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)；上文“不分发上游源码”指 Phase 0，不再适用于此表。MNG 元数据布局另查阅 [MNG 1.0 HTML 规范](https://www.libpng.org/pub/mng/spec/mng-1.0-20010209-pdg.html)。未知 critical 块只保留并标记，不宣称支持其运行语义。
