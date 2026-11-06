# 第 12 批：Android flash 执行链路

本批将四部曲路径中的 `.flash` 接入同一套 Rust Runtime 和 Android 横屏宿主。实际剧情继续通过 SAF 读取原始 DAT；标题开始选择仍使用已有系统桥，宿主尚不是完整原版菜单或文本窗口。

## 原版依据和支持范围

`research/verify_original_flash.py` 固定四个原版 EXE 的 SHA-256，核对默认值、颜色表、次数翻倍、交替分支和 30 ms 等待参数。结果保存在 `original-flash-batch12-verification.json`，不提交原始 EXE 或代码字节。

- 数字参数是闪烁次数，不是秒数；默认白色、一次。颜色和次数可交换顺序。
- 支持 `white`、白、赤、青、黄、緑、紫、水。每次依次显示纯色、恢复场景快照；正次数的末相位恢复场景。
- BRAVE 解析入口 `0x4958bc`，循环 `0x49872a`，等待参数 `0x498786`；BLESS/BIRTH 对应 `0x441eac`、`0x4423ac`、`0x442408`；BLACK 对应 `0x441e04`、`0x4422f4`、`0x442359`。
- BRAVE 的颜色填充调用位于 `0x498772`，快照拷贝调用位于 `0x498762`。其他三作分别在其相应循环中交替填充和拷贝。

以上是静态原版代码证据，不是原版逐帧截图对照。原版等待函数处理窗口消息，实际节奏可能被消息影响；输入中断分支也存在版本差异。本批不声称严格时间或输入中断兼容。

当前明确限定次数为 **1..=128**，这是运行器的工作量限制，不是原版上限。原版零/负次数的循环行为尚未纳入兼容范围，运行器拒绝这些参数；未知选项同样报错。场景快照必须不透明，Android 输出 alpha=255；原版 DIB 的高字节不等同于 Android 显示透明度。

本批实际剧情路径包含 BIRTH 的五次蓝色单次闪屏、两次蓝色五次闪屏，以及 BLACK 的两次白色单次闪屏（显式 `white`，次数默认），共九个请求、34 个相位。BLESS 和 BRAVE 在本批有界路径中没有 flash。参数单元测试直接覆盖蓝色和默认白色；其他颜色目前依据原版静态核对和解析实现，不列为设备剧情覆盖。

## 实现和状态约束

Rust 解析参数、保存快照、生成纯色/恢复 RGBA 帧，Android 只负责调度和显示。闪屏使用独立的离散相位入口，不用经过时间推算相位，以免慢设备跳过短闪屏。

每个相位之前等待至少 30 ms；上一帧提交后再安排下一次等待。名义总时长为 `次数 × 60 ms`，JNI 传输、Bitmap 创建和验证文件导出增加实际耗时，因此这不是性能或原版帧率验收。

请求必须持有当前 token，相位必须连续；只允许重复最近一次相位，不能跳跃或倒退。重复读取不会推进脚本 PC。全部相位完成前拒绝确认，确认后释放闪屏状态并恢复场景快照。退出时沿用 onPause 停止宿主任务、worker 销毁 VM 的路径。

两份保留像素缓冲区合计最多 64 MiB；这不是进程总内存上限，PNG 包、场景资源、JNI 数据和 Android Bitmap 另占内存。

## 验证设计

- Rust 新测试检查颜色/次数解析、默认值、非法次数、交替颜色与最终恢复、连续相位、重复请求、错误 token、提前确认、PC 稳定及确认后的失效。
- 设备测试逐相位导出 RGBA，独立按原始脚本行核对颜色/次数，并检查完整相位序列、每相位等待、纯色像素和恢复像素。场景快照另由独立 DAT 解码器和 Pillow 校验，避免仅以运行器自己的输出作为依据。
- 五项生命周期测试覆盖 fade、BGM、BGM fade、shake、flash。flash 使用仅探针启用的启动参数，在真实请求开始后 5 ms 请求进入后台；不修改脚本时长。真机实际在 41 ms、模拟器在 30 ms 进入 onPause，两者均未确认被取消的 flash，销毁完成、播放器释放、无迟到帧或确认。
- FFI 专项在两个设备分别运行 32 次预算失败/销毁循环、409 个分配失败注入点及四部曲标题选择/非法选择检查，受管内存最终归零。此专项使用小型 fixture 隔离错误路径，不作为 SAF 原始 DAT 剧情证据。

工作区 Rust 测试通过，其中 Runtime 为 22 项；Python 研究测试 11 项通过。`cargo fmt --all -- --check`、桌面工作区及 Android ARM64 Clippy（`-D warnings`）通过。桌面 278 张页图全部通过独立 DAT/Pillow 对照。

## 执行边界

最终桌面原始 DAT 路径共 278 页：BLESS 12 页后停在 `story1:10` 的 movie；BIRTH 97 页后停在 `012401:615` 的无参 `.bgm`；BRAVE 达到 120 页上限，位置 `011002:19`；BLACK 49 页后停在 `P01:229` 的无参 `.bgm`。上限不是剧情结束。探索与最终桌面输出的 562 个文件逐字节相同。

无参 `.bgm` 与已有 `.bgm fade` 是不同语法。本批不猜测停止/淡出行为，保持明确错误边界。后续先核对这两个路径的原版处理，再接通音频停止链路；BLESS 的 movie 仍是独立阻塞项。存档、后台恢复和完整 MNG 播放仍未由本批实现。

## 最终设备证据

- 真机为原生 ARM64；模拟器为 x86_64 经 libnb 翻译执行 ARM64。两者分别通过 278 页、9 次 flash / 34 个相位、21 次 wipe、19 次 shake 和 134 个音频请求，横屏检查通过，每个会话结束时 VM 受管内存归零。
- 真机核对 154 张 shake 帧、97 张 wipe 帧；模拟器分别为 115、68 张。全部 34 个 flash 相位逐个匹配源脚本参数和期望像素，两台均无跳相位或提前确认。
- 每台 82 份视觉请求快照通过独立原始 DAT/Pillow 场景及像素对照。两台的 164 个捕获文件（PNG 和 checkpoint）逐字节一致；快照报告记录输入文件哈希、实际运行 ID 和 APK 哈希，且核对请求集合完整无遗漏。
- 两台使用同一 APK，SHA-256 为 `f840e8f8b76ade1707098e667762a2ff009d51aab55f2cc6924fc23c343cbcd6`。APK 内原生库与本地 ARM64 构建产物相同，SHA-256 为 `523ac09eea2ea22bd5f6827371bcbc3c6c2431b244bb304aca6e19a995e52752`。
- flash 完成较名义时长最大延后真机 205 ms、模拟器 220 ms；shake 分别 60/73 ms，wipe 分别 73/70 ms。探针同步导出验证数据，上述结果只验证相位顺序和完成时序，不证明原版节奏或生产性能一致。
- 音频核对原始 DAT 哈希、播放器进度/完成状态及清理，本批没有新增人工听音结论。测试期间 UI 自动化曾短暂返回空控件树，后续实际控件定位成功；没有跳过页面、帧或音频断言。

报告为 `research/story-batch12-{arm64,translated}-verification.json` 以及同前缀的 `*-ffi-verification.json`、`*-lifecycle-verification.json`、`*-snapshot-verification.json`。桌面、页像素和音频对照分别为 `story-batch12-desktop-verification.json`、`story-batch12-pixel-verification.json`、`story-batch12-audio-reference.json`。

## 复现

先用 `verify_story_boundaries.py --profile batch12` 向空目录生成桌面页，再用 `verify_story_layers.py` 和 `reference_story_audio.py` 生成独立图像/音频对照。设备运行 `verify_probe.py`、`verify_story_lifecycle.py`、`verify_story.py --until-boundary --allow-page-limit --effect-captures LOCAL_DIR`，最后对捕获目录运行 `verify_story_layers.py --effects`。

运行器设备入口始终读取 SAF 原始 DAT。研究工具使用的独立解码器、UTF-8 脚本参考和本地 mask 仅用于对照，不是设备运行资源。报告与源码提交，游戏资源、音视频、APK 和捕获帧保留在本地。
