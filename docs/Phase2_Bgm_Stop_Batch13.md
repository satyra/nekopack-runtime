# 第 13 批：无参 `.bgm` 原版行为与 Android 执行

本批只接通无参 `.bgm`，继续使用 Rust 核心、Android 横屏宿主和 SAF 原始 DAT。没有实现 movie、存档或后台恢复，也没有扩展其他声音选项。

## 原版证据

四部曲的 `SoundCommand` 在没有参数时保留空资源名、未指定音量/声像修改标志。执行器遇到空名称后检查两个修改标志；两者均为假时调用当前声音通道的句柄释放函数，随后清空名称记录。这条路径不进入 `fade` 的时长调度，不能解释为默认四秒淡出。

- BLESS/BIRTH：解析入口 `0x44487c`；空名称分支 `0x445fe2 -> 0x446598`；停止调用 `0x446610 -> 0x4c03ac`；句柄清空 `0x4c0405`。
- BRAVE：解析入口 `0x4a5d24`；空名称分支 `0x4af181 -> 0x4afab0`；停止调用 `0x4afb4a -> 0x59d3e4`；句柄清空 `0x59d437`。
- BLACK：解析入口 `0x4447c8`；空名称分支 `0x445f2e -> 0x4464e4`；停止调用 `0x44655c -> 0x4bf4e4`；句柄清空 `0x4bf53d`。

`research/verify_original_bgm_stop.py` 固定四个 EXE 的 SHA-256，断言默认修改标志、空名跳转、修改标志分支、释放调用、引用计数递减及清空句柄。`original-bgm-stop-batch13-verification.json` 保存位置和代码段哈希，不包含原版字节。

这是原版静态控制流证据，不是实时声音采集。原版使用共享所有权，最终析构仍取决于是否存在其他引用。Android 取消自己的淡出动画是宿主适配要求，不宣称原版使用同样的定时器结构。各作地址和通道布局分别核对，没有假定结构完全相同。

## 实现

Rust 将无参 `.bgm` 转换为独立的 `bgm_stop` 宿主请求，时长为零、名称与 payload 为空，不读取 Ogg 资源。请求等待当前 token 的宿主确认后才推进脚本；重复 poll 不改变 PC，错误或过期 token 被拒绝。Headless 记录停止效果后继续。

Android 使用现有 BGM 释放路径取消该通道的淡出动画、释放播放器并删除其临时音频文件，再发送完成确认。只移除 BGM，不清空 SE 或语音通道。已经没有 BGM 时也可完成停止；本批设备剧情实际覆盖的是正在播放 BGM 的停止及 BIRTH 后续重新播放。

当前 Runtime 单元测试覆盖无资源停止、连续两个停止、pending/PC 稳定、错误及旧 token、确认后进入文本。无播放器重复停止和“停止打断淡出”的组合没有作为独立设备场景注入，不能从正常剧情回归声称这两项已专项验证；现有后台淡出取消测试另行保留。

## 回归范围与验收

桌面原始 DAT 共 301 页：BLESS 12 页后在 `story1:10` 停于 movie；BIRTH 到 120 页上限，`012401:711`、深度 2、steps 842；BRAVE 到 120 页上限，`011002:19`、深度 2、steps 688；BLACK 49 页后越过 `.bgm`，在 `story1:10`、深度 1、steps 362 停于 movie。BIRTH/BRAVE 上限不是剧情结束。

探索与最终桌面输出的 606 个文件逐字节一致。工作区 Rust 测试通过，其中 Runtime 23 项；Python 研究测试 14 项通过。桌面工作区与 Android ARM64 Clippy（`-D warnings`）通过。

新增设备验证要求停止请求数量与逐页/边界效果记录相同，每个请求的停止记录恰好一次、BGM 数量为零、释放记录严格位于 begin 与 ack 之间；停止后的旧 BGM 不得再报告播放进度或完成事件。Python 反例测试拒绝缺失/重复释放、同毫秒内错误顺序、残留播放器以及迟到播放事件，同时允许其他通道继续报告进度。

FFI 专项继续使用小型 fixture 隔离预算及分配失败，不作为 SAF 剧情证据。五项后台专项沿用 fade、BGM、BGM fade、shake、flash 的实际 onPause/onDestroy 清理检查。

## 首次真机验证失败与修正

首次真机运行在 BIRTH 第 98 页的 `wait_audio` 超时：SE token 122 的时长为 1098 ms，播放开始后 265 ms 被同通道 token 124 替换。旧播放器只报告过 position=0，尚未自然结束；新 SE 正常报告进度并完成。旧验证器要求全部历史音频都产生非零进度或自然完成，没有表示合法的提前替换。

`story-batch13-initial-audio-observation.json` 保留该失败的 APK、run ID 和相关完整事件，未记为通过。验证器新增严格的替换证据：旧音频确实启动、同通道新请求发生在旧时长结束前、新播放器成功启动、旧音频没有完成或迟到播放事件。报告以 `replaced_by_token` 单独记录，不伪造进度/自然完成。反例测试拒绝仅请求未启动、跨通道、过时替换、零时长和迟到事件。运行器/APK 未修改，真机完整从头重跑；模拟器原有严格进度检查继续执行。最终两台报告均经最新证据规则复核，结果见 `story-batch13-audio-evidence-verification.json`，包含输入报告与检查器的 LF/UTF-8 规范化哈希。

## 最终设备结果

- 真机原生 ARM64；模拟器为 x86_64 经 libnb 翻译执行 ARM64。两台分别通过 301 页、145 个音频资源请求、2 次无参 BGM 停止、9 次 flash、21 次 wipe、19 次 shake。横屏与每个会话结束后 VM 受管内存归零检查通过。
- BIRTH/BLACK 的停止确认耗时分别为真机 80/22 ms、模拟器 12/10 ms。先释放 BGM 再确认继续，旧 BGM 无迟到播放事件；BIRTH 的后续新 BGM 正常启动。以上是宿主调用耗时，不是原版逐毫秒或物理静音时刻对照。
- 最终真机运行再次出现 SE 122 只有 position=0 然后被 SE 124 替换，按实际中断证据通过；模拟器该 SE 有正常进度。两台的逐页状态与全部 flash 相位哈希一致，没有将音频进度差异伪装为完全相同的设备表现。
- 每台 85 份视觉请求快照通过独立 DAT/Pillow 场景与像素校验，两台的 170 个捕获文件逐字节一致。快照报告包含完整请求集合、输入哈希、运行 ID 和 APK 哈希。所有音频请求都有实际进度、自然完成或明确替换证据，本批没有仅依赖末尾观察窗口豁免的请求。
- 六份剧情、FFI 和生命周期报告对应同一 APK：`4c4f9ab1cdb11800821a7c80c6cb0ee684d9abbc46ad6115fbbba268f41db8ef`。本地 ARM64 构建与 APK 内原生库相同：`474f5b99c7db3deb2283f53381f279017e072e8e2e8d3a674b8d259b25f3ccb6`。
- FFI 每台通过 32 次预算失败/销毁循环、409 个分配失败注入点及四部曲标题选择检查；后台五项专项通过。所有 301 张桌面页图通过独立 DAT/Pillow 校验。没有新增人工听音结论。

设备报告为 `research/story-batch13-{arm64,translated}-verification.json` 及同前缀的 `*-ffi-verification.json`、`*-lifecycle-verification.json`、`*-snapshot-verification.json`；桌面、页像素、音频资源参考及最终音频证据均使用 `story-batch13-` 前缀。

## 后续与复现

下一步的共同阻塞项是 BLESS/BLACK 的 movie；应先确认资源格式和播放/跳过语义，再接入 Android。BIRTH 已验证停止后的重新播放路径，继续扩展剧情仍需明确页数预算及遇到的未知命令。

桌面使用 `verify_story_boundaries.py --profile batch13` 输出至空目录；独立图像/音频使用 `verify_story_layers.py`、`reference_story_audio.py`。设备依次运行 `verify_probe.py`、`verify_story_lifecycle.py`、`verify_story.py --until-boundary --allow-page-limit --effect-captures LOCAL_DIR`，然后以 `verify_story_layers.py --effects` 校验全部视觉快照。`verify_story_audio_evidence.py --reports REPORTS --output OUTPUT` 可复核完整报告的音频证据。游戏资源、APK 和捕获帧只保存在本地。
