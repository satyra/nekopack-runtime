# NekoPack Android Runtime

目标：Rust + Kotlin 实现独立、通用的 Android ARM64 NekoPack 游戏运行环境，由玩家导入自己的游戏。当前为 **Phase 0 技术调查，等待审核**；没有运行时、Android 工程或可安装 APK。

2026-11-06：检查 NekoPackExtract、Niflheim/NEKOPACK、GARbro，并用本机 Tiny Dungeon 四部曲进行只读静态调查。22 个 DAT 的 51,201 个资源条目通过索引范围及块长度检查；抽样 12 个 MNG 的块 CRC 通过。上述结果来自研究工具，不是新运行器的兼容性认证。

## 调查文档

- [技术研究](docs/NekoPack_Technical_Research.md)
- [封包规格](docs/NekoPack_Archive_Specification.md)
- [脚本分析](docs/NekoPack_Script_Analysis.md)
- [运行时架构](docs/NekoPack_Runtime_Architecture.md)
- [兼容性矩阵](docs/NekoPack_Compatibility_Matrix.md)
- [开发路线](docs/NekoPack_Development_Roadmap.md)
- [证据与版本](docs/Sources.md)
- [本地样本元数据](research/sample-audit.json)

## 调查复现

`research/audit_samples.py` 是 Phase 0 调查辅助脚本，依赖本机旧移植项目的 `inspect_neko.py`、`ArcNEKO.cs` 和 `TDED_analysis/GARbro/nekopack.lst`。这些依赖没有复制进本仓库；因此其他机器不能仅克隆本仓库就运行完整样本调查。输出记录了解码器 SHA-256；后续 Phase 1 将实现独立 Rust 解析器并交叉验证。

```powershell
python research/audit_samples.py --games 'D:\codex\电脑移植安卓\游戏' --decoder-dir 'D:\codex\电脑移植安卓\research' --output research/sample-audit.json
python -m unittest discover -s research -p 'test_*.py' -v
```

脚本只读取游戏文件，输出文件哈希、计数和格式元数据，不导出剧情、图像、音频或引擎二进制。文件名中的汉化版本标记不等于已确认的原版版本号。完整资源解码、字符编码、脚本语义、启动流程、原版行为对照及真机测试仍待验证。

Phase 0 审核后再开始 Phase 1；不提前搭建空壳 VM 或臆造指令。未知指令必须报告，不能静默跳过。
