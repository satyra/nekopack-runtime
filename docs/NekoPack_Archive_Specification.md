# NEKOPACK 封包与 MNG 调查规格

范围：Niflheim/NekoPackExtract/GARbro /1 的共同布局，以及本轮四部曲静态结果。所有整数除 MNG 外为小端。以下是实现证据归纳，不是官方格式声明。[N2、X2、G1、L1]

## /1 DAT 布局

- 0x00：8 字节 ASCII `NEKOPACK`。
- 0x08：u32 名称哈希 seed；X2 称 Hash。
- 0x0C：u32 order/priority；X2 称 Padding。N1 报告越大越优先。[L1] 的 patch101 为 301，其余包为 0。
- 0x10：u32 索引 parity / 解密 seed；0 表示未加密。
- 0x14：u32 索引字节数 N。
- 0x18：N 字节索引数据。
- 0x18+N 起：按索引顺序排列资源块。

解密索引连续包含目录记录：`directory_hash:u32, count:u32`，随后 count 个 `filename_hash:u32, length:u32`。没有明文文件名、显式资源偏移或已确认的官方版本字段。资源块是 `parity:u32, length:u32, payload:length bytes`。从 0x18+N 开始逐块累加 8+length 即可预建随机访问索引。磁盘块之间**不按 8 字节补齐**；解码器的补齐仅用于内存运算。[N2 ReadIndex/Export；X2 ReadBuffer；G1 ReadBlock]

## 解密与哈希

对块 seed h，在 u32 模 2^32 运算下：

```text
t1 = h XOR (h + 0x5D588B65)
t2 = t1 XOR (h - 0x359D3E2A)
low = t2 XOR (t1 - 0x70E44324)
high = low XOR (t2 + 0x6C078965)
key = low | (high << 32)
```

每 8 字节与 key 异或，随后分别将解密出的四个 u16 加入 key 的对应 u16 通道（模 2^16），通道间不进位。尾部可在内存补零到 8 字节，输出截断回实际长度。N2 按 u16 循环并针对奇数长度多分配 1 字节；应采用明确的零初始化实现，不能照搬其未初始化尾字节读取。[N2 Decode；X2 Decode；G1]

名称哈希从 archive seed 开始，对原始名称字节查 ShiftMap/name_table，然后执行 `h = 81 * (table[byte] XOR h)`，取低 32 位。须保存编码、路径分隔与表映射规则；不可先随意 Unicode 小写或路径规范化。名称恢复是候选哈希匹配，不是反解哈希。[N2 GetNameHash；G1 NekoEncryption32bit]

N2 的 parity 是 seed 与 length 的派生值，不是 payload 内容摘要。归一化计算：`t = (length XOR ((seed XOR length)+0x5D588B65))-0x359D3E2A`，`v = (length XOR ((length XOR t)-0x70E44324))+0x6C078965`，结果是 `rotl32(v,t>>27)`，每步按 u32 回绕。值为 0 时 N2 跳过验证与解密。本次 L1 没有执行 parity 校验，后续需专测移位为 0 和溢出情况。[N2 ParityCheck]

未在该 DAT 外层读取路径发现压缩步骤；PNG/Ogg 自身编码是另一层。不能由此认定其他引擎版本没有外层压缩。多 voice*.dat 是多个独立包，未见本次索引跨包引用机制。[X2、N2、L1]

## 其他布局不能误用 /1

GARbro /2 在 0x0C 读取初始化 key，0x10 读取 seed，解密 0x14 的双份长度，索引在 0x1C，目录含重复 count；使用 NekoXCode，而非 /1 加密。/3 含 0x400 字节 key 区，索引从 0x41C 读取，索引含字符串名称和偏移。二者仅源码审查，未有对应本机样本验证；不得简单尝试 /1 然后声称自动识别所有版本。[G2]

## MNG

签名 `8A 4D 4E 47 0D 0A 1A 0A`。随后为大端长度、4 字节块类型、数据和 CRC 的序列。MHDR 的七个 u32 为画布宽高、ticks、layer count、frame count、play time、simplicity。图像 IHDR/IDAT/IEND 之外，还需处理 FRAM、DEFI、TERM 等控制语义；MEND 结束流。[X3、N4、M1]

本轮每个 system.dat 最多抽取前三个 MNG，只在内存中读取：共 12 个 MNG，453 组 IHDR/IEND、453 个 tEXt、425 个 DEFI，全部块 CRC 通过。包含 FRAM 和 TERM；没有执行帧合成、循环、透明度/图层回放。块统计不是完整支持列表。[L1]

tEXt 的原始字节、FRAM 名称及 DEFI 对象信息必须保留。作者报告 tEXt 参与引擎索引，因此导出成一串 PNG 不等同资源语义完整；对应的具体关键字和运行时选择规则尚待逐资源对照。[N1、X3]

Phase 1 必须验证：截断、索引计数越界、长度不符、整数溢出、奇数尾长、零 seed、哈希碰撞和大包；随机访问使用 u64 文件偏移。MNG 验证必须包含 CRC、块顺序、图像解码、计时与原版显示对照，遇到未支持控制块显式报错。（设计）

来源索引：[Sources.md](Sources.md)。
