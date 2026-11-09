# Batch 28：原始 Config 分类音量

## 修复与边界

此前 Android 只应用指令音量、淡出和焦点 duck，漏掉了游戏 Config 分类音量。BLESS 原始 `config.nut` 默认 BGM、SE、ENV、SYS 为 50%，语音为 75%。因此普通 BGM 的指令 50% 应与分类 50% 相乘，当前下发线性增益由 0.5 修正为 0.25；ENV 为 0.25、普通 SE 为 0.5、语音为 0.75。映射依据是此前对原版 command/category/fade 乘积及 DirectSound 衰减转换的静态验证，不是原版与模拟器的声学对录。

`StorySession::audio_config/set_audio_config` 直接读取、修改原始 Squirrel Config，并调用现有 `RestoreVolume`。Rust 不另存一份配置，不在 Kotlin 硬编码游戏默认值。未知通道、非整数、负数、超过 100 的值在边界报错；设置修改不完成脚本请求、不推进页面。JNI 操作 9 读取，操作 10 以通道索引和百分比更新，沿用原单一工作线程和错误边界。

Android 对现有播放器按 `command × category × fade × duck` 应用音量。修改分类音量保留播放器、通道身份、位置和淡出状态；异步准备完成时使用最新分类值。原 `gain` 日志仍记录指令百分比，新 `audio-gain-events.jsonl` 记录实际下发值与独立因子，避免混淆两种口径。

剧情宿主新增“音量（当前会话）”设置入口。这不是原版 Config 图形界面的完整移植，也没有实现原 `config.sav` 或跨启动持久化。VolSYS 可修改 VM 设置，但系统提示音播放尚未接入。MuteBGM、VoiceCheck、KeepVoice、逐字显示和两阶段点击推进尚未实现；本批不改变这些行为。

## 原版依据

`research/verify_config_source.py` 通过已有 DAT 补丁优先级读取 BLESS 的原始 `system/config.nut`，核对默认表达式，原始字节 SHA-256 为 `42e4126c3953b8854c2bf784fb39f959768e65dd6dbc7bccb9a0e1ee4ad5a894`。结果为 `research/config-batch28-original.json`；原版音量乘积证据引用 `research/env-batch23-original.json`，没有修改历史报告。

## 验证

设备为 `emulator-5568`，Android 9、x86_64 内核、`libnb.so` 翻译 ARM64。用户本轮不连接手机；本批没有原生 ARM64 真机结果。使用 SAF 原始 DAT，未改游戏脚本或改用预解包运行路径。

- Config 专项：13 页 JSON/PNG 对照后，对五个分类各做 0%/100% 共 10 次真实滑条操作。读取 VM 返回值，核对播放器身份、BGM 实际下发值与剧情页未变化，重开设置显示当前值，然后推进第 14 页并正常销毁 VM。
- 失焦与淡出专项：原生 MediaPlayer 焦点丢失、失焦期间准备与 ACK、恢复、重复淡出通过；BLESS 默认 BGM 有效增益断言更新为 0.25。
- 生命周期专项：61 页、5 次后台往返通过，包含视频 Surface 重建、BGM 反复暂停、角色动作恢复和 VM 清理。完整会话重启后重新读到原始 Config 默认值。
- Rust workspace 测试通过，Runtime 50 项；Clippy 全 targets 和 fmt 通过。新增配置测试覆盖真实 Squirrel 状态、非法输入不变性与页面不推进。Kotlin 增益测试覆盖分类与指令、淡出、duck 的组合；15 项 Python 测试通过。

793 页有界回归全部通过，逐页 JSON/PNG 与既有参考一致，493 次音频请求通过资源哈希与播放事件核对。其中 BGM 19 次、ENV 1 次、SE 7 次、语音 466 次，实际下发增益逐事件符合分类乘积；ENV 观察到 10 次回绕，SE 观察到 3 次回绕。通道停止、缓存清理及 VM 释放后存活内存为 0 的检查通过。

11 个角色动作页面共 65 帧，通过原始 DAT/Pillow 与既定原版曲线的独立重建。wipe、震动和闪光保留原有独立像素检查。动作帧校验器首次运行被旧的“仅原生 ARM64”设备条件拒绝，尚未进入像素比较；新增显式 `--execution translated` 并校验内核/native bridge 后通过，没有降低图像或动作断言。历史真机报告未改写。

所有设备专项使用同一 APK：`9f3d5360e301d207a2166b2f1f307691f5e928719d2f36cea4c24e81be076977`。`research/config-batch28-summary.json` 汇总八份来源/专项/回归记录的规范化文本哈希；`research/verify_config_batch28.py` 独立重放配置变化、设备分类、构建一致性、生命周期和四类增益。APK、原始 DAT、图像及音频不提交。

## 下一步

先实现共用运行时的文字显示完成状态与两阶段 NEXT：文字未显示完时完成显示，完成后才推进，并按原版 KeepVoice 决定停止、淡出或保留语音。MuteBGM 需要单独确认原生函数的衰减条件、程度和恢复时机，不能把 Android 焦点 duck 系数当作原版行为。配置持久化应与未来平台无关存档契约一起安排。
