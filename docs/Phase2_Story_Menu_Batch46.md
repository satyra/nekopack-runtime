# Batch 46：原版剧情 MENU 入口

## 范围及原版证据

BLESS 原始 `system/shortcut.nut` 的 `ViewMenu`（91–116 行）加载 `Images("menu")`，用 `SHORTCUTS` 绑定热点，并根据 `InRoute()` 启用 LOG / SAVE / QSAVE。菜单以背景快照和 BASE 精灵为底图，点击热点分派 `InvokeCommand`，取消键返回。`system/system.nut` 的外层 MENU 分派返回后取消自动和快进。

实际通过原始 DAT 加载的 BLESS menu.mng 仅含 SAVE、LOAD、CONFIG、RESET、QUIT 五个绑定热点，没有 AUTO / SKIP / HIDE。本批接通 MENU（工具条与 F2）、CONFIG（原版设置后返回同一对白）及 QUIT（结束当前宿主会话、销毁 VM）。SAVE / LOAD / RESET 点击后明确报告未接通并保留菜单，不保存、读档、重启或推进剧情。仍未实现完整剧情存档、日志回放、返回标题及原版退出确认窗口。

依据是原始脚本和资源，不是 Windows 原版可执行程序的全流程实测。原版 PAGE 音效及精确原生计时未补齐，不声称完全复现。

## 实现

- 新增共用 Rust Menu 展示对象，用已有 Squirrel Images / Spots 适配构造原版绑定与启用计划，读取原始 menu.mng。Rust 负责场景快照、MNG 图层合成、热点命中、按下／移出／释放／取消和动作合法性。保持原有半开矩形命中约定；画布与菜单维度不符会明确失败。
- 菜单图像及输出画布使用 64 MiB 解码像素预算。图片资源仅限 menu.mng，不读取 CG 或回想。
- StorySession 保持单一 VM、调用栈和 Variables。菜单期间拒绝文字帧、ACK、另一菜单、设置、隐藏及模式修改；unsupported 不关闭菜单。关闭校验当前页并取消自动／快进，保留正文计时及步骤。进入设置后沿用既有设置模态，不创建独立剧情 VM。
- Android 使用已有 TitleView 做坐标转换与画面呈现；菜单窗口不推进剧情。前台菜单不会主动暂停音频，后台沿用原有暂停／恢复机制。Android 返回键优先关闭菜单。菜单打开后不交付旧的快捷键展示请求。
- QUIT 沿用现有会话关闭入口；当前不冒充原生 QuitGame 确认、剧情保存或退出整个应用的全部语义。

## 验证设计

核心新增测试覆盖菜单状态下的推进拒绝、过期输入、重复关闭、未实现 SAVE、CONFIG / QUIT 交付、模式取消、已读不变和同页 packet 不变。复用现有 pointer 测试验证匹配按下／释放、移出取消与边界。

设备回归从原始 DAT / SAF 启动 BLESS，验证部分文字时进入菜单，拖动取消，SAVE / LOAD / RESET 保持当前页，屏蔽菜单中的 F1，后台保留菜单，返回键消费，F2 打开，CONFIG 配置修改并恢复后回到原对白，以及菜单 QUIT 后 VM live bytes=0。继续跑隐藏、快捷键、快进、自动和 57 页剧情基线；第 13 页验证菜单后台 BGM 暂停及同一播放器恢复。

首轮在拖动后过早进行下一次点击：渲染队列仍在处理 move/up，测试读取了中间状态。新增诊断中的 pointer_kind / accepting，等待释放呈现完成后再操作；未把首轮失败记为通过。实测菜单资源也纠正了最初对菜单含 AUTO / SKIP 的假设，没有添加不存在的按钮。

本轮设备 emulator-5568 为 x86_64 + libnb.so 的 ARM64 翻译执行。不是 ARM64 真机结果；不是完整游戏通关，亦不包含原版菜单逐像素对照、鼠标 hover 或物理键盘验证。

## 最终结果

最终 APK SHA-256：`687b7c5681a79380f48713a6f38783f4a26a80e7e2b9d8a0d7d5b21b8a1de5f7`。

149 个顶层 Rust 测试、7 次隔离宿主原生探针、17 个 Python 测试、验证器语法检查、fmt、Clippy（all-targets，deny warnings）及 NDK r27d / API 23 ARM64 构建通过。

BLESS 菜单专项和 57 页普通剧情基线通过；同一 APK 的隐藏、快捷键、快进与自动回归通过（自动推进 55 页，检查实际语音完成回调）。第 13 页菜单后台暂停 BGM，前台恢复同一播放器。菜单进入设置并返回保留同页；QUIT 关闭后 VM live bytes=0。设备 SharedPreferences 独立读回确认十项快捷键绑定已恢复。

结果见 `research/story-menu-batch46-emulator.json` 和 `research/story-menu-batch46-checks.json`。
