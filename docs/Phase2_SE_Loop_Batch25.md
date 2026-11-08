# Batch 25：SE 单次、循环与停止

本批沿 BLESS 的实际章节调用继续推进。旧版 `d60425f` 从标题输出 785 页后，在 `012802:42` 的 `.se SE_028 loop` 明确停止；新版本接通该命令和随后 `012802:54` 的无参 `.se`，真机范围扩展到 793 页。始终使用共用 Rust Runtime、Android SAF 原始 DAT，不改游戏脚本、不使用预解包资源。

## 原版依据

`research/verify_original_se_loop.py` 固定 BLESS EXE 哈希，复查既有环境音证据中的公共音频路径，并补充 SE 注册、关键字和默认音量分支，共 54 处指令。输出同时记录证据来源哈希、原始脚本及 SE_028 的资源哈希。

- `0x444712..0x444725` 注册 `sound/se`，通道 index=2，描述符默认 loop/type=0；BGM index=0、ENV index=1，分别独立。
- `0x444ad3` 将显式 loop 置 1，`0x444b12` 将 once 置 0；标志经命令数据传到 `0x4c0024` 和底层播放。资源命令先释放本通道旧句柄，再加载并启动，不等待音频结束。
- `0x445292..0x4452a5` 按描述符类型选择默认命令音量。SE 为 100%，覆盖 loop 标志不把它变成 ENV 的 50%。上层全局/分类音量体系仍未补齐，本批不声称完整混音一致。
- 无资源、无修饰的停止分支 `0x446610` 释放所选通道，`0x4c0405` 清句柄；无隐式淡出，不停止其他通道。
- 原始 `012802` 在 28 行单次播放、42 行循环、54 行停止。SE_028 是 44100 Hz 双声道、48440 个采样，约 1.098 秒；没有 Vorbis comment，解码尺寸落在已核对的缓冲播放路径。

这是原版静态代码与原始资源证据；没有原版实时声波/循环接缝的动态对照。

## 实现和支持边界

Rust HostEffect 增加明确的循环布尔值。`.se NAME` 和 `.se NAME once` 发出非循环 SE，`.se NAME loop` 发出循环 SE，无参 `.se` 发出零时长、不读取资源的 `se_stop`。重复 poll 不重复发起请求，匹配 ACK 才推进；不存在的资源与未支持参数明确失败。显式 once 的页面历史规范化为默认单次形式。

Android 从 Rust 元数据设置 SE 的循环和 100% 命令音量，复用独立 SE 播放器、现有替换和生命周期清理。SE 开始后立即 ACK；停止只释放 SE，保留 BGM。循环过程持续记录播放位置回绕，旧 token 的迟到事件被既有播放器身份检查挡住。

只补上述语法。fade、volume、pan 等修饰、仅含 loop/once 的无资源命令、完整音频存档恢复均不在本批范围。显式 once 有原版静态与 Rust 单元测试依据；本段真实游戏使用默认单次及显式 loop，没有额外伪造 once 游戏脚本。

## 验收

- Rust workspace、49 项 Runtime 测试、Clippy 全 targets/warnings 为错误、fmt、15 项 Python 测试通过；既有 ignored 项不计作通过。新增测试覆盖默认/loop/once 的参数、顺序 ACK、重复 poll、连续停止和错误路径；停止证据测试同时覆盖 ENV、SE 的迟到事件。
- NDK r27d APK：`ed03a980094c5a1c2a8e8d1726cf9777b51ecab664eff7098172fb53e22de967`。PGKM10 原生 aarch64、native bridge=0；设备已由用户 root，但本批未调用 su 或修改系统。未使用模拟器或其余三部。
- 原始 DAT 桌面执行 793 页，旧前 785 页 JSON/PNG 逐字节不变。新增真机范围 607–793 共 187 页通过独立 DAT/Pillow 图层和像素重建。
- 真机从标题经明确的视频跳过验证到 793 页：783 页默认 SE_028 实际自然结束，786 页循环并观察到 5 次播放位置回绕；789 页停止 SE 后 BGM 仍播放，旧 SE 无迟到 progress/completed/looped；793 页进入 `012802:71` 的 XXA_012802_0000 语音。
- 493 次音频请求通过资源哈希与播放证据检查，42 个演出快照独立匹配。ENV 仍要求实际循环，未弱化既有检查。
- 11 个动作页面共 88 帧全部通过独立原始 DAT/Pillow/原版曲线复核，包含新增 640 页单角色下沉、685 页两角色同时下沉。更新后的校验器也重新通过上一批全部 75 帧。
- 另一轮从标题到 786 页，确认循环 SE 已播放、位置推进后实际按 Home。播放器、临时音频缓存和 VM 释放，关闭后 1 秒无迟到音频事件、无下一页，VM live bytes=0。正常结束同样确认 VM live bytes=0。后台行为仍为结束会话，不是可恢复存档。
- 横屏截图只留本机；没有新增人工听音结论、完整游戏兼容或恒定帧率声明。

报告及哈希见 `research/se-batch25-summary.json`。游戏资源、APK、截图和原始帧未提交；历史验收报告未修改。

## 复现入口

- `research/verify_original_se_loop.py`：原版公共音频路径、SE 注册与资源证据。
- `research/verify_se_batch25.py`：检查实际 CLI 退出记录、785 页旧前缀、793 页数量及 SE/后续语音位置。
- `platforms/android/verify_story.py --until-boundary --allow-page-limit --reuse-continue-bounds --require-env-loop --require-se-loop --movie-completion secondary --games 0`：本批 reference 明确为 793 页，不是未知指令边界。
- `research/verify_se_evidence.py`：默认单次完成、循环回绕、非阻塞 ACK、仅停止 SE 和后续语音。
- `research/verify_motion_frames.py --include-se-chapter`：11 个动作页含双角色逐帧复核。
- `research/verify_story_layers.py --expected-count 187`：新页面独立图层/像素；演出快照另以 effects 模式校验。
- `platforms/android/verify_env_cancel.py --channel se --page 786`：实际 Home 清理；默认 ENV 参数保留。

本批不宣称已找到或处理 793 页之后的全部边界。下一轮从这段已验证对白继续，仍按实际指令缺口推进。
