# Batch 26：音量、重复淡出与音频焦点

本批修复音频宿主中的确定性错误，不推进新的剧情边界。共用 Rust Runtime 继续通过 Android SAF 读取 BLESS 原始 DAT。后台恢复、完整存读档、文本交互和跨页动作调度尚未完成；本报告不将它们计为已修复。

## 修复内容

- Rust 为 BGM 和 ENV 明确发送默认指令音量 50%，SE 保持 100%。Android 原先未设置 BGM 音量，实际使用播放器默认满音量。
- 指令音量与淡出包络分别保存。重复淡出从当前包络开始，新曲目才重置包络；音频焦点衰减另乘独立系数。
- 普通剧情音频临时失焦时暂停，恢复焦点后继续；可衰减的失焦采用 0.2 系数。永久失焦暂停并清除焦点持有标记，不将负焦点事件直接作为运行时致命错误。
- 音频异步准备期间失焦时，保存待启动回调；实际恢复播放后才 ACK，避免脚本在尚未播放时推进。被释放或替换的播放器会同时清除待启动回调。
- 错误回调增加播放器身份检查，旧实例的错误不能终止替换后的播放器。

所有播放器和焦点状态仍由 Android 主线程管理；Rust 的请求 token 和剧情推进职责不变。

## 原版依据

`research/verify_original_bgm_gain.py` 固定 BLESS EXE SHA-256，核对 12 处机器指令和两个字符串。`0x4446d4..0x4446e7` 将 `sound/bgm` 注册为通道 0、默认类型 1；`0x44480f` 保存描述符类型；`0x445292..0x4452a5` 为该类型选择默认指令音量 50。输出见 `research/audio-state-batch26-original.json`。

此处只证明指令音量。原版 Config 分类音量、对白期间 MuteBGM 和用户配置持久化尚未接入，不能把 50% 视为完整最终混音增益。Android 音频焦点策略属于平台适配，不宣称来自原版 Windows 行为。

## 专项验证

`platforms/android/verify_audio_state.py` 在原生 ARM64 PGKM10 上从原始 DAT 执行前 13 页，逐页对照既有桌面 JSON/PNG，经真实视频返回按钮到达 BGM 页面。仅显式 `audio_state_probe` 模式启用测试驱动；正常运行不插入测试命令。

测试通过另一个 AudioManager 客户端请求临时焦点，检查真实焦点回调和 MediaPlayer 状态；不是直接伪造焦点通知。随后在失焦期间重新准备同一真实音频资源，确认没有提前 ACK，再释放测试焦点客户端。最后调用生产淡出处理器两次，检查包络连续。

最终专项报告 `research/audio-state-batch26-native.json` 记录：

- 初始 BGM 指令增益 0.5。
- 暂停位置连续两次均为 3776 ms。
- 失焦期间替换的 BGM 位置为 0、未播放；焦点恢复后才出现 playing 和唯一 ACK，播放位置随后为 74 ms。
- 连续淡出的衔接增益均为 0.41625，随后下降到 0.3465281。
- 显式结束后 VM live bytes 为 0。

这是播放器状态与传给 `setVolume` 的增益验证，没有声波录制、原版响度对比或新增人工听音结论。可衰减失焦的增益乘法有 Kotlin 测试；永久失焦、真实来电以及视频自身焦点处理没有在本批专项真机覆盖。

## 完整回归

最终 APK SHA-256：`c78a31d1de336a656dd93589843f65ac120ee4a7e87f2af93a17d9548744e5f7`。专项与完整回归使用同一 APK，PGKM10 原生 ARM64、native bridge=0；未使用 root、模拟器或其他三部。

- BLESS 从标题到既定第 793 页全部通过既有桌面 JSON/PNG 和脚本来源对照；这仍是有界回归，不是完整游戏通关。
- 493 次音频请求通过原始 DAT 资源哈希和播放事件校验。19 次 BGM、1 次 ENV 的指令音量均为 50%，7 次 SE 均为 100%。
- ENV、SE 要求实际观察循环回绕；SE 停止的通道隔离检查保留。wipe、震动和闪光中间帧通过既有独立计算核对。
- 正常结束确认音频缓存删除、播放器全部释放、VM live bytes=0。
- Rust workspace（Runtime 49 项；既有 ignored 项不计作通过）、Clippy 全 targets、fmt、15 项 Python 测试和 Kotlin AudioGain 测试通过。
- 一轮早期专项因验证脚本没有处理开场视频而超时；随后补入已有视频返回流程。另一轮普通回归在补充异步准备失焦修复时主动中止。这两轮均未计入最终通过结果。

证据见 `research/audio-state-batch26-{native,story,original,audio-evidence,summary}.json`。`research/verify_audio_state_batch26.py` 重放音量、暂停位置、重复淡出以及失焦准备 → 恢复焦点 → playing → 唯一 ACK 的顺序。`research/verify_story_audio_evidence.py` 独立重放正常回归音频事件。

复现：先以 `platforms/android/verify_audio_state.py` 执行专项；再用 `platforms/android/verify_story.py` 的既定 793 页参考、`--require-env-loop --require-se-loop --movie-completion secondary --games 0` 完成正常回归；最后运行上述两项证据校验器。APK、原始音频、图像和捕获帧仅保存在本机。

## 仍需处理的主线问题

1. **前后台恢复**：`MainActivity.onPause` 仍结束会话。需要冻结 Rust 请求推进和有效动画时钟，暂停音频，并处理视频 Surface 重建；显式关闭才销毁 VM。音频焦点恢复不等于应用后台恢复。
2. **Config 和文本交互**：原始 `config.nut` 默认 `KeepVoice=1`；`message.nut` 的 ID_NEXT 分支在文字未显示完时只切换最快显示，显示完成后才按 KeepVoice 决定停止、淡出或保留语音。当前宿主仍直接显示完整文字并无条件停止语音。应先读取共用系统状态，再实现统一输入行为，不能把另一个固定值当成通用修复。
3. **动作调度**：同一角色的多次命令、动作与 SE/ENV/wait 的交错，以及快速点击时 `Screen.Commit` 的实际队列语义仍须确认。现有正常回归等待动作完成，不能覆盖快速点击语义。
4. **可恢复存档**：`StoryLabels` 仍仅是列表元数据。需要明确脚本调用栈、变量、系统状态、场景和音频的快照契约及恢复边界；本批没有添加会丢失状态的“存档”接口。

后续顺序：前后台有效时钟与视频恢复 → Config/文本输入及快速点击 → 共用动作队列调度 → 基于已稳定状态契约的存读档。继续 Android 优先和 BLESS 真机验证，不为其他平台复制 VM。
