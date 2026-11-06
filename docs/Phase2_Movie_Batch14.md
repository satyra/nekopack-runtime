# NekoPack movie 增量（Batch 14）

## 范围与实际边界

本批接入 `.movie` 的等待、Android 原始视频播放、自然完成和显式跳过后的恢复。Android 仍为首要交付平台；宿主是现有横屏验证壳，不是完整游戏应用。没有加入 Windows 播放宿主，没有更改 AdvHD 模块。

BLESS/BLACK 的真实脚本在 `story1:10` 调用 `op.mpg`，恢复后先执行空行，再在 `story1:12` 遇到尚未实现的 `.route`。该命令保持显式失败，不伪造下一章已可运行。BIRTH/BRAVE 的 120 页回归范围没有到达 movie，其四部曲视频覆盖来自独立 SAF 宿主验证，不能混称为四部都从剧情进入电影。

## 原版依据

`research/verify_original_movie.py` 检查四个本机 EXE 的 SHA-256、解析器中的 `skip` 字符串及默认空字符串、条件分支、事件映射和播放结束后的清理调用。结果见 `research/original-movie-batch14-verification.json`。这是原版静态反汇编依据，不是原版播放时长或声音的动态捕获。

- 进入 movie 时先停止四个声音通道。
- 第二参数恰好为 `skip` 时，左键事件 `0xff01` 可以结束播放；未给该参数时不检查此退出条件。
- 右键事件 `0xff03` 不受上述标志限制，可以结束播放。
- 播放器状态结束或消息循环退出后，先清理播放器，再返回脚本解释器。没有据此推断 Escape 的跳过语义。
- 原版 `Movie` 配置可以禁用播放。本批验证配置固定为启用；尚未接入该配置开关。

Android 的画面点击对应主输入，右上角“返回剧情”按钮对应原版右键退出。最初“跳过视频”标签在真机上被快速自动点击；用户确认没有操作，设备启用了 GKD，改名后干扰消失。未修改用户的无障碍设置。

## 原始资源与解码器

四部 `op.mpg` 是安装目录内的散文件，不在 DAT 中，格式均为 MPEG-PS / MPEG-1 Video / MP2 Audio。BLACK 同时有其他视频文件，严格读取脚本指定的 `op.mpg`，不按文件体积或名字猜替代资源。

通过已有 SAF 目录授权列举同一游戏目录，按不区分大小写的唯一文件名定位视频；Rust 从借用 FD 复制取得拥有权的 `File`，以 `Read + Seek` 流式读取。没有预解包电影、转码、外部播放器或 Wine。设备原文件 SHA-256 与本机原文件一致，见 `movie-batch14-upload.json`；宿主另外从实际 SAF 输入计算哈希。

Android 的[平台格式列表](https://developer.android.com/media/platform/supported-formats)不保证这些 MPEG-1/MP2/PS 组合。新增 `nekopack-video` 使用 MIT 许可的 [pl_mpeg](https://github.com/phoboslab/pl_mpeg)，固定到 `c871f2be022ece7ef4f64230b4fb8e1fb9eb6023`。上游头文件保留作者与许可，并明确标记末帧、MP2 量化和归一化三处修正，详见 `vendor/pl_mpeg/NEKOPACK_CHANGES.md`；许可、作者归属与 APK 分发声明见 `THIRD_PARTY_NOTICES.md`。FFmpeg 只作本地独立对照，不链接到 APK。

## 执行和所有权

Rust `StorySession` 为每个 movie 生成带 token、逻辑名称和主输入跳过标志的请求。重复 poll 不推进 PC，普通 effect ACK 拒绝确认 movie；专用 ACK 只接受自然完成、合法主输入跳过或次输入跳过。错误 token、重复确认、非法原因均不会跳过后续指令。headless 只记录 movie 并继续控制流，不进行视频播放。

Android 先释放现有声音通道，再启动独立视频工作线程。剧情 VM 仍由原来的独立 VM 线程持有。视频线程拥有解码器、AudioTrack 和帧提交；不会在 JNI 之间保存 Java 数组指针。PCM 使用短流式缓冲区；自然结束等待已写 PCM 的播放头耗尽，关闭 AudioTrack、解码器后才回调 VM。横屏画面按视频比例适配 Surface，播放期间保持屏幕唤醒，移除 Surface 后释放该请求；不把输出尺寸写入核心。

主输入禁用时不会确认请求。后台切换、Surface 丢失、焦点丢失和错误都不是自然完成；退出会话不会发送继续剧情的 ACK。关闭标志抑制迟到回调；Surface 销毁与提交画布同步，末尾取消不再进入自然结束完整性检查。

原生 decoder 的 boxed reader 地址在同步 C 回调期间稳定，decoder 不实现 Send/Sync。Rust I/O panic 在回调内捕获，C++ 分配与 I/O 失败在 C ABI 内捕获；部分初始化也由 arena 回收。panic payload 沿用现有桥的隔离策略，遗忘可能具有抛出式 Drop 的 payload；内存计数不覆盖该 Rust panic payload 或进程全部分配。

支持范围限制为一个视频流、一个 MP2 音频流、偶数尺寸且不超过 1920×1080、1–60 fps、32/44.1/48 kHz、最长 600 秒和文件不超过 512 MiB。C++ 受管数据预算 64 MiB，初始化及每步 I/O 有额度，解码步数有总上限。报告中的 peak/live 是 C++ decoder 数据，不包含分配头、Rust/JNI/Bitmap/AudioTrack 或系统 GPU 内存。没有声称完成恶意 MPEG 模糊测试、硬实时解码或远程 SAF 提供方验证。

## 验证记录

完整 301 页回归在解码器三处修正前的 APK 完成，包含四部曲的原始 DAT、声音、演出、影片返回与 VM 清理；修正只涉及视频解码器。最终 APK 重新验证四部原影片、四部标题 FFI、BLESS/BLACK 两条实际 movie 剧情链路、跳过与后台清理。两阶段构建哈希分别保留，不声称所有结果来自同一 APK。

最终设备结果见同批 `story-batch14-*` 与 `movie-batch14-*` 报告。真机为 PGKM10，Android 15，原生 ARM64；模拟器为 Android 9、x86_64、libnb 翻译执行 ARM64，不能作为 ARM64 真机的替代。

单元测试覆盖 movie 等待、PC 稳定、主输入限制、token、重复确认、连续 movie 以及后续未知指令停机。decoder 测试覆盖错误输入与回调 panic；四个真实视频另执行 6 档分配预算、4 个早/晚 I/O 错误或 panic 场景，以及 16 次提前销毁，C++ 受管 live bytes 均归零。较小 BRAVE 视频在 8 MiB 档可以成功，未把它强行记为预算失败。

设备独立宿主完整解码四部原始视频，并验证帧数、PCM 样本数、播放头、自然结束、源文件哈希和与桌面相同的第 0/24/240 帧。默认主输入忽略、显式 `skip` 主输入退出、次输入退出及后台不继续分别验证。脚本回归仍校验原始 DAT、文本位置、逐页状态、图像、声音和演出请求；movie 返回后继续到真实未知命令边界。

初版模拟器在 Android `Bitmap_copyPixelsFromBuffer` 的堆缓冲区释放路径崩溃；改为独立 direct buffer 后重新验证。软件画布明显慢，API 26+ 改用硬件画布；API 23–25 的软件路径未在本批设备覆盖。BIRTH 的高码率 720p/29.97 fps 在翻译模拟器上仍可能慢于片长，应看各次实测 elapsed，而不能把完整解码等同恒定实时播放。

## 尚未达到的保真程度

`research/verify_movie_decoder.py` 对照独立 FFmpeg 的三个采样画面及前 3 秒 PCM，报告为测量结果，不是“与原版相同”的通过标签。采样 RGB 平均绝对差不超过 1/255，单分量最大差可到 20/255。初版 BIRTH 少一帧，PCM 采样 RMS 差约 120–554（16-bit 量纲）；初始差异保存在 `movie-batch14-initial-{counts,decoder-differences}.json`。修正 EOF 参考帧释放、量化级数除数及 Q30 归一化后，四部全片帧数与音频样本数均与独立 FFmpeg 一致，整段 PCM RMS 差约 1.78–2.45、最大差 12–17，相对增益约 0.99995。完整结果与显式回归容差见 `movie-batch14-independent-counts.json`；这仍不是逐样本相等，也不是原版播放器对照；没有新增人工听音结论，也没有测量物理屏幕/扬声器的精确同步。

下一批优先确认 `.route` 的原版状态与存档语义，再推进 movie 后的章节调用；继续保留视频 RGB 与 PCM 的剩余舍入差异，不把现有容差验证宣称为原版逐像素/逐采样保真。不要把 `.route` 当成无操作绕过，不扩展无关平台或完整播放器功能。

## 复现入口

- `verify_original_movie.py`：原版指令与输入映射，需用户自有 EXE。
- `cargo test --workspace`，`cargo clippy -p nekopack-video -p nekopack-runtime --all-targets -- -D warnings`。
- `nekopack-video` 的 `decode` 与 `failure_check` examples：私有原 MPG 的解码、预算/回调/销毁检查。
- `verify_movie_decoder.py`、`verify_movie_complete.py`：独立 FFmpeg 采样和全片对照，保留实际差异；`verify_movie_vendor.py` 校验第三方改动范围。
- `verify_story_boundaries.py --profile batch14`、`reference_story_audio.py`、`verify_story_layers.py`：桌面与独立资源依据。
- Android `build_probe.py`、`verify_probe.py`、`verify_movie.py`、`verify_movie_controls.py`、`verify_movie_story_cancel.py`。
- Android `verify_story.py --until-boundary --allow-page-limit --movie-completion natural`；movie 跳过链路使用 `--movie-completion secondary`。

所有原始游戏资源、解码帧、音频、屏幕截图和 APK 留在本地。仓库仅提交代码、许可、元数据与验证结果。

## 最终收尾结果

最终 APK SHA-256：`7d97ce202ad9a20ab7664623b6cb443bf76f473df2eccba0e0578e0f9ca41e03`。`research/movie-batch14-final-verification.json` 交叉检查构建哈希、输入报告哈希、脚本边界、图像、PCM 和清理结果。

两台设备分别完成：阶段构建四部曲 301 页及 85 张独立静态图像验证；最终构建四部完整影片、四部标题 FFI、BLESS/BLACK 共 61 页至电影自然返回、BLESS 12 页至主动退出电影、实际剧情电影切后台释放，以及 BIRTH/BRAVE 各 2 页原始 DAT 冒烟检查。后台取消无后续剧情 ACK，受管 VM 内存归零。最终构建自然返回与主动退出均停在同一 `.route` 边界。

四部影片完整播放耗时：真机约 104.5、140.5、116.2、136.6 秒；翻译模拟器约 104.8、165.4、117.5、137.7 秒。尤其 BIRTH 的模拟器结果不代表实时性能达标。整片帧数分别为 2497、4196、3472、4081；每通道 PCM 样本数分别为 4590720、6720768、5556096、5999616。

最终源码通过 workspace 单元测试、workspace/all-targets Clippy（警告视为错误）、格式检查及 14 项 Python 测试。后台验证器最初错误假定 movie 元数据带有 checkpoint；修正为检查实际请求字段后，两台设备重新执行通过。等待 PC 的正确性由核心单元测试覆盖，设备报告不声称采集了不存在的 movie checkpoint。
