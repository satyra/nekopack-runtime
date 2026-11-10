# Batch 40：Android 设置返回键

## 改动

在当前 Android 宿主（minSdk 23、targetSdk 28）的 `onBackPressed` 中接入设置返回回调。设置可见或剧情设置仍在打开/关闭流程时消费返回事件，并复用现有 `configBack` 按钮及 native operation 22；按钮禁用期间不重复提交返回请求，也不落入 Activity 默认退出行为。

关闭层级仍由 Rust 判断：快捷键选择弹窗先取消；没有弹窗时关闭设置。剧情设置继续使用 Batch 39 的同一 VM、对白 token、暂停时钟及音频恢复流程；没有新增 Android 独立弹窗状态或改动 Rust 执行逻辑。标题设置同样走原有返回路径。Activity 销毁时清除回调；设置之外的系统返回行为保持现状。

这项接入针对当前宿主的系统返回键路径，没有升级 targetSdk 或新增预测式返回动画。

## 回归范围

`platforms/android/verify_story_settings.py` 改用 ADB 的真实 `KEYCODE_BACK` 取消快捷键弹窗和返回剧情，不以点击屏幕“返回剧情”按钮替代。取消弹窗后检查绑定未变、设置仍打开、脚本状态未推进；再次返回检查原对白恢复。原来的“设置页按 Back 直接销毁”断言已替换为“Activity 保持存活、同一页可以继续、再由明确的结束运行按钮关闭”。历史 Batch 39 报告不修改。

沿用 BLESS 原始 DAT / SAF 入口，覆盖已有文字、透明度、音量应用顺序、后台切换、BGM 同播放器恢复和角色动作队列。片头通过现有视频“返回剧情”按钮离开，不绕过 VM。验证未进入 CG 或回想页面。

设备为 emulator-5568，x86_64 + libnb.so 翻译执行 ARM64 APK；不是真机回归。结果与构建记录见 `research/android-back-batch40-emulator.json` 和 `research/android-back-batch40-checks.json`。

## 结果

- 第一次系统返回只关闭 F3 快捷键弹窗；绑定、设置页和剧情状态均保持不变。第二次返回恢复原对白，Activity 没有销毁。
- 第 1 页打开时已显示 7 个字符，返回后仍为相同 token，文字继续显示而非从头开始。设置修改、持久化及恢复原配置通过。
- 第 56 页在角色动作时钟 41ms 时进入设置，后台返回后通过系统 Back 恢复，动作继续并完成；57 页元数据及场景 PNG 与既有基线一致。
- BGM 保持同播放器及资源 token；音量先应用再恢复播放。明确点击“结束运行”后 native close 通过，VM 追踪存活字节为 0。
- NDK ARM64 构建与验证脚本语法检查通过。Rust 核心未修改，不将 Batch 39 的单元测试计为本批重新运行。一次 UiAutomator 空根节点经限次重试恢复，无 ADB 文件读取重试，全部断言执行完成。

本批未宣称全篇通关、真机结果或预测式返回手势动画验证。
