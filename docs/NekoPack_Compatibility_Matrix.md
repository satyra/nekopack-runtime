# 兼容性矩阵（逐样本记录）

日期：2026-11-06。统一状态：已实现并验证、部分实现、未实现、未知、不适用。游戏运行器尚未实现；新增 Rust /1 资源层，下面各作品原始计数来自 Phase 0 的 [L1](../research/sample-audit.json)，Rust 差分证据见 [L2](../research/rust-archive-verification.json)。静态检查通过不能填写为“可运行”。方法及限制见 [Sources.md](Sources.md)。

## Tiny Dungeon BLACK and WHITE

- 本地版本：含汉化标记目录，精确官方发行/补丁版本未知；以 L1 文件 SHA-256 固定副本。
- 9 个 DAT，seed 4B0EC5B3，全部顺序值 0。
- 索引及资源块长度：静态检查通过。script.dat 有 254 条；system.dat 有 39 条。
- voice1–voice5 为独立包；每包存在 1 个未知目录哈希 EFFB4B01 条目，其含义未知。
- MNG：system.dat 前缀识别 20 个，抽样 3 个块 CRC 通过。

## Tiny Dungeon BLESS of DRAGON

- 本地版本：汉化硬盘版目录；原版和汉化 EXE 并存，精确运行版本未知。
- 4 个 DAT，seed 24315C66，顺序值 0。
- 索引及资源块长度：静态检查通过。system.dat 包含 script 174 条、system 38 条。
- MNG：前缀识别 20 个，抽样 3 个块 CRC 通过。

## Tiny Dungeon BIRTH for YOURS

- 本地版本：汉化硬盘版目录，含 patch101.dat；以文件指纹固定。
- 5 个 DAT，seed 24315C66。patch101 顺序 301，其余 0。
- 索引及资源块长度：静态检查通过。基础 system.dat 含 script 159 条、system 39 条；patch101 另有 script 32 条。有效脚本数不能直接相加，覆盖关系待核对。
- MNG：前缀识别 21 个，抽样 3 个块 CRC 通过。

## Tiny Dungeon BRAVE or SLAVE

- 本地版本：汉化硬盘版目录，精确官方版本未知。
- 4 个 DAT，seed 9B75ADA6，顺序值 0。
- 索引及资源块长度：静态检查通过。system.dat 含 script 203 条、system 46 条。
- MNG：前缀识别 23 个，抽样 3 个块 CRC 通过。

## 四部曲共同状态

- 研究工具目录/封包枚举：已实现并验证（仅本次固定副本）。
- Rust /1 DAT 索引、块边界/长度/parity 校验、资源前缀读取、流式解密：已实现并验证（22 个包全索引/条目前缀，完整 payload 分层抽样）。
- 资源名还原、包挂载与补丁覆盖、MNG 解码：未实现。
- 脚本：静态 token 分析已实施；完整语法与执行语义未知；VM 未实现。
- 进入游戏、剧情播放、渲染、动画播放、音频播放、存读档、Android：未实现。
- 真实原版行为对照、音频 codec 和循环点、汉化编码、同优先级挂载顺序：未知。
- 本次没有 EXE 在 /1 候选检查中获得有效内嵌封包；不是对任意嵌入/压缩方式的否定。

## 外部参考作品与其他变种

Niflheim 作者报告的「ヒマワリと恋の記憶」：本项目未取得样本，全部本地兼容验证未知。[N1]

GARbro /2、/3：有公开读取实现，未验证样本；不能用四部曲 /1 结果替代。[G2]

Endless Dungeon 在本机目录中可见，但本轮按用户指定的四部曲范围排除，未列为已验证样本。
