# Batch 52：消息日志 PAGE／CLICK 音效

## 原版依据与实现

BLESS `message.nut` 第 66、135 行在进入 ViewLog 和 ID_CANCEL 返回时调用 PAGE；第 122–124 行只有滚动位置变化时调用 CLICK。`system.nut` 的 ID_LOG 直接调用 ViewLog，因此现有 F9／右侧 Log 入口不额外添加 CLICK。另一个 ID_BACK 分支在 ViewLog 前调用 CLICK，但本批没有新增或映射该输入。

本机原始 sound.dat 中，PAGE 为第 35 项 `sound/se/se_PAGE.ogg`，CLICK 为第 84 项 `sound/se/se_CLICK.ogg`。沿用上一批已经核对的 EXE `sound` 函数证据：名称格式为 `se_%s`，读取 `sound/se`，音量为 VolSYS。`verify_original_message_log.py` 重新比较原始 DAT 与导出脚本，记录代码位置、资源摘要及固定 EXE 指令。证据为静态代码／资源核对，不宣称本批做了 Windows 原版动态听感对照。

Rust 决定音效事件：打开／关闭日志产生 PAGE；指针处理前后的日志滚动位置不同才产生 CLICK。边界箭头、重复点击同一滚动位置和点击历史语音均不产生 CLICK。事件随当前日志响应交付，不在重绘或 Android 生命周期恢复时重复生成。

Android 复用已有 UI 音频播放路径，新增 JNI 操作只接受 PAGE／CLICK 两个固定事件；资源读取与 VolSYS 查询仍在 Rust。STAR／PAGE／CLICK 共用 system 声道，新的系统音效可以替换旧系统音效；BGM 与 voice 保持独立。没有新增音频后端、剧情状态副本或存档格式。标题与普通／快捷存读档的 STAR 路径保持原有含义。

## 验证方法

`verify_story_saves.py --quick-check --log-check --log-sound-check` 在 BLESS 原始 DAT／SAF 路径上回归。每次日志往返检查 PAGE 开关各一次、有效滚动的 CLICK 次数，并对顶部／底部边界、同位置点击、历史语音点击检查没有多余系统音效。除原有对白包和场景 PNG 对照外，继续检查日志期间文字状态冻结、返回后的显字数量与推进标志、BGM 播放器身份和历史语音。

测试将系统音量设为 0 和 37，保留其它音量，再恢复原值。独立脚本重读设备证据，核对声音条目来自对应原始 DAT、实际播放事件与进度、VolSYS 增益、静音值以及语音仍使用 VolVO，并验证音效 token 没有漏记或重复记录。普通／快捷／标题读档及 NKSV1 兼容继续回归。

## 边界

- 本批设备为 emulator-5568，x86_64 + libnb.so 的 ARM64 翻译执行，不是真机结果。
- 播放器状态、音频资源身份和增益验证不等于人工听音确认。
- 原有字体像素差异、原版 `.sav` 互通和音频采样位置恢复不属于本批；没有读取 CG／回想详细内容。
- 没有改动剧情 VM、角色调度或 Android 音频生命周期实现；没有扩大到其它系统页面的音效。

## 最终结果

最终 APK：`c5c6a83d7cf7a9a1bae49522c0036c14234a50b28ff9613db25759e099680d11`。

57 页剧情基线、9 次日志往返、普通／快捷／标题读档与 NKSV1 兼容回归通过。主回归中 PAGE 与 CLICK 各 18 次，全部匹配原始 DAT 摘要，并观察到播放位置和 PCM 播放头推进；VolSYS 覆盖 0、17、37、50，历史语音继续使用 VolVO=75。4 次语音重播在关闭日志后继续原播放器，BGM 在有该声道的用例中保持同一播放器。29 次 STAR 回归同样通过。各主会话关闭后 VM 分配为 0，音频缓存为空。

右侧浮动 Log 的触摸打开和“返回剧情”按钮另行通过，观察到开关各一次 PAGE、没有 CLICK，系统音量为已恢复的 50。原版资源与脚本证据、主回归、音频独立核验、存读档独立核验、右侧入口结果及摘要见 `research/log-sounds-batch52-*.json`。

161 个顶层 Rust 测试、7 次隔离原生探针、17 个 Python 测试、fmt check、Clippy all-targets / deny warnings、验证脚本语法检查和 NDK r27d / API 23 ARM64 构建通过。没有重新运行完整设置页或自动／快进专项。静态断言初稿将同文件 ViewHide 的 PAGE 也算入 ViewLog，已限定函数范围并重新验证；没有为此修改运行器行为或历史报告。UIAutomator 的空层级由既有限次重试恢复，次数记录在本批 checks JSON，没有跳过功能断言。
