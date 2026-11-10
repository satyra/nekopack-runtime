# Batch 45：剧情隐藏对白与恢复

## 原版依据及实现范围

BLESS 原始 `system/message.nut` 的 `ViewHide` 创建 `Sprite.Base()` 背景快照，循环绘制，收到 `ID_OK`、`ID_SPACE` 或 `ID_CANCEL` 后返回。`system/system.nut` 的 `ID_HIDE` 分派在返回后调用 CancelAuto / CancelSkip。隐藏函数没有显式暂停音频或重置正文 Timer。

这是原始系统脚本的静态证据，不是 Windows 原版可执行程序的计时实测。原生 Timer 的内部行为仍未独立确认。本批约定隐藏期间计入前台活动时间，Android 后台时间继续由现有 SessionTimeline 排除。

Rust StorySession 新增对白隐藏模态及严格的进入／恢复操作；校验当前页序号、等待状态、推进许可及重复操作。隐藏时不接受文字帧、ACK、设置或模式变更，不推进脚本，也不因此标记已读；恢复保留同页状态并关闭自动／快进。HIDE 快捷键读取现有原版配置，返回宿主展示请求。

Android 提供“隐藏对白”按钮及 HIDE 绑定。进入时挂起文字轮询，捕获场景 ImageView，不包含姓名／消息框；用冻结快照覆盖场景，隐藏宿主控制条但保留布局尺寸。底层场景动画与音频不因该模态暂停。触摸、空格、Enter 或 Android 返回键请求恢复；恢复动作不作为正文 NEXT。隐藏时功能键不继续分派。后台切换仍暂停音频及活动时钟，并保留隐藏模态。

没有新增引擎专属 Android 变量表或独立 VM；状态合法性仍由共用 Rust 核心判定。此次不包含原版侧栏图标、PAGE 界面音效、存读档或回想功能。

## 验证方法与边界

新增 Rust 测试覆盖重复／过期转换、隐藏时 ACK 和文字帧拒绝、已读状态不变、恢复取消自动／快进及恢复不换页。现有工作区测试、隔离原生探针和 Python 测试继续运行。

设备验证器从 SAF 原始 DAT 启动 BLESS：检查部分正文隐藏、屏幕快照像素、返回键／空格／触摸恢复、HIDE 改绑、隐藏时功能键屏蔽、配置恢复、BGM、后台切换及角色动作附近的隐藏／恢复；同时复跑原有快捷键、快进、自动和 57 页普通剧情基线。没有读取 CG 或回想的详细内容。

首次设备尝试时模拟器未启动，启动专用 QA 实例后继续。第一轮发现验证脚本再次打开设置时使用了旧呈现快照，导致恢复绑定步骤失败；改为等待新的 input_sequence，单独恢复并核对原十项绑定，关闭 VM 后重跑。未将失败轮记为通过。

本批设备是 emulator-5568，x86_64 + ARM64 native bridge 翻译执行；不能替代 ARM64 真机回归。按键由 ADB 注入，不代表物理／蓝牙键盘验证。快照对比验证本实现冻结的场景，不等同原版画面逐像素对齐。短角色动作的检查不构成所有动画并发组合的验证。

## 最终结果

最终 APK SHA-256：`9a75eca093f7d933967f8e449f661595ebc8eb82a81067a4d95c6309d6bff11f`。

148 个顶层 Rust 测试、7 次隔离宿主原生探针、17 个 Python 测试、fmt、Clippy（all-targets，deny warnings）、验证器语法检查及 NDK r27d / API 23 ARM64 构建通过。BLESS 隐藏专项（含 Enter）及 57 页剧情基线通过；触摸／返回键／空格／Enter 恢复均未换页，隐藏前后 packet 一致，关闭 VM live bytes=0。

同一 APK 的模式回归验证了自动推进 55 页、真实语音完成回调、快进未读边界、历史重启、实时快捷键与配置恢复。设备 SharedPreferences 独立读回确认十项绑定已恢复。原版可执行程序精确计时、真机及所有章节仍未验证。

完整首轮结束后发现报告的 partial_dialogue 字段被隐藏快照变量覆盖；修正变量名后，对同一 APK 重跑隐藏和 57 页基线，并新增 Enter 验证。原模式回归仅保留未受影响的字段，未将被覆盖字段作为证据。

结果见 `research/hide-dialogue-batch45-emulator.json`、`research/hide-dialogue-batch45-modes.json` 和 `research/hide-dialogue-batch45-checks.json`。
