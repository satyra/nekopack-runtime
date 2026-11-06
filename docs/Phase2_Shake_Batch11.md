# 第 11 批：Android shake 执行链路

本批将上一轮的参数模型接入 Rust `StorySession` 和 Android 横屏验证宿主。继续从 SAF 读取原始 DAT，没有新增预解包运行资源，没有推进视频或 flash 实现。标题仍由已有系统桥选择开始，当前宿主不是完整的原版菜单/文本窗口实现。

## 原版依据及兼容边界

`research/original-shake-rendering-batch11.json` 和 `verify_original_shake_rendering.py` 保存可复验的原版地址、指令断言与代码段哈希，不包含原始代码字节。补充确认了 BLESS/BIRTH/BLACK 的 8/8/250 默认值、once 的 50 ms 覆盖及 small 整数减半；数学公式沿用上一轮四部曲核对结果。

BRAVE 场景快照入口 `0x4846e4`，shake 循环的裁剪拷贝链路为 `0x59a62c -> 0x59a8b8 -> 0x59aac8`。行内核区分完全不透明源像素与半透明源像素：`0x59abe5..0x59ac17` 的不透明分支直接覆盖目标像素；裁剪矩形外没有写入。循环退出路径没有额外的零偏移拷贝。这是本轮保留边缘和最后一帧的静态代码依据，**不是原版游戏逐帧截图对比**。

四部曲本批路径实际覆盖默认 shake 与 small；once、long、short、单轴及重复选项的组合目前依靠参数单元测试和原版解析核对，未列为设备剧情覆盖。

当前使用现有 Scene 合成后的不透明快照；半透明快照明确拒绝，未实现原版另一条 alpha 行内核。时间采样由 Android 刷新及实际解码耗时决定，不声称帧率与原版相同，Rust f64/libm 与原版 x87 也未证明所有采样点逐位相同。探针的文本控件在游戏画布外，本批不宣称原版完整 UI 的抖动效果一致。

## 实现

- Rust 保存场景快照及可变目标缓冲区。每帧按照 `sin(t/7)`、`sin(t/8)` 的整数偏移裁剪拷贝，保留未覆盖像素；到期保留最后一次已绘制帧，下一次正常场景绘制再更新画面。
- 连续帧时间不得倒退，同一时间重复请求返回相同内容。帧请求不推进 PC 或指令计数；错误 token 和到期帧之前的完成确认被拒绝。确认完成后释放 shake 状态。
- 两个保留像素缓冲区合计上限 64 MiB。这不是整进程内存上限；PNG 包、其他 Scene 资源与 Android Bitmap 另占内存。
- JNI 继续使用同一 worker 线程和原有帧请求入口；Android 仅显示 Rust 返回的 RGBA。视觉请求附带执行 checkpoint，供原始 DAT 场景对照使用，不是存档格式。
- 首个变化帧在下一次屏幕刷新计算，避免重复计算零相位帧耗尽短转场时间。实际 elapsed 时间不改写，不用伪造相位通过测试。

## 已发现并修复的扩展路径问题

BIRTH 新路径含 50 ms wipe。初次模拟器运行中，部分请求仅出现 elapsed=0 和 elapsed=50 两帧，中间帧断言失败。`research/story-batch11-initial-wipe-observation.json` 保留失败事件和当时 APK 哈希。修复上述首帧调度后重新安装、重新执行回归；未把初次失败记作通过。另一次模拟器导出 RGBA 文件发生 90 秒 ADB 超时，单独复读同文件成功且长度正确；`story-batch11-transport-observation.json` 保留证据，随后完整重跑，不跳过像素校验。

## 验证范围

桌面原始 DAT 路径：BLESS 12 页后在 `story1:10` 停止于 movie；BIRTH 33 页后在 `012401:276` 停止于 flash；BRAVE 达到本批 100 页上限（`011001:247`），不是脚本结束；BLACK 19 页后在 `P01:90` 停止于 flash。总计 164 页。探索输出与最终桌面输出的 334 个文件逐字节一致；页图已通过独立 DAT/Pillow 对照。

本批工作区 Rust 测试及桌面/Android ARM64 Clippy 通过，补充了透明度/尺寸契约测试，Python 11 项研究测试通过（含两项新增清理证据测试）。新测试覆盖边缘保留、到期保持、倒退时间、重复帧、错误 token、提前确认及释放后失效。清理验证按日志顺序拒绝释放后的帧/确认，包含相同毫秒时间戳的情形；两份设备生命周期日志也重新通过该更严格检查。

生命周期修复后的四项专项已在两台设备通过。shake 开始后，真机 68 ms、模拟器 71 ms 进入 onPause；各已提交一帧，随后无 shake 完成确认或迟到帧。最终包完整剧情回归也已在两台设备通过。

### 最终设备证据

- 真机：原生 ARM64；模拟器：x86_64 经 libnb 翻译执行 ARM64。两者分别通过 164 页、13 次 shake、15 次 wipe、90 个音频请求，保持横屏。
- 真机核对 102 张 shake RGBA 帧、69 张 wipe 帧；模拟器分别为 79、47 张。每次效果均有中间帧，shake 确实出现非零位移，到期帧保持上一帧，完成确认不早于名义时长。
- 每台 43 份视觉请求快照（fade/wipe/shake）通过独立 DAT/Pillow 场景与像素校验；最终包导出的 86 个 PNG/metadata 文件与独立校验输入逐字节一致，哈希记录在 `*-snapshot-verification.json`。
- 六份剧情、FFI、生命周期报告统一对应 APK SHA-256 `bf805f74b20bf8f9ba322eaf27b4495586f114d20b245d74ff84d945c7e891cc`。最终源码编译出的原生库与 APK 内的库一致。
- FFI 专项沿用既有小型 fixture 隔离预算/分配失败和销毁，不作为原始 DAT 演出证据；剧情测试单独走 SAF 原始 DAT。

这不是帧率或严格时长性能验收：探针还同步导出 RGBA 供验证。最终 shake 完成较名义时长最大延后真机 78 ms、模拟器 64 ms；wipe 分别延后 58/86 ms。没有将这些延迟写作原版同步一致。

报告：`research/story-batch11-{arm64,translated}-verification.json`、同前缀的 `*-ffi-verification.json`、`*-lifecycle-verification.json`、`*-snapshot-verification.json`；桌面、页像素、音频与 mask 对照分别见 `story-batch11-desktop-verification.json`、`story-batch11-pixel-verification.json`、`story-batch11-audio-reference.json`、`story-batch11-mask-reference.json`。

## 后台切换验证设计

沿用 Home 中断 fade、BGM 和 BGM fade。另在真实 BIRTH shake 请求开始后 35 ms 由验证宿主调用后台切换，触发实际 onPause/onDestroy；仅通过 `close_on_shake` 探针启动参数启用，不改变原始指令时长。检查 VM 管理内存归零、播放器及临时音频释放、被取消 shake 无完成确认、释放后无帧或迟到确认。该测试不是暂停恢复功能验收。

初次真机测试发现 onStop 受窗口退场影响延迟，250 ms shake 可在清理前完成。保留 `story-batch11-initial-lifecycle-observation.json` 后，将主动退出会话提前到 onPause，并验证其后无帧和完成确认；VM 仍在 worker 上由销毁流程释放。

## 复现

先用 `verify_story_boundaries.py --profile batch11` 生成空目录中的桌面页；使用 `verify_story_layers.py` 校验页图，`reference_story_audio.py` 生成请求音频哈希，`prepare_mask_reference.py` 从原始 DAT 准备本地 mask 对照。设备使用 `verify_story.py --until-boundary --allow-page-limit --effect-captures LOCAL_DIR`，之后对该 LOCAL_DIR 再运行 `verify_story_layers.py --effects` 验证所有视觉请求快照。`--allow-page-limit` 仅允许参考目录明确缺少 boundary 的有界页数测试，不把退出码当成剧情完成。

原始 EXE/DAT、场景快照、逐帧 RGBA、音频和 APK 均留在本地。未修改上一批历史验证报告。
