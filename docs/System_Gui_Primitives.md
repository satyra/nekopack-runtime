# 单协程 GUI 原语

`host/system_gui.nut` 只提供 Spot、Spots 和 Glyph，不创建线程、不替换原 GUI 或 Radio/VolumeBar，也不自动调用 `GUI.Get(Config)`。原 `ViewConfig → ViewDialog` 或 `ShortCutsWindow.Select` 可以继续保留在同一个 SystemCommand 协程栈中。

## Step 接口

先在拥有 `SystemCommandHost` 的同一 VM 加载新文件。每步安装宿主原语时合并 `SystemGuiBindings()` 返回的 `{Spot, Spots, Glyph}`，并在步结束或错误时与其他临时 root bindings 一起恢复。不要在原函数中通过 native `closure.call` 绕一层调用可 suspend 的处理器。

主调度器提供：

- `SystemCommandHost.x/y`：场景坐标；二者为 -1 时可按 Forcus 返回键盘焦点。绑定名保留原拼写 `Forcus`。
- `SystemCommandHost.buttons`：当前帧收集器。每个记录为 `[group, id, enabled, x, y, width, height, name]`，group 是命令内稳定的字符串，enabled 为 bool；对应 B 标识编码为 `group:name:id`。
- Screen.Update 在提交帧时捕获按钮记录；下一帧重新收集。按钮记录不取代原 Spots.Check 的脚本控制流。

Spots 使用现有设置/标题宿主已采用的矩形几何、首个命中区域优先、偏移与焦点行为，支持空/可迭代 Images、大小写无关 BindR、Enable/Active/Visible、GetPosH/V，以及动态 Add 和数字 ID 查询。原生复核后改为集合顺序首个命中；SwitchMode 允许已选项继续命中，采用 active/normal 绘制并抑制 hover。它不宣称实现任意逐像素 mask 或 popup 绘制；显式请求 popup 会报不支持。

Glyph.Print 追加现有 `['T', text, x, y, bold, border]` 绘制记录。Step 需用现有 T 协议序列化，Rust renderer 使用已安装宿主字体。当前已证子集为 22px 白色预览、可选黑边和 Bold；其他字号/颜色、多行 T 计划明确报错。后续已补五/六参数裁切 Draw 与跨命令 Screen/Sprite Snap，详见 [原生绘制契约](System_Gui_Native_Contracts.md)。

## 最小确认框夹具

`tests/fixtures/system_gui_dialog.nut` 是独立的合成调用探针，保留原 ViewDialog 的禁用确认、Screen 捕获/暗化、BindR、Forcus、wait 和布尔回传形态；不含游戏文字或图像。它的 com 包装器记录 DialogResult 和 AfterDialog，允许检查 bool 返回是否确实恢复原调用者。

合成 `dialog` 元数据采用 100×60 画布：BASE、CONFIRM 为全画布；`= OK`、`+ OK`、`* OK` 位于 `[10,20,30,20]`；`= CANCEL` 位于 `[60,20,30,20]`；`- OK` 与 OK 同区域。资源内容可用纯色生成，不需要商业素材。

预期流程：首次调用到 wait(0)，下一计时步提交按钮后 wait(-1)；触摸确认/取消或取消事件先产生 sound，再停于末尾 wait(0)，此时 AfterDialog 仍为 0；再续跑才将 bool 交回调用者。Config.Dialog.CONFIRM 为 false 时立即 true，且不请求 Images；AutoSelect 开启时无指针确认可以选择焦点。

独立测试位于 `tests/system_gui.rs`，覆盖以上三类退出、末尾 wait、root 绑定恢复、禁用确认、焦点、偏移几何、禁用/隐藏命中、原按钮元数据及 T 字形计划。所有资源和文字均为合成数据，测试不查看或导出图片，不操作 ADB。

## 原设置出口与持久化边界

SystemGuiSpots 使用与既有宿主一致的 `items` / `spot.enabled`；headless Sprite 明确提供 `avail`。原 `ShortCut.Init()` 产生的新实例在 root 绑定恢复后继续可由 SideMenuPlan 消费。空 mask Sprite 没有 asset 时使用 normal 命中区，不能把非 null 等同于有效资源。

原 `SaveConfig()` 继续执行其 `file(SavePath("config.sav"),"wb")` 与 `Save(f,Config)`。共用文件边界只承载已证实的配置写入；发出 save_config 后挂起，安卓提交 NKCF 偏好成功才发送 ConfigSaved ACK。此时普通按键或计时不能代替保存确认；失败直接报告，不把返回当成功。任意原 .sav 文件读写仍未实现。RestoreScreen 映射为现有安卓显示/输入配置应用；音频变化仍由原 Config 经公共音频管线应用。

NKCF13 在旧 1–12 基底外保存全十二个快捷键动作，保留旧格式读取。原 CheckList.Get 返回整型0/1；Dialog/VoiceMute 接受原 bool 或严格0/1，并对安卓输出明确的JSON布尔值。其他整数和类型仍拒绝。

事件桥按原 LButtonDown 触发确认、按住移动触发拖动、释放产生 event=0 的消息唤醒；键盘确认/返回后保留对应消息唤醒。无新消息的 poll 和计时检查不会解除 wait(-1)。无绘制的 wait(0) 先完成，再交付稍后到达的释放消息，避免把它吞在零等待里。输入队列保留按下/抬起顺序，只合并相邻拖动。

剩余：任意 popup/逐像素命中和其他 Glyph 样式、sysload/continue的原槽位读写与状态替换、room/quit嵌套动作仍不能从本轮设置路径推定全部覆盖。原 UI 调用入口与现有启动器独立 Settings 展示入口应分别验证。
