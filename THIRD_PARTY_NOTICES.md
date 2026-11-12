# 第三方声明、许可与格式来源

本项目原创源码采用 **GPL-3.0-only**，仅 GNU GPL 第 3 版，完整原文见根目录 [LICENSE](LICENSE)。第三方组件保留下述各自的许可与声明；项目许可不为外部游戏资源或补丁提供授权。

Android 分发的项目与第三方许可快照保存在 [platforms/android/licenses](platforms/android/licenses/)。[NOTICE](platforms/android/licenses/NOTICE) 提供组件清单，[index.json](platforms/android/licenses/index.json) 记录版本、原文文件、来源和 SHA-256。该目录可整体递归复制到 APK 的 `assets/licenses/`，包括 `NOTICE` 和 `index.json`，无需排除文件。[project/LICENSE](platforms/android/licenses/project/LICENSE) 与根目录 GNU 官方 GPL v3 原文逐字节一致。许可文本不代替分发二进制时所需的完整对应源码，交付要求见 [发布指南](docs/RELEASING.md)。

快照覆盖 `Cargo.lock` 的全部 16 个 registry 包（两版 `miniz_oxide` 分别记录）、Kotlin stdlib 1.9.22、Android NDK r27d 的 `libc++_shared`、Squirrel 2.2.5 和 pl_mpeg。Rust 许可原文来自与锁文件 SHA-256 一致的 `.crate` 发行包，保留发行包提供的替代许可文件；收录清单不表示每个 crate、feature 或文件都链接到每个 APK。Kotlin stdlib JAR 参与 D8 转换；其许可和 NOTICE 原文来自对应的 [JetBrains Kotlin v1.9.22 标签](https://github.com/JetBrains/kotlin/tree/v1.9.22/license)，原始 NOTICE 的标题描述 Kotlin Compiler 发行版，未在快照中改写。NDK 的完整工具链 NOTICE 也包含超出已打包 C++ 运行库范围的声明，保留原文而不截取或概括。

快照另保留 Gradle 8.7 发行包的完整 LICENSE 和 NOTICE，作为构建工具来源记录。Android 构建脚本只使用该发行包内的编译工具及 Kotlin stdlib；这些聚合声明不表示 Gradle、Kotlin 编译器插件或所有 Gradle 依赖都被打包进 APK。FFmpeg 仅用于本地独立对照，不链接或打包进入运行器。

使用 Python 3.11+ 从本地发行物重新收集：

```powershell
python platforms/android/update_licenses.py `
  --cargo-home "$env:LOCALAPPDATA/RustGNU/cargo" `
  --ndk D:/codex/tools/android/android-ndk-r27d `
  --kotlin-jar D:/codex/tools/android/gradle-8.7/lib/kotlin-stdlib-1.9.22.jar
```

默认完全离线：Kotlin 的固定版本原文使用已提交快照，按脚本内固定 SHA-256 验证；`--refresh-kotlin` 才会从上述官方标签重新下载并验证。Cargo 发行包必须已存在于指定缓存且匹配 `Cargo.lock`；NDK 和 Gradle 必须为上述版本。更新锁文件或发行工具后需重新收集并审阅索引；脚本会报告旧快照遗留文件，不自动删除。构建 APK 使用已提交快照，不要求收集工具访问网络。

系统执行增量使用 Alberto Demichelis 的 **Squirrel 2.2.5 stable** 开源解释器源码子集。该版本随附的许可为 zlib 风格条款，不套用新版 Squirrel 的许可标签。完整原始声明保留在 [COPYRIGHT](vendor/squirrel2/COPYRIGHT)；来源、下载包 SHA-256 与本地修改见 [修改清单](vendor/squirrel2/NEKOPACK_CHANGES.md)。这些源码并非本项目原创，修改版已明确标记。没有复制商业游戏 EXE/DLL 或游戏系统脚本。

像素增量新增 Cargo 依赖 `png 0.18.1`（MIT OR Apache-2.0），通过其公开 API 解码/编码 PNG。
锁定的相关依赖还包含 bitflags、crc32fast、cfg-if、fdeflate、flate2、miniz_oxide、adler2、simd-adler32 和 zlib-rs；具体版本及许可原文见上述索引。其中 simd-adler32 为 MIT，zlib-rs 为 Zlib；其他这些包提供 MIT 许可选项。字体与文字处理还使用 ab_glyph、ab_glyph_rasterizer、owned_ttf_parser、ttf-parser 和 encoding_rs，完整许可均已收录。encoding_rs 的声明为 `(Apache-2.0 OR MIT) AND BSD-3-Clause`，其 WHATWG 声明与替代许可一并保留。本仓库不复制这些依赖源码或发布编译后的游戏资源。
参考：[png 0.18.1](https://docs.rs/png/0.18.1/png/)、[image-png 源码与许可](https://github.com/image-rs/image-png)。

Rust 实现参照公开格式研究独立编写，没有复制 Niflheim 的未明确授权源文件或专用文件名表。
解密算法与格式依据包括 X'moe 的 NekoPackExtract，以及 morkt 的 GARbro；保留其许可声明如下。
版本和文件链接见 [Sources.md](docs/Sources.md)。本项目不打包这两个工具的二进制。

MIT License

Copyright (c) 2017 X'moe

Copyright (C) 2015-2016 by morkt

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.


视频增量使用 Dominic Szablewski 的 [pl_mpeg](https://github.com/phoboslab/pl_mpeg)，固定 revision `c871f2be022ece7ef4f64230b4fb8e1fb9eb6023`。带有明确标记的三处修正的头文件保存在 `vendor/pl_mpeg/pl_mpeg.h`，文件内保留 `SPDX-License-Identifier: MIT`；上游与本地 SHA-256 见同目录 `UPSTREAM.json`，具体改动见 `NEKOPACK_CHANGES.md`。上游没有单独 LICENSE 文件，分发所需的标准 MIT 许可文本和作者归属保存在 [LICENSE](vendor/pl_mpeg/LICENSE)，并在新的 Android 许可快照中保存为 `pl_mpeg/LICENSE`。预算、I/O 和 C ABI 封装由本仓库另行实现；这不是本项目原创的 MPEG 解码算法。
