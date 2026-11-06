# Phase 1：名称、覆盖关系与 MNG 元数据

2026-11-06。本增量完成用户要求的名称哈希、补丁关系分析、MNG 元数据读取，并继续使用同一组四部曲样本。仍不包含脚本 VM、像素解码、动画播放或 Android APK。

## 名称与候选查找

`nekopack-archive::name_hash(seed, bytes)` 使用 GARbro /1 的 256 项 ShiftMap，按原始字节进行 u32 回绕运算。表来自 MIT 授权的 G1，许可证保留在 THIRD_PARTY_NOTICES.md。不将 Unicode 字符串自动转码、不进行额外大小写或路径归一化；ASCII 大小写和斜杠映射效果由原表决定。

CLI `hash-name SEED_HEX NAME_BYTES_HEX` 的 seed 和名称输入均为十六进制。例如 script 的字节是 `736372697074`。非 ASCII 名称由调用方用实际游戏编码转为字节后输入；这避免错误地把 CP932、CP936 与 UTF-8 混为一谈。

`Catalog::lookup(directory_bytes, filename_bytes)` 针对每个包自己的 seed 计算哈希，返回全部匹配候选及来源。它不是哈希反解器，不宣称自动恢复未知文件名，也不把一次哈希匹配当作不存在碰撞的证明。四部曲的 `script/start.txt` 与 `script/title.txt` 均找到候选；这不证明它们一定是原版启动入口。

## 覆盖关系

新增 `nekopack-resource`：保存包头、条目、来源序号和原始条目序号。未知名称的重叠按 `(seed, directory_hash, name_hash)` 分组，避免将不同 seed 的相同数值错误合并。已知名称可以用 lookup 跨 seed 查询。

`resolve` 是显式的“数值最高 priority 优先”分析策略，依据 Niflheim 作者说明；其状态是 Missing、Selected 或 Ambiguous。最高 priority 有多个候选时保留歧义；包内重复哈希、重复传入的文件也不会静默消失。来源序号仅表示传入顺序，不代表引擎读取顺序。

真实样本结果：

- BIRTH for YOURS：patch101.dat 的 32 个条目全部与基础包 script 条目形成重叠。priority 301 对基础包 0，策略均选择补丁候选。64 份基础/补丁 payload 与独立 Python 解密结果逐字节相等；32 对内容的 SHA-256 均不同。
- BLACK and WHITE：voice1–voice5 的未知目录 EFFB4B01、名称 CCC861C7 形成一组 5 候选重叠，priority 都是 0；结果为 Ambiguous。此次不重新解码这些总计近 1 GB 的未知资源，也不推测它们的用途。
- BLESS of DRAGON 与 BRAVE or SLAVE：本次相同 seed/hash 分组没有跨包重叠。

这确认了包内数据关系及本项目策略，不等同原版引擎动态挂载实验。同优先级实际读取顺序仍未知；跨 seed 的未知名称也没有自动合并。

## MNG 元数据

新增 `nekopack-image::Mng::parse(bytes, max_chunks)`，借用原始资源数据，保留有序块、偏移、长度、CRC、原始 payload。已实现类型化 MHDR、FRAM、DEFI、TERM、tEXt、IHDR，以及 IDAT/IEND/MEND 标记。

FRAM 保留名称原始字节、变更标志、可选 delay/timeout、带符号剪裁和 sync IDs；DEFI 保留对象号、可选显示标志及带符号位置；TERM 保留终止/重复信息。缺省字段与显式零值、空列表区分。tEXt 不经有损字符转换，不丢弃原始字节。字段布局依据 [MNG 1.0 规范](https://www.libpng.org/pub/mng/spec/mng-1.0-20010209-pdg.html)。

解析器检查签名、块范围、CRC、MHDR 首块/唯一性、MEND 末块、已知元数据长度及基本图像边界。未知块保留为 Unknown，critical 标志显式暴露。它不是完整 PNG/MNG 语义验证器：不解压 IDAT，不验证像素、完整控制块顺序、动画状态或循环行为。

CLI：

```text
nekopack-cli hash-name 9B75ADA7 736372697074
nekopack-cli lookup-name 736372697074 73746172742E747874 system.dat patch101.dat
nekopack-cli inspect-overlays system.dat patch101.dat
nekopack-cli inspect-mng animation.mng
nekopack-cli inspect-mng-archive system.dat
```

lookup/overlays 的 source 是命令行文件列表中从 0 开始的序号。MNG 输出 `mng` 头部字段和 `chunk` 行：资源序号、块序号、偏移、类型、长度、CRC、未知 critical 标记、原始元数据 hex；IDAT/JDAT/JDAA 不打印媒体 payload。库保留所有原始块，CLI 限制单 MNG 64 MiB、100,000 个块；这些是预算，不是格式上限。

## 四部曲验证

完整证据：[metadata-verification.json](../research/metadata-verification.json)，可通过 [verify_metadata.py](../research/verify_metadata.py) 复现。

- 22 个 DAT 的完整文件 SHA-256 与既有样本清单一致。
- 完整解密索引及 51,201 个条目的元数据、解密前缀再次核对通过。
- 277 组名称哈希向量与 Python 参考一致：包括全部单字节值、三个实际 seed、整张字节表及 CP932 字节样例。
- 32 次已知名称查询与独立索引枚举一致；共 33 组重叠的候选来源、priority 和决策一致。
- 全部 84 个 MNG：第一部 20、第二部 20、第三部 21、第四部 23。共 27,712 个块，CRC、块位置/长度、MHDR 值和全部保留的原始元数据与 Python struct/zlib 参考一致。
- 共 3,072 个 IHDR、3,072 个 FRAM、3,072 个 tEXt、2,590 个 DEFI；嵌入图像数量不是已验证的播放帧数。

测试：26 项 Rust/CLI 测试、6 项已有 Python 测试通过；fmt、Clippy、release 构建通过。类型化可选字段、带符号坐标、空 FRAM、显式空同步列表、损坏 CRC、截断和来源歧义有合成测试。真实 MNG 差分覆盖原始块与头部，并不声称已独立验证所有类型化字段的动态作用。

验证环境仍为 Windows GNU Rust 1.99.1。研究脚本需要本地游戏及旧 inspect_neko.py 参考工具；Rust 四个 crate 均可独立构建，无第三方 crate 依赖。仓库不包含游戏资源、媒体或对白；CLI 诊断输出可能包含资源内命名字节，完整输出留在本地，不随报告上传。

## 下一步

在已读取的 MNG 元数据上建立像素解码与控制块状态模型，先选择有明确原版对照的 UI 资源；继续核实补丁实际加载顺序与脚本入口。名称查询、静态优先级策略和 MNG 元数据完成不代表完整 Phase 1 或通用运行器完成。
