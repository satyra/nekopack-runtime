# Batch 23：独立环境音通道与后续章节

本批接通 BLESS `012701:9` 的 `.env ENV_001`，通过共用 Rust Runtime 和 Android SAF 读取原始 `sound/env/ENV_001.ogg`，不预解包、不转换资源、不改游戏脚本。最终 APK 只在原生 ARM64 真机回归。

## 原版依据

`research/verify_original_env.py` 固定原 EXE SHA-256，校验 54 处指令及原始 DAT 中全部 12 处 `.env` 用法，结果见 `env-batch23-original.json`。

- `0x4446f3..0x444706` 注册 `sound/env`，默认 loop=1、通道 index=1；BGM 为 index=0，SE 为 index=2。通道地址按 `0x584340 + index * 0xa8` 选取。
- 命名播放分支先在 `0x44600d` 释放本通道旧句柄，再加载资源并调用 `0x4c0024`。循环标志经虚表传到 `XSoundBuf::Play`，循环值 1 进入底层播放 flags；并不等待音频结束才返回。
- `0x445299` 给环境音命令默认音量 50，非同类默认值为 100。底层将命令、分类和渐变的百分比因子相乘，除以 1000000，再做 `2000 * log10(ratio)` 转换。此次把环境音命令自身的 50% 因子传给宿主，Android 应用线性 0.5。
- 无参数且无音量/声像修饰时，`0x446610` 立即释放选定通道，`0x4c0405` 清除句柄。没有默认淡出，也不释放其他通道。
- BLESS 实际使用三种 ENV 资源和无参停止。三个 Ogg 均无 Vorbis comment，解码大小均走已核对的缓冲播放分支。ENV_001 为 44100 Hz、176134 个采样，约 3.994 秒。

这些是静态控制流和原始资源证据，不是原版声波或循环接缝的动态对照。原版还会保存/恢复环境音通道名；本批没有添加完整存档恢复。

## 实现

Rust 发出独立 `env` / `env_stop` HostEffect。命名请求包含 `loop: true`、`volume_percent: 50` 及 DAT 中的原始 Ogg；停止请求不读取资源、时长为 0。匹配 token 的宿主 ACK 才推进脚本，重复 poll 不重复发起播放，过期 ACK 被拒绝。缺失资源与未支持参数明确失败。

Android 在现有播放器表中单独管理 env：播放前应用 Rust 的循环和音量参数，开始播放后 ACK；替换只释放同通道旧播放器，无参停止只释放 env。BGM、语音与环境音可以并行；视频切换和会话关闭沿用统一释放路径。记录播放位置回绕、环境音开始/停止时的 BGM 状态，以验证真实设备行为。

当前支持一个资源名和无参停止。显式 loop/once、fade、音量、声像参数未实现；原版全局/分类音量体系尚未接通。原版 BGM 命令也默认 50，现有 BGM 宿主仍保留此前增益行为，需另行统一；不能把本批环境音设置视为完整混音一致。

## 验收结果

- Rust workspace 测试通过，Runtime 45 项；既有隔离入口的 1 项 ignored 未计作通过。Clippy（全部 targets、warnings 为错误）、fmt 及 Python 15 项测试通过。
- 最终 NDK r27d APK：`de2cacd9792ea3a9c1f02fc1f0387d19bd5f16121a918b618f308cf751ffaba0`。PGKM10 / Android 15 原生 aarch64，native bridge 为 0。本批未使用模拟器或其余三部。
- 真机从标题经显式视频跳过，连续验证 598 页 JSON、PNG 和源文本；补齐此前 503–598 页共 96 页。前 563 页桌面输出逐字节不变；新增 96 页由原始 DAT/Pillow 独立验证图层与像素。
- 354 次音频请求资源哈希及播放/替换/释放证据通过；31 个演出终点快照独立匹配。
- 八个动作页面共 67 个捕获帧全部匹配独立参考，包含第 501 页队列，以及此前未在真机验收的第 521 页角色 7 下沉回位。
- 第 564 页开始 ENV_001，第 593 页 `.env` 停止并进入 `012702:11` 夜间对白；第 598 页 `012702:36` 再由 `.bgm` 单独停止音乐。
- 环境音实际观察到 10 次播放位置回绕，设置音量 50%、循环开启。ACK 在音频结束前返回，脚本继续执行；环境音跨过 1 次 BGM 替换及 17 次语音开始。停止 env 后 BGM 仍在播放，旧 env 无迟到进度/完成/循环事件。
- 独立关闭轮从标题到第 564 页，确认环境音已播放和推进后实际按 Home；音频资源、缓存和 VM 全部释放，关闭后观察 1 秒无迟到音频事件，无下一页。这是结束会话，不是后台存档恢复。
- 正常结束与后台结束均由 native 关闭检查确认 VM live bytes=0。横屏截图、APK、游戏资源和原始帧只留本机。

只计入最终 APK 的完整轮次。图层参考新增 `--expected-count`，本批明确要求 96 页，防止参考目录准备不完整时把部分结果计作通过。真机仍实际点击“继续”并核对每页；复用已识别的按钮坐标，电影界面切换时重新识别，未绕过 VM 或注入剧情状态。

## 复现与范围

- `research/verify_env_batch23.py`：原始 DAT 重跑 598 页、旧前缀比较、开始/停止/后续音乐停止断言。
- `platforms/android/verify_story.py --until-boundary --allow-page-limit --reuse-continue-bounds --require-env-loop --games 0`：本批明确的 598 页上限，不是未知指令停止。
- `research/verify_env_evidence.py`：校验环境音增益、实际回绕、通道独立、非阻塞 ACK 及停止后的事件。
- `platforms/android/verify_env_cancel.py --page 564`：实际环境音播放期间按 Home 并验证清理。
- `research/verify_story_layers.py --expected-count 96` / `research/verify_motion_frames.py --include-env-chapter`：新增静态页面与全部实际动作帧的独立参考。

结果与文件哈希见 `research/env-batch23-summary.json` 及同前缀报告。额外桌面探测曾输出 604 页，停于 `012702:67` 的重复 `<` 修饰符；这些额外页面未计入本批真机验收，也未在本批实现该动作。下一批应确认其原版语义再推进。
