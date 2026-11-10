# Batch 50：快捷存档／快捷读档

## 原版依据与入口

BLESS 本机 `system/saveload.nut` 中，QuickSave 先检查 InRoute，将第 2 至 10 槽逐级后移，再保存到第 1 槽；QuickLoad 默认参数为 1。ViewSavePage 的第 11 个页签（Q，内部 page=10）读取同一组十个快捷槽。`config.nut` 默认 F7／F8 分别绑定 QSAVE／QLOAD，`system.nut` 通过 InvokeCommand 调用它们。

这是脚本静态证据，不是本批重新运行 Windows 原版得出的动态时序结论。BLESS 原版 menu.mng 实际只有 Save、Load、Config、Reset、Quit 五个已绑定按钮；快捷按钮位于另一层 `shortcut.mng`。用户提供的 BLACK and WHITE 右侧菜单截图补充了界面位置证据；实现和设备回归仍使用 BLESS 自身资源，不混用截图对应游戏素材。

首轮验证脚本错误地假设独立剧情菜单存在 QSAVE 按钮，在找不到按钮时明确失败。后续补查 `shortcut.nut` 的 ShortCutIcons，确认 SIDEMENU 顺序为 QSAVE、QLOAD、SAVE、LOAD、AUTO、SKIP、LOG、CONFIG，并按 SLOT0 至 SLOT7 定位、按 Config.Popup 绘制提示。没有将独立菜单与右侧悬浮菜单混为同一资源。

Rust `SideMenu` 读取原始 DAT 中的 shortcut.mng，统一生成图标、禁用图标、浮层和命中区域。Android 仅做屏幕映射，首次点击原版右侧区域展开，再点击按钮执行，点击区域外收起；这属于触摸适配，不宣称与桌面鼠标悬浮触发完全相同。快捷读档按有效快照检查启用，普通存读、自动、快进和设置复用已有操作，LOG 明确提示未接通。没有保留临时添加的底部“快存／快读”按钮。键盘 F7／F8 继续读取用户当前绑定，读档页（包括标题入口）的 Q 页提供历史选择。

## 实现

快捷与普通存档使用同一个 Rust `Saved` 快照、校验、准备及提交过程，未新增独立 VM、资源解析器或 Android 剧情状态副本。快捷操作经过当前对白 sequence 检查并进入存档模态，关闭自动／快进；快捷存档成功后回到原对白，不推进 PC，也不打开普通槽位页。过期请求在提交模态前拒绝，不销毁当前 VM。

普通 100 槽与快捷十槽分开。共用 `SaveUi` 的内部 101 至 110 对应快捷历史 1 至 10，画面仍显示 001 至 010。空槽及损坏预览禁用；直接快捷读档遇到空槽或校验失败，显示错误并恢复当前对白，准备失败前不清理旧播放器。

Android `QuickSlots` 只管理宿主文件：先让 Rust 导出并同步独立快照，再用 AtomicFile 发布最新在前、最多十项的索引。写入新快照或发布索引失败前，原索引引用的快照仍保留。被淘汰及未发布的快照在下一次保存前清理，因此正常完成后最多保留一个不在索引内的旧快照；不把它显示成第 11 个有效快捷槽。

普通存档格式仍为 `NKSV\x01`，不访问原版 `.sav`。快捷槽索引只是 Android 私有文件映射，里面没有脚本状态；未来其它宿主可以采用自己的原子文件实现并调用同一套 Rust 快照 API。

## 边界

继承 Batch 48／49 的范围：仅当前已实现的对白等待边界；音频按资源、声道和循环属性重启，不恢复采样位置；字体与缩略图未证明原版逐像素一致。原版 `sound("STAR")` 快捷操作提示音尚未接入，本批反馈为 Android 提示，不宣称系统提示音一致。没有进行断电／文件系统故障注入实验，AtomicFile 的使用不等同于完成这类可靠性验证。

设备结果以同批 research JSON 为准；测试使用隔离 probe 存档目录，不覆盖用户普通存档。模拟器为 emulator-5568，x86_64 + libnb.so 的 ARM64 翻译执行，不是 ARM64 真机。

## 验证

158 个顶层 Rust 测试、7 次隔离原生探针、17 个 Python 测试通过。新增用例覆盖快捷模态的过期请求、禁止推进、取消后状态保留，以及右侧禁用按钮／拖出取消不派发动作。fmt check、Clippy all-targets / deny warnings、验证脚本语法检查和 NDK r27d / API 23 ARM64 构建通过。

最终 APK：`0da003db6e23f3baa9ec473cd98b2d7efbfb73139973705bde2febbf9dc8757e`。

`quick-save-batch50-emulator.json` 验证 57 页原始 DAT 剧情基线及普通存读回归、重启进程恢复、12 次快捷写入后的最近十槽顺序、普通槽不被改写、Q 页最旧槽恢复到第 3 页、最新快捷槽恢复到第 56 页并继续第 57 页、F7／F8 和原版右侧 Q.Save／Q.Load、标题 Q 页直接恢复，以及损坏快捷槽拒绝后当前状态保持。

`quick-save-batch50-independent.json` 重新读取设备证据：快捷／普通恢复的 BGM 和语音资源摘要与原始播放一致，BGM 循环属性保持；旧 token 不再回调，各运行会话关闭后 VM 分配为 0、音频缓存为空。快捷索引恰好十项且均存在，物理文件为十一份（包括待下次回收的淘汰快照）。本批没有将播放状态和资源摘要验证等同于人工听音确认。

`quick-save-batch50-side-menu.json` 验证原版八项顺序、空 Q.Load 禁用、触摸快捷存读、右侧 Save／Load 打开后返回原对白、右侧 Config 返回同一对白、Log 明确提示未实现，以及关闭 VM。设备截图保存在本机 `.local/batch50-side-device/`，检查了图标在剧情右侧叠加、有效快捷槽启用和原对白显示，不提交游戏截图。

首轮菜单入口误判与中间构建日志仅保留在 `.local`，不冒充最终 APK 结果。最终完整回归在补齐退出回调检查后重跑；过程中 UIAutomator 的一次空层级结果经既有有限重试恢复，没有跳过功能断言。本批未重跑完整自动／快进专项，也没有扩展 CG／回想内容验证。构建、报告和日志摘要见 `research/quick-save-batch50-checks.json`。
