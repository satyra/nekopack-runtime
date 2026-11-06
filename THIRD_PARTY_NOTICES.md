# 格式与算法来源

像素增量新增 Cargo 依赖 `png 0.18.1`（MIT OR Apache-2.0），通过其公开 API 解码/编码 PNG。
实际构建依赖还包含 bitflags、crc32fast、cfg-if、fdeflate、flate2、miniz_oxide、adler2、simd-adler32；具体版本由 Cargo.lock 固定。
其中 simd-adler32 为 MIT，其他均提供 MIT 许可选项。依赖源码与完整许可由 Cargo 的对应 crate 包提供；本仓库不复制这些依赖源码或发布编译后的游戏资源。
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
