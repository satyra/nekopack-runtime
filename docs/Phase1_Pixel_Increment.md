# Phase 1：嵌入 PNG 像素解码

2026-11-06。在名称/覆盖关系/MNG 元数据之后，本增量完成四部曲实际使用的独立嵌入 PNG 解码、RGBA 数据读取和单图 PNG 导出。没有把独立图像导出等同 MNG 播放器。

## 实现

`nekopack-image` 新增：

- `decode_png(bytes, max_rgba_bytes)`：PNG 到 RGBA8，支持低位深灰度/索引色展开、tRNS、RGB、RGBA、灰度透明、Adam7。保持 straight alpha，保留透明像素的 RGB，不做 gamma/ICC 转换，不预乘。
- `Mng::embedded_images()`：返回 PNG 图像范围及最近前置 FRAM/DEFI 的块序号。块序号是来源引用，不是计算好的对象状态、坐标继承或动画帧。
- `Mng::decode_image(ordinal, max_rgba_bytes)`：从原始 IHDR–IEND 块重建标准 PNG 数据流，再解码。MNG 中的 FRAM/DEFI/tEXt 原始数据仍在原对象中。
- `RgbaImage::write_png(output)`：导出 RGBA 像素。导出文件不保留原始 MNG 的控制块或引擎命名信息，不能用于替换游戏资源。

PNG 压缩/过滤/交错由 `png 0.18.1` 处理，版本及传递依赖记录在 Cargo.lock，避免自行重写 DEFLATE。archive/resource 仍没有外部 crate 依赖；整个 workspace 从此不再是零依赖。[png 解码器文档](https://docs.rs/png/0.18.1/png/)。

为了避免隐藏不支持的差异，16 位采样、APNG、MNG filter 64、全局/继承 PLTE/tRNS、JNG/BASI/delta-PNG 和未知 critical 像素依赖会明确报错。元数据检查仍可以保留未知块；能检查元数据不代表能解码像素。全局调色板继承与 filter 64 是 MNG 的专门机制，不能直接丢弃后假装得到等价 PNG。[MNG 规范](https://www.libpng.org/pub/mng/spec/)。

## 命令

```text
nekopack-cli decode-png image.png
nekopack-cli decode-mng-archive system.dat
nekopack-cli export-mng-image system.dat ENTRY_ORDINAL IMAGE_ORDINAL
```

前两条输出二进制 `NRGBA001` 流：8 字节标识，随后每条记录为 7 个小端 u32（封包条目序号、嵌入图序号、宽、高、前置 FRAM 块序号、前置 DEFI 块序号、RGBA 字节数）及 RGBA 数据。没有对应控制块时序号为 0xFFFFFFFF。图像独立解码，不缩放到 MNG 画布、不按 DEFI 移动、不执行 FRAM 定时。标准 PNG 输入使用 0/0 图像序号。

第三条输出标准 PNG 二进制。可用 Python `subprocess.run(..., capture_output=True, check=True).stdout` 接收并保存；不要让二进制经过会转码的文本管道。批量解码遇到后续错误可能已经输出前面的记录，只有退出码为 0 才表示整包成功。

CLI 输入资源预算 64 MiB、单图 RGBA 预算 64 MiB；同一时间只处理一张图。`png` 内部分配也设置预算，必要的压缩数据拷贝和色彩展开缓冲另占内存，因此该值不是整个进程 RSS 上限。库调用方控制自己的预算。没有一次保存全部图像的像素缓存。

## 四部曲结果

证据：[pixel-verification.json](../research/pixel-verification.json)，脚本：[verify_pixels.py](../research/verify_pixels.py)。样本源文件和 MNG 解密内容的 SHA-256 均与上一增量清单一致。

- BLESS of DRAGON：20 个 MNG、702 张图像。
- BIRTH for YOURS：21 个 MNG、768 张图像。
- BRAVE or SLAVE：23 个 MNG、882 张图像。
- BLACK and WHITE：20 个 MNG、720 张图像。

总计 84 个 MNG、3,072 张图像、527,843,812 字节 RGBA，与 Python Pillow 独立解码逐字节相同，包含 alpha 为 0、1、128、255 的像素；每个 MNG 首图导出 PNG 后再由 Pillow 解码，84 份全部一致。验证过程逐张通过管道传输像素，仓库只记录尺寸、序号、控制块引用、计数及哈希，不保存游戏图像。

实际所有 IHDR 都是位深 8、颜色类型 6、压缩 0、过滤 0、非交错。全体 FRAM 的 mode 为 2，2,590 个 DEFI 长度均为 12，TERM 原始字段相同。这是控制数据盘点，不是播放语义验证；大量具名图像也不能简单按序当作连续动画。

合成测试涵盖独立生成的 RGBA/RGB/灰度/灰度透明/索引透明/Adam7 数据，透明 RGB 保留，导出回读，截断、CRC、预算、16 位拒绝、未知依赖拒绝及控制块来源关联。31 项 Rust/CLI 测试、fmt 与 Clippy 通过；release 构建通过。原有 6 项 Python 研究测试作为回归检查。真实 image.dat 中的全部独立 PNG 未在本轮逐张解码；真实像素验证范围是上述 84 个 MNG。

## 复现

```powershell
cargo build --release --workspace
python research/verify_pixels.py --games 'D:\codex\电脑移植安卓\游戏' --decoder-dir 'D:\codex\电脑移植安卓\research' --cli target/release/nekopack-cli.exe --output research/pixel-verification.json
```

研究脚本需要 Pillow、本机游戏与原有参考 DAT 解码器。Rust 运行器代码不依赖 Python/Pillow。

下一步应在已验证像素基础上实现对象位置、画布合成与控制块状态，并对照原版 UI 的资源选择、实际显示和交互。尚未运行原版进行逐帧对照，也没有 Android APK。
