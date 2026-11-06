# 第十批：全屏图像、wipe、原版 BGM 淡出与 Android 横屏

日期：2026-11-06。基线：`dd803abc79cd61e52c065ef377a906519e4d553e`。

## 结果与范围

同一 Rust Runtime 继续通过 SAF 直接读取原始 DAT。在 PGKM10 真机与原有模拟器上，四部曲分别执行 12、8、72、16 页，共 **108 页/设备**。真机为 Android 15/API 35、aarch64、native bridge=0，原生 ARM64；模拟器为 Android 9/API 28、x86_64/libnb，明确记录为 ARM64 翻译执行。

Android 宿主改用 sensorLandscape，支持两种横屏方向；继续/结束按钮并排，文本区域最多三行，画面保持 FIT_CENTER 和逻辑宽高比。实际内容区域：真机 2184×984、模拟器 1280×684，四款运行均断言宽大于高、orientation=landscape、画面区域非空。当前仍为验证宿主，不是完整游戏 UI；超长文本的滚动/分页尚未实现，本批最长文本 71 字符。

## 原版行为的确认

只读分析四款本地原版 EXE，未改写或分发原程序。证据保留文件 SHA-256、地址、参数赋值和函数区间哈希；`verify_original_effects.py` 可对用户持有的原程序复核。

- 四款的 `fade` 参数分支都把默认值设为 `0xFA0`，即 4000 ms。BLESS/BIRTH 的赋值地址为 0x444b9f，BRAVE 为 0x4a67d8，BLACK 为 0x444aeb。
- BRAVE 执行路径 0x4afad0 调用 0x59d528，音频状态保存旧值、目标、时长并重置计时后返回；该分支跳过立即停止，不等待淡出时长。由此修正上一批把显式 BGM 淡出作为串行等待的行为。
- 硬边 wipe 的像素内核：`mask + phase >= 255` 时取目标，否则取原画面；phase 为 `elapsed_ms × 256 / duration_ms`。0 阶段取原图，达到 255 阶段取目标。
- `wipe ... fade` 的柔边内核：`alpha = clamp(mask + 2×phase − 255, 0, 255)`；每通道为 `old + floor((target−old)×alpha/256)`。硬边 120 字节及柔边 232 字节函数在四款原程序中逐字节相同，报告只保留哈希，不保留代码字节。
- wipe 的数字参数是毫秒，省略或非正值的原版默认值为 500 ms。当前运行器只接受非负数字，0 使用默认值。
- 全屏 EF 位于 image/visual；同名 wipe 遮罩位于 image/mask。两者原始资源哈希和内容不同。

上述是原版静态证据，不是原版窗口录屏或音频波形对比；具体音量映射、非整数 BGM 参数等没有扩大为完整兼容性承诺。

## 实现与状态边界

- Scene 增加全屏 visual 图层，从原始 image/visual/*.png 加载，与普通角色槽、背景、纯色覆盖分别管理。本批限定为逻辑画布同尺寸、不透明、零偏移的静态 PNG；未确认的透明/尺寸/偏移情况明确报错。
- wipe 算法放在 Rust transition 模块；旧显示画面与待显示场景分离。遮罩要求同尺寸、不透明灰度图。新增原始 RGBA 构造器检查维度、长度和预算；wipe 的旧图/目标/遮罩驻留像素合计上限为 64 MiB，字段私有以维持构造约束。
- Android 在同一 VM 工作线程请求带 token、经过时间的纯渲染帧。轮询不推进 PC，不重复执行脚本；只有匹配效果确认才释放等待请求。UI 线程只接收数据、创建 Bitmap、显示和计时。关闭时阻止迟到帧显示，并在工作线程销毁 VM。
- BGM 淡出在启动时确认脚本，音频动画独立于画面动画。默认 4 秒；新 BGM 会取消旧淡出，旧回调不能释放新播放器；退出取消尚未完成的动画。
- BLACK 的 CG 后使用无参 `.fade`，因此补齐必要的 500 ms 默认淡变。正数按原版整数秒解释，小数截断；0/负数使用默认值。wait 的原有时间规则没有随之改变。
- 图像驻留限制、wipe 缓冲限制、VM 受管内存计数是不同范围；不把它们当作整个 Android 进程的总内存限制或泄漏证明。

## 真机发现的性能问题

首次 100 ms EF04 wipe 用逐帧 PNG 传输，只显示起点和终点；测试因缺少中间帧明确失败，保留在 initial-wipe-observation 报告中。

改为 Rust 输出原始 RGBA8，JNI 传递字节，Android 显式转换 ARGB 像素后创建 Bitmap，移除逐帧 PNG 编码/解码。最终真机显示阶段 0/56/85/100 ms，125 ms 后确认；模拟器显示阶段 0/51/100 ms，127 ms 后确认。每个阶段的 RGBA 均与独立 NumPy 整数算法及原始遮罩一致。这里保留实际耗时，不声称精确 100 ms 呈现或稳定 60 FPS；软件帧生成、传输、文件观测和显示仍有延迟。

## 实际执行边界

- BLESS：12 页；story1:10 的 movie 未实现，深度 1、196 步，保持停止。
- BIRTH：8 页；EF04 图像及 100 ms wipe 已执行，在 012401:58 的 shake 停止，深度 2、189 步。虽然页数不增加，但新增转场已经在 Android 呈现并校验。
- BRAVE：72 页；通过无参 BGM 淡出，返回后进入下一剧情源 011001；在其第 137 行 shake 停止，深度 2、484 步。
- BLACK：16 页；全屏 VGVE_00d 与无参淡变已显示，在 P01:74 的 shake 停止，深度 2、198 步。最终显示的 CG 像素另与原始 DAT 独立读取结果比较，不依赖新文本页出现。

没有跳过 shake/movie；没有增加视频、震屏、完整 MNG、存档恢复或桌面宿主。下一批优先核对并实现实际遇到的 shake，视频仍独立评估。

## 验证结果

- 108 页静态图层及最终像素通过独立 DAT 读取器、脚本重放与 Pillow 比较；两台设备的页元数据、源文本行、PNG 与桌面对照一致，四个边界检查点也一致。
- 两台设备各 49 个音频请求（36 语音、7 BGM、6 SE）通过原始字节哈希比较。正位置、达到时长后的完成、终止段中尚未推进就被释放的 SE 分开记录。SE_029、SE_024 属于最后一种，不算播放完成。
- 两台设备各 11 次画面淡变、1 次 wipe、2 次 BGM 淡出。BGM 1 秒淡出在启动后立即确认，约 1029 ms 完成；默认 4 秒淡出与画面演出并行，并在下一首 BGM 到来时正确取消。不能把这次取消写成“已播放满 4 秒”。
- 三种 Home 回归：画面淡变中、BGM 播放中、BGM 淡出中。均清理播放器/临时 Ogg、结束 VM；被取消的画面效果没有迟到确认，音频淡出没有迟到完成。未单独验收 100 ms wipe 进行中的 Home 中断。
- 真机及翻译执行分别重跑 32 轮 FFI/预算失败销毁、238+74+97=409 个分配故障点及四部曲标题/无效选择；受管 VM live bytes 回到 0。此入口仍是既有 fixture 探针，与真实 DAT 剧情路径分开记录。
- Rust workspace 测试、桌面及 Android ARM64 Clippy、9 项 Python 研究单元测试通过。新增测试覆盖默认淡变、4 秒 BGM 请求、wipe 整数端点/柔边舍入、错误 token/重复帧不推进及 RGBA 长度校验。
- 六份最终设备报告使用同一 APK/native 库哈希。最终仅收紧 Wipe 字段可见性后重新交叉编译，原生库逐字节不变，确认安装包仍对应最终执行代码。

最终 APK SHA-256：`6a404bcb893e56d47c25c14a1b716ffeee7e96854ebba1c5a94379e4e02434ad`。原 NDK r27d/C++ shared 库策略沿用，实际设备均为 4 KiB 页面；不扩大为 16 KiB 真机验收。

## 证据与复现

- `research/original-effects-batch10-evidence.json`：原版默认值、异步执行路径及 wipe 内核哈希；使用 `verify_original_effects.py --games ORIGINAL_GAMES` 复核。
- `research/story-batch10-desktop-verification.json`、`story-batch10-pixel-verification.json`：执行状态和独立静态像素。
- `research/story-batch10-audio-reference.json`、`story-batch10-resource-reference.json`：独立音频及效果资源哈希。
- `research/story-batch10-arm64-verification.json`、`story-batch10-translated-verification.json`：完整剧情、音频、横屏、实际中间帧与最终 CG 校验。
- 同前缀的 `*-lifecycle-verification.json`、`*-ffi-verification.json`：生命周期与故障回归。

先用 `verify_story_boundaries.py --profile batch10` 生成空目录中的桌面页，再执行 `verify_story_layers.py`。使用 `prepare_effect_reference.py` 从原始 DAT 准备本地 wipe/CG 对照，再运行：

```text
python platforms/android/verify_story.py --adb ADB --serial DEVICE --desktop PAGES --fixtures SOURCE_REFERENCE --audio-reference AUDIO_REPORT --wipe-reference EFFECT_REFERENCE --until-boundary --output DEVICE_REPORT
```

旧批次通过报告保留不改。原始 EXE/DAT、对话全文、图片、RGBA 帧、音频、APK 与签名材料不进入仓库。标题仍为已有 headless 系统桥选择开始，并非原版标题菜单 UI；后台仍结束会话，不是暂停恢复；完整游戏和原版逐帧视听一致性未验收。
