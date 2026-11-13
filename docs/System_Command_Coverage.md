# 原六作系统命令覆盖清单

2026-11-13，按本轮共用协程桥、消息钩子和 Save 8 接入后的源码更新。Story 已能分派实际注册的普通 `com[name]`，执行原函数的即时返回或跨帧等待；消息页也已消费原 MsgBox 对象及有效绘制钩子。仍不能宣称“六作全部系统命令均已完整接入”：注册、函数分派、宿主 API 覆盖、分支结果回传和实际设备闭环是不同层次。

## 证据范围与方法

本清单依据系统 `.nut`、当前运行时源码和命令元数据，不读取剧情正文、回想正文或图像。原注册审计未操作 ADB；本次文档更新也没有设备操作。本清单不替代主代理的最终 APK 验收报告，也不是剧情使用率或视觉一致性报告。

- TD1–TD4：`D:/codex/电脑移植安卓/research/inspection/pc-utf8/TD{1..4}/*.nut`；与 `.local/title-verified/{0..3}/system` 的已缓存系统注册交叉检查。只读取 `.nut`，未读同目录 story*.txt。
- ED：原 `D:/codex/td-port-workspace/游戏/Endless Dungeon/Endless Dungeon/system.dat`；按已知系统模块名通过 CLI `lookup-name` / `read-entry` 定点提取到 `.local/system-command-coverage/ed`。
- sorakoi：原 `.local/sorakoi-import/game/ソラコイ/system.dat`；相同方式提取到 `.local/system-command-coverage/sorakoi`，与既有 analysis 系统源码核对。
- 两个定点提取集合包含 system、message、config、select、saveload、staff_roll、x_rooms、macro、title、gui、shortcut；不存在的模块明确跳过。此处统计这些已验证系统模块中的字面量 `com["..."] = function`，排除注释，未声称枚举未知模块或动态构造的注册键。
- 注册元数据：`.local/system-command-coverage/registrations.json`，含命令名、系统文件和物理 LF 行号。以下行号按文件 LF 计数；部分缓存是 CRCRLF，编辑器显示可能不同。

## 六作注册清单

以下 `<dialogue>` 表示空字符串键 `com[""]`，它不是无参点命令。

### TD1（11 个有效字面量注册）

- `config`：`config.nut:256`。
- `chapter`：`message.nut:152`。
- `date`：`message.nut:176`。
- `<dialogue>`：`message.nut:197`。
- `sysload`：`saveload.nut:333`。
- `select`：`select.nut:4`。
- `map`：`select.nut:258`。
- `staffroll`：`staff_roll.nut:65`。
- `title`：`system.nut:46`。
- `route`：`system.nut:47`。
- `room`：`x_rooms.nut:10`。

### TD2（11 个有效字面量注册）

- `config`：`config.nut:259`。
- `chapter`：`message.nut:152`。
- `date`：`message.nut:176`。
- `<dialogue>`：`message.nut:197`。
- `sysload`：`saveload.nut:264`。
- `select`：`select.nut:4`。
- `map`：`select.nut:262`。
- `staffroll`：`staff_roll.nut:65`。
- `title`：`system.nut:16`。
- `route`：`system.nut:17`。
- `room`：`x_rooms.nut:10`。

### TD3（11 个有效字面量注册）

- `config`：`config.nut:259`。
- `chapter`：`message.nut:152`。
- `date`：`message.nut:176`。
- `<dialogue>`：`message.nut:197`。
- `sysload`：`saveload.nut:264`。
- `select`：`select.nut:4`。
- `map`：`select.nut:262`。
- `staffroll`：`staff_roll.nut:65`。
- `title`：`system.nut:16`。
- `route`：`system.nut:17`。
- `room`：`x_rooms.nut:10`。

### TD4（13 个有效字面量注册）

- `config`：`config.nut:262`。
- `chapter`：`message.nut:154`。
- `date`：`message.nut:178`。
- `<dialogue>`：`message.nut:199`。
- `sysload`：`saveload.nut:264`。
- `select`：`select.nut:4`。
- `map`：`select.nut:147`。
- `staffroll`：`staff_roll.nut:99`。
- `title`：`system.nut:16`。
- `route`：`system.nut:17`。
- `room`：`x_rooms.nut:10`。
- `CompleteCG`：`x_rooms.nut:137`。
- `CompleteScene`：`x_rooms.nut:256`。

### ED（16 个有效字面量注册）

- `config`：`config.nut:328`。
- `@`：`macro.nut:47`。
- `chapter`：`message.nut:144`。
- `date`：`message.nut:168`。
- `DemoMode`：`message.nut:190`。
- `<dialogue>`：`message.nut:200`。
- `sysload`：`saveload.nut:325`。
- `select`：`select.nut:4`。
- `map`：`select.nut:133`。
- `choice`：`select.nut:154`。
- `staffroll`：`staff_roll.nut:67`。
- `route`：`system.nut:16`。
- `quit`：`system.nut:120`。
- `room`：`x_rooms.nut:9`。
- `CompleteCG`：`x_rooms.nut:140`。
- `CompleteScene`：`x_rooms.nut:271`。

### sorakoi（18 个字面量注册）

- `config`：`config.nut:425`。
- `sysload`：`config.nut:426`。
- `@`：`macro.nut:77`。
- `climax`：`message.nut:214`。
- `date`：`message.nut:244`。
- `DemoMode`：`message.nut:305`。
- `delay`：`message.nut:316`。
- `<dialogue>`：`message.nut:325`。
- `continue`：`saveload.nut:135`。
- `select`：`select.nut:4`。
- `choice`：`select.nut:96`。
- `staffroll`：`staff_roll.nut:147`。
- `route`：`system.nut:18`。
- `quit`：`system.nut:122`。
- `title`：`title.nut:4`。
- `room`：`x_rooms.nut:8`。
- `CompleteCG`：`x_rooms.nut:217`。
- `CompleteScene`：`x_rooms.nut:330`。

## Story 路径已接入或有明确宿主实现

- **共用原函数分派**：`story.rs` 的命令匹配在现有专用分支之外，使用 `has_system_handler` 检查当前 VM 的真实注册，再交 `start_system_command`。`system_command.rs` / `host/system_command.nut` 调用原 `com[name](args[,rows])`；即时 null 返回完成 Machine 请求，协程 wait 则保留请求和稳定 token，等待显式计时或输入续跑。poll 不自行推进。非 null 返回尚不隐式转成场景分支。
- **绘制和宿主状态**：桥保留 Timer、wait/event/InSkip 和原 Screen/Sprite 操作，区分重绘场景的 Base 与捕获当前已呈现屏幕的 Screen。当前屏幕可包含上一消息文字覆盖层。无 mask Trans、原 Fill 量化、持久画布与快照已有原生证据和实现；ClearImage、SwitchAuto、EnableDelay 作为明确动作交回 Story，原 sound 请求进入共用音频通路。不是继续使用 headless 的空操作代替这些行为。详见 [系统过场契约](System_Transition_Semantics.md)。
- `route`、`CompleteCG`、`CompleteScene`：执行当前 VM 的原函数和变量写入，再更新标签状态。Complete 两命令仅在相应原系统实际注册时成立。
- `title`：原标签更新函数与带 rows 的原标题菜单形态需区分；bootstrap/标题会话有对应菜单呈现路径。不能因为普通标签调用成功，宣称任意 Story 中的原菜单函数及其所有目的地均已闭环。
- `@`：继续由 `story/mapped_actor.rs` 执行原 `com["@"]`，捕获 `ImageAction` 展开结果交给共用图层实现，支持范围仍受已实现展开操作约束。
- `<dialogue>`：`message_adapter.rs` 执行原参数解析、语音判定和 Glyph 布局；新消息层还从排除注释后的原消息函数捕获有效前钩子和装饰绘制语句。Story 消费当前 MsgBox 的实际 Sprite/锚点、原 UpdateClimax/DrawDate 调用及 DelayAction 对应状态。原完整 `com[""]` 事件循环仍由 Story 的消息调度替代，不能将已接入钩子扩称为其中任意 GUI/输入分支均原样执行。详见 [消息对象与钩子](System_Message_Layers.md)。
- `select`：保留宿主选项效果和显式分支确认，Machine 管理 `.case/.end`；未直接运行原 `com["select"]` 的整个 UI 函数。
- `staffroll`：保留独立 Credits/HostEffect 管线；其支持形态与通用执行原 `staff_roll.nut` 全函数仍需区分。
- `map`：Story 当前只专门处理 title 资源中的 `map title [fade]` 返回标题。bootstrap 可调用原 map 函数并取返回值；不能把该路径推广成任意 Story map 均已实现。
- `.system`：通过共用 `load_system_module` 在当前 VM、变量和资源层执行模块，并安装相应消息适配器。普通注册函数现可进入共用 Story 调度；函数内部若调用尚未覆盖的宿主 API，仍须按实际错误和证据判断支持范围。
- `.load` / `.call`：由 `nekopack-script/src/runtime.rs:717–735` 内部处理，前者替换当前脚本帧、后者压入调用帧。它们不是缺失的 `com["load"]`，六作上述系统注册也没有 `load` 键。应与 `sysload` 的读档 UI、sorakoi `continue` 分开。

## 原命令语义及本轮覆盖状态

### date

TD1–TD4（message.nut:176，TD4 为 :178）及 ED（:168）原函数在 InRoute 时取得 `Images("date")`，设置 `DateImage` 并重置 `DateTimer`。后续 `DrawDate(s)` 虽由原消息循环调用（ED :174/:337），但该函数第一条有效语句就是 `return;`（ED :175，TD4 :185）。应保留此原行为，不能删除早返回来制造日期动画。新共用消息层只执行原有效调用，返回为空时不生成覆盖层。

sorakoi `message.nut:244` 是另一种原脚本实现：更新 StoryTitle/Caption，组合 eyecatch 图层，ClearImage，使用 TransBegin/TransEnd/TransSet、RenderTrans、Timer、wait/event/InSkip 和 Screen.Draw/Fill/Update 完成可跳过的计时呈现。本轮已由共用协程桥承载该原函数和无 mask 过场，未按游戏重写日期动画。

本地原归档未找到 `system/eyecatch.mng`；原 Images 对缺失整档返回空集合的行为已有原生证据，生产资源层据此处理缺失，坏档和歧义仍报错。本轮用于验证图层、时间边界及输入的隔离 date fixture 额外提供纯合成 eyecatch MNG，执行原 date 代码。这不证明原剧情已实际走到该命令，也不构成原素材画面一致性证据。

### box

复核注释边界后，TD1–TD4 `message.nut:32`、ED `:22` 的这段赋值都位于 `/* */` 中，不能列作有效注册。原注册元数据已排除这些条目。当前消息消费者已读取真实 `MsgBox` 对象的 Sprite 句柄、锚点、frame 画序及有名条件，取代固定文件推断；六作原对象的桌面内存渲染契约已验证。若其他实际系统模块注册 box，函数执行后的对象可由下一消息页消费；本次不替注释代码强行注册 box，也不将合成对象变更测试说成原游戏存在有效 box 调用。

### chapter

TD1–TD4（:152，TD4 :154）和 ED（:144）的原函数第一条有效语句都为立即 `return;`。共用 Story 分派现执行该原函数并正常完成，保留其空操作语义；后面的章节动画不可达，不作为待补的可见功能。

### climax

仅上述 sorakoi 注册：`message.nut:214` 根据可选参数设置或清零 ClimaxCount。该 setter 现由共用桥执行；消息消费者在原有效前钩子位置执行 `UpdateClimax`（:223/:410），每页一次，在绘制位置按原 `Config.Climax` 条件消费 `ClimaxImage`（:461）。恢复时将 InRestore 传给原函数，重绘不再次推进计数。计数、Sprite 合成和最终覆盖层均已接入，范围不再只是 setter 成功。Save 8 还保存、恢复经原 SaveInfo 契约确认的 ClimaxCount。

### DemoMode

ED `message.nut:190` 和 sorakoi `:305`：原函数设置 Config.KeepAuto，调用 SwitchAuto，并置 DemoMode=true。现由共用桥执行这些原变量写入，并将 SwitchAuto 请求交给 Story 自动播放状态；不是根据命令名在 Rust 中复制一套 setter。原完整消息循环的其他分支仍按现有消息调度覆盖范围判断，不由这条状态桥推定全部等价。

### delay

sorakoi `message.nut:316` 原注册接受可选参数：写 DelayTime（无参为 0）并调用 EnableDelay；消息循环在 :441–442 根据计时器调用 Screen.DelayAction。本轮已移除“仅无参 fade 0”的简化：共用桥执行原函数，Story 保留延迟前已呈现画面，并消费 EnableDelay/DelayAction 对应时序。非零时长按严格 `elapsed > DelayTime` 判断；零值和 skip 路径按文字完成、语音完成条件释放，点击/NEXT 可提交目标状态。等待期间继续请求 page_frame，关闭 Effect 不跳过等待。延迟画面、时钟和提交状态进入当前存档格式。

本结论针对已证明的 sorakoi 系统覆盖函数；其他原包中的 `.delay` 可能走原生实现，本审计没有把它们的 ABI 直接等同于此函数。

### choice

ED 与空恋均有原 `choice(args,list)` 注册。本轮已按原闭包参数数量接入通用分支契约：包含隐式 this 的参数数大于 2 才收集 rows 并开启 `.case/.end`；空 rows 仍为分支；普通单 args 函数返回值被忽略。数字返回按原 sq_getinteger 转为整数，非数字为 0；支持多个 case 常量、空默认、首次匹配、无匹配跳 end 与嵌套。证据见 [System_Command_Branches.md](System_Command_Branches.md)。

设备验证使用原 ViewDialog 加合成注册函数/选项，确认点击后原调用栈返回 2 并进入第二 case；不代表已经执行 ED/空恋原 choice 选项绘制全函数。此前对 ED 162 个剧情条目的仅命令扫描未找到 `.choice` 调用，不能人为声称原剧情触发过。

### config / sysload / room / quit / continue

这些名字有原注册，现也存在普通原函数的共用转交路径。后续已验证原 ViewDialog 在同一个 SystemCommand 协程内接收触摸、完成最后 wait 并返回调用者，见 [System_Gui_Primitives.md](System_Gui_Primitives.md)。但原 `com config` → ViewConfig及三个嵌套子页 → SaveConfig/安卓保存ACK已在原system资源与合成剧情入口上设备验证，参见 [验收记录](../research/shared-original-config-verification.json)。此为共用执行能力的实际调用证据，不是五款完整设置矩阵；ViewLoad、房间和退出的任意嵌套路径仍未得到通用闭环证明。标题目的地扫描、设置页、存读档 UI、鉴赏房、退出/继续按钮各自已有独立宿主实现，既不能列为整项功能不存在，也不能据独立 UI 可操作推定任意原 `com` 内的同名调用均已可恢复、可回传结果。

### 保存恢复

`story/saves.rs` 当前写出 NKSV/NKSC 版本 8，读取兼容版本 1–8。版本 8 包含此前的延迟场景画面/时间状态，并保存已证的 ClimaxCount，恢复消息页时原钩子不重复推进计数。此范围不是任意 Squirrel 全局对象或协程栈的自动序列化；仍在 wait 中的系统命令禁止保存。不能把 Save 8 描述为任意 GUI/命令中途均可断点恢复。

## msgfilter 的证据边界

本次六作系统模块的字面量注册清单没有 `msgfilter`，已读系统代码与运行时命令调用点也未定位此名字。没有原注册/调用证据，不能将它列为“已证实遗漏的原 com 命令”，也不能声称已支持它。若后续取得原生注册、动态别名或另一系统模块的证据，再确定它属于原生命令、普通函数还是脚本命令；不要将现有 `.filter` 图像效果自动视为同义实现。

## 剩余范围与验收边界

共用 `com[name]` 协程、消息对象/钩子消费者及已证保存字段已实施。后续应集中补齐：原 choice 控件完整绘制；JPEG mask 与未知转场参数；其他原函数进入 sysload/continue/room/quit 的嵌套宿主 API、等待与结果回传，以及尚未覆盖的GUI原语。未验证的绘制形态和宿主 API 应明确失败，不能静默套用无 mask 转场或独立 UI 作为替代。

原注册/源码证据、合成资源隔离测试、原资源契约测试和设备实际调用证据继续分开记录。尤其本轮原 date 代码配合成 eyecatch 的隔离验证不代表原剧情调用；六作原 MsgBox 元数据与内存渲染回归也不代表所有原消息事件分支均已设备验证。最终 APK、设备结果和同包回归由单独验收报告记录，本清单不重复其结论。
