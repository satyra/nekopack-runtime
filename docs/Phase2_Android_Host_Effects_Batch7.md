# 第七批：宿主完成事件、Android 音频和淡变

日期：2026-11-06。基线：018e66a78413925c158d6fa54e27f5e71b950933。

## 结论

Android 剧情路径从“记录 effects 并立即确认”改为等待宿主事件。Rust 保留 pending 请求，Android 完成相应动作后携带 token 确认，再恢复同一个 Machine。四部曲各前两页、五个真实音频资源、四次淡变及两种后台中断回归通过。

仍是 **API 28、x86_64 模拟器经 libnb.so 翻译执行 ARM64**。未连接 ARM64 真机，没有把模拟器结果当作真机验收。音频证据为播放器准备、开始、进度和资源哈希；不声称已完成人工听感、扬声器/蓝牙延迟或原版音量混音对照。

## 完成事件契约

- `HostEffect` 包含 token、kind、资源名、duration_ms 和 payload。Rust 核心没有 Android API、计时器或 MediaPlayer 依赖。
- 同一效果的重复 poll 返回相同数据，不重复执行、不消费更多脚本指令预算。只有匹配的 token 能完成 pending 请求；页面序号不能代替效果 token。
- `fade`/`wait` 在计时结束后确认；`bgm`/`se`/`voice` 在异步准备成功并开始播放后确认。音频确认是“已开始”，不是“播放到结束”。语音不会阻塞整段文本，点击下一页会停止旧语音。
- 实际宿主模式遇到非法时间、缺失音频或非 Ogg 输入直接报错并停止，不退回静音跳过。旧 CLI 静态预览仍明确使用无宿主模式，供既有独立像素对照；不是另一套 VM。
- 时间参数按本机已有 Tiny Dungeon 技术研究的秒数 profile 解释，接受非负整数或最多三位小数，转换到毫秒并检查范围。四部曲本次实际覆盖 fade 2、fade 3。未新做原版计时对照；Android `.wait` 尚未由真实游戏路径触发，当前只有核心协议测试及宿主实现。

## Android Adapter

`StoryEffects.kt` 在主 Looper 创建、操作和释放 MediaPlayer，使用异步 prepare，不在 UI 线程阻塞准备。Rust VM 仍只驻留专用工作线程，所有开始、poll、文本确认、效果确认和 close 都由该线程串行处理；Android 回调不会直接重入 VM。

资源仍从 SAF 提供的原始 DAT 由 Rust 解密。原始压缩 Ogg 字节在运行时写入应用私有临时文件交给 MediaPlayer；没有离线解包 fixture 或音频转码。记录 SHA-256 供独立参考比较。正常结束、换音频、点击下一页停止语音及退出会释放播放器并删除对应文件。

BGM 使用循环播放策略，音效和语音各一个通道；同通道替换旧播放器。没有实现 Ogg 循环点元数据、多音效混音、音量设置或淡入淡出音量。标题启动仍是已有 headless 选择 1，本批音频从实际剧情请求开始，未实现可交互标题菜单及其音乐连续性。

淡变采用两层 ImageView 和线性 alpha 动画，目标帧仍由 Rust 合成。记录中间帧以及实际持续时间；即使动画系统提前结束，计时未满足时也不提前确认。第一帧从宿主初始黑底开始，不宣称复现原版标题画面到剧情的完整过渡。

音频焦点使用兼容最低 API 23 的平台接口；编译器保留 deprecated 警告，没有隐藏它。焦点丢失策略为结束本次运行，目前没有验证真实来电、蓝牙和其他应用抢占焦点。

## 生命周期策略

按 Home 进入后台会关闭播放器、取消动画与计时回调，并结束 Activity。在同一 Rust 工作队列销毁会话；关闭后不再接受迟到的完成事件。此阶段不提供退后台继续播放或进度恢复。

`onDestroy` 记录 native close 结果，正常关闭要求 VM 受管 live bytes 回到 0。它不代表全进程 RSS/Java 堆泄漏检测。系统直接杀进程、断电或文件系统错误下的临时文件清扫未验收；本轮验证的是正常退出及 Home 生命周期路径。

## 实测

- 四款共八页的页面元数据及最终 PNG 与上一批桌面静态参考完全一致；文本行核对保留。
- 五个音频请求：BLESS 语音、BIRTH 语音和 SE、BRAVE BGM、BLACK BGM。全部有唯一开始事件、播放器进度或完成记录，运行时字节 SHA-256 与独立 Python DAT 读取器一致。
- 为本次路径增加四个 sound.dat 和前三款 voice.dat，七个设备原始档案整包 SHA-256 与历史原始档案审计一致。BLACK 前两页为旁白，本批未新增其后续语音包覆盖。
- BLESS 3 秒淡变实际确认耗时 3022 ms；其他三款 2 秒淡变分别为 2046、2035、2024 ms。均有 0–1 之间的中间 alpha 帧；计时只证明没有提前确认，不是帧率或原版演出一致性测试。
- Home 中断进行中的淡变：未确认被取消的效果，native close 成功。Home 中断 BGM：播放器释放、临时音频删除，未出现关闭后的完成确认。
- 新增核心测试覆盖重复 poll、错 token、错输入类型、跨文本/效果确认、非法时间、溢出和缺失音频终止。
- workspace 测试、桌面和 ARM64 Clippy（-D warnings）、fmt、Python research 8 项通过。同一 APK 旧入口的四部曲标题/无效选择、32 轮 FFI/预算销毁、409 个分配故障点通过。

证据：

- `research/android-host-effects-verification.json`
- `research/android-host-effects-lifecycle-verification.json`
- `research/android-host-effects-ffi-regression.json`
- `research/android-audio-dat-verification.json`
- `research/story-audio-reference.json`

三个执行报告的 APK 与 native 库哈希一致。没有提交游戏音频、DAT、APK 或私有运行产物。

## 复现与真机下一步

沿用第六批 SAF 目录及 story_game 入口，目录增加原始 sound/voice 档案。构建脚本自动编译 Android src 下所有 Kotlin 文件。

`reference_story_audio.py` 从独立参考读取器生成音频哈希；`verify_story.py` 增加 `--audio-reference` 参数，开启真实音频、时间、中间帧及释放断言。`verify_story_lifecycle.py` 实际发送 Home 测试中断，不直接替宿主确认效果。

建议下一轮连接现有 ARM64 安卓手机：开启 USB 调试，数据线连接并在手机接受电脑授权。先确认 arm64-v8a、系统版本和页面大小，再复跑原始 DAT、四部曲页面、FFI/预算销毁、音频和 Home 中断。模拟器回归继续保留。真机连接说明见 [Android 官方文档](https://developer.android.com/studio/run/device)。

MediaPlayer 线程、状态和异步准备依据 [官方 API 文档](https://developer.android.com/reference/android/media/MediaPlayer)；这只作为实现依据，不替代本地测试结果。
