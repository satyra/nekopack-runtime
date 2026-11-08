# Batch 24：角色左移与位置后接动作

本批实现 BLESS `012702:67` 的 `.img3 SGMY011a right3 <<<<<<<<<`，仍由共用 Rust Runtime 解析、计算和完成每角色动作队列；Android 沿用现有持续取帧接口，无专属游戏分支。资源由 SAF 读取原始 DAT，未改游戏脚本。

## 原版依据与范围

`research/verify_original_leftward.py` 固定 BLESS EXE SHA-256，检查 51 处指令、位置关键字、插值常量、原始 DAT 指令和 message.nut 的绘制循环，见 `leftward-batch24-original.json`。

- `0x43cf94..0x43cf9c` 将字符 `<` 分派到 `0x43cfcf`；`0x43cfd2` 从水平累计值减去计数，`0x43d033..0x43d03b` 乘 50。九个 `<` 是相对左移 450 个逻辑像素。
- 位置解析 `0x43d7cc` 识别 right/left，right3 乘 160 得到中心相对坐标 +480，并设置 absolute 标志。`0x4389b1` 初始化位置时长为 0，位置分支 `0x439776..0x43979d` 入队；下一符号项重新采用默认 250 ms。
- `0x43835f..0x438385` 将绝对目标转换为相对当前坐标的差值；零时长项由 `0x438557` 回到消费路径。之后的移动沿用已确认的 FIFO、默认正弦插值和中心相对坐标截断。
- 此命令因此先定位到屏幕 x=1120，再在默认 250 ms 内左移到 x=670，y=360。移动与对白演出并行。250 ms 是逻辑时长；宿主取帧和显示存在开销，未声称恒定帧率或精确墙钟呈现。

依据是原版静态反汇编、原系统脚本与资源，不是原版实时逐帧画面对照。数值简写、混合符号 token、速度/加速度和显式转场组合未在本批实现，继续明确报错。

## 修改

内部 Rightward 命名改为 Horizontal，重复 `<` 产生负位移；已有正位移元数据不变。允许已支持的绝对位置先于动作 token，复用原有定位、队列和帧计算。位置或资源 token 出现在动作之后仍拒绝，未引入通用任意操作队列。

核心测试增加负坐标截断、负向溢出、左移后右移的队列、重复 poll、完成与提前确认对白；解析测试覆盖 right3 后接九个 `<`，并保留未知参数失败。独立参考增加左移终点、605 页原始命令与每帧重建，不把 Rust 输出当作独立期望值。

## 验收

- Rust workspace 测试、47 项 Runtime 测试、Clippy 全 targets/warnings 为错误、fmt 和 15 项 Python 测试通过；既有 ignored 项不计作通过。
- NDK r27d / Android ARM64 APK SHA-256：`83d90a0db8d8c985386dcc1679807a0ce2adee1e2c194aa9463eea99db5c95fa`。PGKM10 原生 aarch64、native bridge=0。用户告知手机已 root，本批只使用普通 ADB 和应用自身调试接口，没有调用 su、修改系统或运行模拟器。
- 桌面原始 DAT 从标题执行到 620 页，退出码 0；冻结前 606 页作为本批真机范围。前 604 页 JSON/PNG 与上一批桌面探测逐字节相同。599–606 共 8 个新增真机页面由独立 DAT/Pillow 重建图层与像素，全部一致。
- 真机从标题经明确的视频跳过到 606 页，逐页核对 JSON、PNG 和源文本。605 页是 `012702:70` 无语音叙述；606 页是 `012702:82`，执行闪光、SE_024 和 OP_012702_0003，已移除刚才移动的角色，没有残留动作。
- 九个动作页面共 75 帧，其中新左移 8 帧，均匹配独立原始 DAT/Pillow/原版曲线；检查初态、中间态、终态、单调左移及 Rust 完成标志。旧队列与其余动作一起回归。修改后的校验器也重新通过上一批 67 帧。
- 360 次音频请求和 32 个演出快照通过校验，环境音继续要求实际循环。横屏截图留本机；未新增人工听音或原版声波一致性结论。
- 独立取消轮从标题到 605 页，在动作调度后 80 ms 请求切后台，已显示初始帧，随后 cancelled，未 finished；关闭后观察 1 秒无迟到动作事件、无下一页，音频缓存清理，VM live bytes=0。正常结束亦确认 VM live bytes=0。后台行为仍是结束会话，并非存档恢复。

逐帧参考初次检查遇到新叙述行的全角空格：通用 strip() 错误移除了正文首字符。校验器改为仅剥离 ASCII 行缩进后，使用同一批设备捕获重新验证；运行时和 APK 未修改，失败轮次未计作通过。

未提交游戏资源、APK、截图或原始帧。汇总与报告哈希见 `research/leftward-batch24-summary.json`。

## 复现

- `research/verify_original_leftward.py`：原版指令、关键字与原始脚本证据。
- `research/verify_leftward_batch24.py`：验证 CLI 生成的冻结 606 页、604 页旧前缀、左移及后续对白；此脚本不负责生成页面。
- `platforms/android/verify_story.py --until-boundary --allow-page-limit --reuse-continue-bounds --require-env-loop --movie-completion secondary --games 0`：使用冻结 reference；606 是明确页数上限，不是未知指令边界。
- `research/verify_motion_frames.py --include-leftward`：所有九个动作页面逐帧校验。
- `research/verify_story_layers.py --leftward-motion --expected-count 8`：新页面独立终点；加 effects 模式核对演出快照。
- `platforms/android/verify_motion_cancel.py --page 605`：移动期间后台清理。

额外桌面 607–620 页不计入本批真机通过范围。本批未搜索新的最远不支持指令；后续继续从已验收的章节对白推进。
