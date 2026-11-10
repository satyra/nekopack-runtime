# Batch 49：原版存读档界面与标题读档

## 实现范围

在 Batch 48 的运行器快照上，接通 BLESS 原版 `system/saveload.mng` 界面。剧情菜单与标题页共用 Rust `SaveUi`，通过 SAF 挂载的原始 DAT 读取底图、槽位框、页签、按钮和按下状态，不再使用 Android 简洁槽位 Dialog。

普通存档共十页、每页十槽，支持存读模式切换、页签、覆盖写入、空读档槽禁用和返回。槽位显示编号、文件时间、章节标签和场景缩略图；坏文件显示“损坏存档”，读档时禁用。标题进入 Load 模式，不提供 Save 切换。原资源的 Q 页签仍可见，但点击明确提示快捷读档尚未接通，不伪装成可用快捷存档。

界面布局、槽位偏移、缩略图先于框绘制、文字大小和颜色来自本机 BLESS `saveload.nut`。原版存读行为观察沿用 `research/original-save-batch48-observation.json`；本批没有额外打开 CG 或回想页面。

## 核心与宿主边界

`crates/nekopack-runtime/src/save_ui.rs` 负责素材合成、文字、缩略图与命中区域；Android 负责文件描述符、时间格式、触摸坐标映射和界面呈现。槽位预览逐页读取，使用文件时间和长度缓存当前打开期间的预览；实际读档重新读取并完整校验文件，不依赖预览缓存。

`Saved::decode` 同时供剧情和标题使用。预览验证文件、脚本与场景，不加载音频载荷；实际准备阶段才建立恢复所需音频。标题读档将准备好的状态交给同一套 StorySession，不调用标题 Start，不重放开场指令。配置及已读记录仍属于当前用户配置，不随快照倒退。

成功准备后，Android 清理旧对白调度与音频，再提交快照、恢复场景与音频并显示保存的对白。校验失败不清理当前 VM／播放器；取消回到进入界面之前的对白或标题。返回键关闭存读档页，不退出应用。

## 兼容范围与限制

二进制格式仍为 Batch 48 的 `NKSV\x01`，本批没有改变字段顺序或格式版本；不读取、转换或覆盖原版 `.sav`。存档仍限于已实现的对白等待边界，不宣称支持任意 Squirrel 堆／协程快照。

持续音频恢复资源、声道和循环状态，从头播放；当前语音重新播放，没有音频采样位置恢复。角色动作中间时钟有 Rust 测试，设备保存操作有 UI 延迟，不把设备动作页通过等同于精确捕获动作中间帧。

动态文字使用现有宿主 CJK 字体和合成粗体，未宣称与原版 MS Gothic 逐像素一致。缩略图使用面积平均缩放，未验证与原版 Reduction 的位级一致性。快捷存读和原版存档互通仍未实现。

## 验证记录

专项脚本：`platforms/android/verify_story_saves.py`；独立证据检查：`research/verify_story_save_evidence.py`。设备为 emulator-5568，x86_64 + libnb.so，执行 ARM64 APK 的翻译路径，不是 ARM64 真机。

存读档专项验证：原版页签及模式切换、空槽禁用、覆盖字节一致、后台保持模态、首段存读与继续、重启进程后读档、57 页原始 DAT 剧情比较、角色页恢复与继续、BGM／语音重启、文件预览后损坏的拒绝与取消恢复。

标题专项验证：取消后仍在标题、损坏预览禁用、无 Save 切换、读入第 56 页再进入第 57 页；没有执行标题 Start，没有额外开场视频或转场效果。独立检查确认旧音频 token 不再回调，恢复音频资源摘要相同，正文 token 单调，各会话 VM 关闭后分配为 0、音频缓存清空。

界面截图在本机 `.local/batch49-populated-save.png` 与 `.local/batch49-title-load.png`，检查十槽布局、缩略图、章节标签与页签，未将游戏图片提交仓库。

专项结果：`research/original-save-ui-batch49-emulator.json`；独立结果：`research/original-save-ui-batch49-independent.json`。完整构建、测试与兼容回归结果见同批 checks 文件。

156 个顶层 Rust 测试、7 次隔离原生探针、17 个 Python 测试通过；fmt check、Clippy all-targets / deny warnings、验证脚本语法检查和 NDK r27d / API 23 ARM64 构建通过。

兼容回归通过三次返回标题／重新开始、旧 VM／播放器清理与已读保留，剧情菜单 SAVE／LOAD 取消、F2、设置页嵌套及返回键，部分对白恢复、透明度即时刷新、后台模态暂停、BGM 同播放器恢复，以及另一次 57 页剧情与动作页比较。本批未重跑完整自动／快进专项。UIAutomator 曾两次短暂返回空层级，既有有限重试取得有效层级后继续，未跳过断言。

兼容结果：`research/original-save-ui-batch49-settings-emulator.json`；汇总与日志摘要：`research/original-save-ui-batch49-checks.json`。三份设备报告的 APK 摘要一致：`390063a0e4280509f99e6c9e7bccc75534a12ddf9aaecb1d11a740a47d23aaa9`。
