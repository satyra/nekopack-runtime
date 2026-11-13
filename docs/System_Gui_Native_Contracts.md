# 原生系统 GUI 契约（静态核验）

对象：`.local/sorakoi-import/game/ソラコイ/sorakoi.exe`，SHA-256
`9a30908aedf0f10b41ebf1a22c16f48eee840f1d949d2f7b7d8394de20a86d2f`。
以下均为 PE 虚拟地址；只读取 EXE 代码与技术 `gui.nut` / `config.nut`，未执行原 EXE，未读取图片、剧情或回想。

复核命令：

```powershell
python research/inspect_original_gui_native.py --exe .local/sorakoi-import/game/ソラコイ/sorakoi.exe --verify
```

该脚本也支持 `--strings` 与 `--range 58255c:426`（VA:字节数，均十六进制）。任意截取地址可能落在指令中部；解释应从本文已确认的函数入口/指令地址开始。

## Draw 重载

`Sprite.Draw` 注册 native wrapper 为 `0x58255C`（注册 push 在 `0x52E9CC`）。
`Screen.Draw` 通过通用 wrapper `0x57FAD4` 分发，方法表实体在 `0x5267C9` 写为 `0x57F544`。不能把注册之前的方法表赋值当作当前方法：方法注册后的长段字符串表代码才写入当前实体。

调用参数不含接收对象：

- `Draw(source, x=0, y=0, opacity=255)`：原有短重载；source 锚点参与原有定位流程。
- `Draw(source, x, y, width, height, opacity=255)`：固定 source 起点 `(0,0)` 的矩形拷贝；x/y 是绝对目标坐标，**不加 source.x/y**；width/height 是复制像素范围，**不是缩放尺寸**。
- 原生另有更长重载，显式传 sourceX/sourceY；当前技术 GUI 没有必要为此扩展接口。

证据：`0x582615` 比较 VM 参数数 5（包含 this），大于 5 后进入中重载；`0x5826C7` 比较 7；`0x5827C8` 调用 `0x694208`。Screen 对应 `0x57F5A3`、`0x57F652`、`0x57F74D`，终端函数相同。`0x694208` 将 sourceX/sourceY 都压入 0 后调用 `0x6945F8`。后者逐行以源/目标原始宽度步进，调用 `0x694BB4`，没有缩放采样。

`0x693FC0` 是共享裁剪器。五参调用可等价实现为：初始 sx=sy=0；如果 x<0，w+=x、sx-=x、x=0，y 同理；然后 w=min(w,targetWidth-x,sourceWidth-sx)，h 同理；w<=0 或 h<=0 时不绘制。不会重复或拉伸 source 填满超出的宽高。

`0x694BB4` 中 opacity<=0 直接返回，opacity>=255 走全强度源 alpha 合成，1..254 走与源 alpha 相乘的整数路径。这里存在必须保留的 native 标量/双像素差异，已在 `crates/nekopack-runtime/src/system_command/blit.rs` 独立实现：

- 所有 RGBA 通道（包括 alpha）使用 `dst + floor((src-dst)*a/256)`；不是除 255，也不是浮点 straight-alpha 合成，负差向负无穷取整。
- opacity>=255 的标量路径：sourceAlpha=255 时直接复制完整像素，否则 a=sourceAlpha。双像素路径只有两像素都 opaque 才直接整对复制；混合 pair 中 opaque 像素仍以 a=255 插值，因此可能留一级差。
- 1..254 的双像素路径使用 `a=floor(sourceAlpha*opacity/256)`（`0x694C7B–0x694C89`）。标量路径使用 `a=high32(packedBGRA*opacity)`（`0x694CE0–0x694CEA`），因此 source RGB 低 24 位能使 a 多 1；BGRA 字节打包为 `A<<24 | R<<16 | G<<8 | B`。
- 每行 full-opacity 核先保留 4 个尾部像素，partial-opacity 核保留 2 个。若总数大于相应尾部阈值且目标像素地址 bit2 为 1，先标量处理一个；然后在当前位置小于 `end-tail` 时每次处理两个，剩余走标量。注意比较为严格小于，最后实际尾部个数可能比阈值少一个。
- native buffer 使用 VirtualAlloc 对齐；`0x693F60` 的地址为 `base+4*(y*width+x)`，`0x694685–0x694696` 的行步长为 `width*4`，无额外 pitch padding。因此 bit2 用 `(y*targetWidth+x)&1` 模拟，不能按 Rust Vec 的实际分配地址决定。奇数宽度的相邻行会切换此对齐分支。

静态代码无法执行原 EXE 做动态对照，本实现的验证边界是指令分支与合成数据断言；未宣称已运行原二进制逐像素 oracle。

原 `gui.nut` 中 BarH 的 `s.Draw(m_bar1,m_bar1.x+x,m_bar1.y+y,w+10,m_bar1.height)` 已自行加 source 锚点，这也与绝对目标坐标一致。测试须用非零 source.x/y，避免错误的重复偏移被零坐标掩盖。

## Screen.Snap 与 Sprite.Snap

原生有两个独立的全局 surface：当前绘制 surface `0x74BA74`，保存的 snapshot `0x74BA84`。

- `Screen.Snap()` 将当前 surface 深拷贝到全局 snapshot，返回无值；不清空当前画面。
- `Sprite.Snap()` 将上一次保存的全局 snapshot 深拷贝到该 Sprite，返回无值。它**不直接捕获当前 screen**。
- 拷贝是独立像素存储；后续 Screen.Draw、Screen.Snap、其他 Sprite 修改都不得改变旧 Sprite 快照。

证据：Screen 方法表 `0x52B047 → 0x57FA8C`，`0x57FA8F → 0x48412C`，后者 push current、push snapshot、调用 `0x68D34C`。Sprite 注册 `0x52EB10 → 0x583218`，`0x583292` push snapshot、`0x583298` 调用相同拷贝函数。该函数调用 `0x68C818`，它在 `0x68C8C7` 分配独立像素缓冲并于 `0x68C8E1` memcpy。此拷贝更换目标的像素缓冲成员，不是将整个 Sprite 含 x/y 的结构赋值。

技术 `config.nut` 的 Screen.Snap 后进入子页面、子页面 base.Snap 的调用顺序依赖此区别。宿主用不可变图层记录实现时，同样应区分 current 与 saved snapshot，并复制记录，避免别名。

初始状态：snapshot 是进程全局，不能因新建一个 `com` 协程而自动清除。`0x483EF3` 检查其 buffer，无 buffer 才在 `0x483EFD–0x483F0E` 分配 1600×900、4 bytes/pixel surface；`0x68C6FA → 0x624FF0 → 0x62501D` 使用新提交的 VirtualAlloc 零页，所以初始像素为透明全零。此尺寸是当前原 EXE 的常量，不是所有标题的通用尺寸。

## Spots.SwitchMode

`SwitchMode(enabled=true)` 是设置模式，**不是每次翻转状态，也不改变 selected**。默认构造为 false（`0x66E1F9`）。注册通用 wrapper `0x58E5EC`，真正实体在 `0x54A84C` 写为 `0x58CC10`；该实体可选读取 bool，省略时设 1（`0x58CC40`），归一化后写对象 `+0x35`（`0x58CC50`）。

开启模式后的实际行为：

- 已 selected 项仍可命中，因此复选按钮可再次点击取消；普通模式会跳过 selected 项。`0x673CC0–0x673CCC` 对模式和 selected(`spot+0xE`)分支。
- 只依据 selected 绘制 active(`spot+0xB0`)或 normal(`spot+0x38`)；hover 不覆盖复选状态。证据 `0x673E9D–0x673EC5`。
- 跳过普通 hover 背景和 popup 绘制路径（`0x673DF5`、`0x673F28`、`0x67405A`）。不可见/禁用处理仍存在。
- selected 由技术 `CheckButton.Update` 自己通过 `ActiveAll` 维护；SwitchMode 不承担自动选中、反选或单选组管理。

原生 `Check` 额外差异：遍历集合遇到第一个有效像素命中即返回（`0x673D74`），而不是选择最小区域；还检测 hit mask alpha（`0x673D44–0x673D55`，某 spot 标志可绕过）。本契约记录这一差异，但不要求本次借 SwitchMode 扩大到完整命中系统改造。

## 合成测试建议

无需真实游戏素材；用生成的彩色像素或纯图层记录验证：

1. 一个带非零锚点的源，分别调用短重载与五参重载：后者 x/y 不叠加锚点，宽高截取左上像素，边缘不被缩放。
2. 负目标 x/y 同时前移 source 起点；超过源尺寸只画剩余像素；0/负 width/height 无输出；部分透明 source 与 0/128/255 opacity。
3. current=A，Screen.Snap；current=B，Sprite.Snap 应得到 A；之后 Screen.Snap=B，旧 Sprite 仍保持 A；Sprite.Draw 修改不会污染 saved snapshot。
4. SwitchMode() 连续调用两次仍开启；SwitchMode(false) 恢复普通模式；已勾选项在 switch 模式可 Check 并取消，普通模式跳过；hover 不覆盖开启模式的 active/normal；禁用项不可命中。

## 原生事件常量

`gui.nut` 滑条 `++` / `--` 需要原生端点常量。以下是原 EXE root table 注册值，均为名称 push 后将 integer push 给 `0x69AF20`、再由 `0x69D0E4` 创建 slot；不是宿主可任意重编号的控件 ID。

- `ID_OK = 0xFF01`（65281）：字符串 `0x75D524`，名称 push `0x51F080`，数值 push `0x51F092`。
- `ID_DRAG = 0xFF02`（65282）：字符串 `0x75D52A`，名称 push `0x51F124`，数值 push `0x51F136`。
- `ID_CANCEL = 0xFF03`（65283）：字符串 `0x75D532`，名称 push `0x51F1C8`，数值 push `0x51F1DA`。
- `ID_NEXT = 0xFF0A`（65290）：字符串 `0x75D56E`，名称 push `0x51F644`，数值 push `0x51F656`。
- `ID_BACK = 0xFF0B`（65291）：字符串 `0x75D576`，名称 push `0x51F6EE`，数值 push `0x51F6FE`。
- `ID_NEXT_END = 0xFF0C`（65292）：字符串 `0x75D57E`，名称 push `0x51F79C`，数值 push `0x51F7AC`，newslot 调用 `0x51F7C8`。
- `ID_BACK_END = 0xFF0D`（65293）：字符串 `0x75D58A`，名称 push `0x51F84A`，数值 push `0x51F85A`，newslot 调用 `0x51F876`。

`--verify` 同时核对这 7 组字符串内容、名称地址与数值 push。

## wait 的 Windows 消息契约

`wait` 注册位置 `0x51E798` 指向 native wrapper `0x57786C`，不是原引擎 coroutine suspend。它取默认整数 0，于 `0x5778C2–0x5778C5` 调用 `0x483F28(delay, true)`，并将返回值作为脚本整数 0/1 返回。

`0x483F28` 的步骤：

1. 如果全局退出标记 `0x7482EC` 已设置，返回 0。
2. 任意负 delay 归一化为 `0xFFFFFFFF`（`0x483F57–0x483F62`）。清除消息处理标记并将脚本 event `0x7A6B94` 清零（`0x483F69–0x483F72`）。
3. delay 非零时调用 `MsgWaitForMultipleObjects(0,NULL,FALSE,timeout,0x4BF)`（`0x483F7C–0x483F88`，import thunk `0x741978 → USER32!MsgWaitForMultipleObjects`）。delay=0 跳过此阻塞步骤。
4. `PeekMessageA(...,0)` 检查队列，`GetMessageA` 取消息，处理 accelerator 后 `TranslateMessage/DispatchMessageA`，重复到队列空（`0x483F8F–0x483FF7`）。GetMessage返回0则设置退出并返回0。
5. 更新鼠标坐标，返回1（`0x484055`）。脚本 event 不一定非零。

因此 `wait(-1)` 是无超时的 Windows 消息等待，**不是仅等 Confirm/Cancel/Drag，也不是固定每帧唤醒**。等待掩码 `0x4BF` 包括 input、posted、paint、timer 等类别。`wait(0)` 不阻塞，但同样泵消息并清除旧 event；正 delay 的等待既可到期，也可被任何匹配消息提前唤醒，不能将其无条件等同于不可打断的 sleep。

原窗口派发 `WM_LBUTTONDOWN=0x201 → 0x463865` 才将 event 写为 `ID_OK`。无按键 mousemove、mouseup 等消息可唤醒消息等待而不写游戏 event；队列排空后 event 仍为0。具体证据为 `0x462FF4–0x462FFA` 的消息分支，以及 `0x463865` 写 `0xFF01`。`WM_MOUSEMOVE` 只有带左键的情况才写 `ID_DRAG`（`0x46384A–0x463860`）。

对嵌套设置页的影响：子页确认关闭后父页再次 `wait(-1)`，原系统可因随后的鼠标释放、移动或重绘消息继续；仅允许非零游戏 event 唤醒的宿主会漏掉这类 message-only wake，造成父页未及时重画。跨平台适配应区分“新的 UI/消息循环唤醒，游戏 event=0”和“只读取状态的 poll”。静态证据**没有证明原程序固定每个计时 tick 都唤醒**：该 EXE 静态导入表不含 SetTimer/timeSetEvent，wait 本身也未创建周期定时器；不能据此将所有 elapsed 更新解释为原生的下一帧通知。

## 共用运行时接入

五/六参 Draw 进入 `system_command/blit.rs`，使用原目标绝对坐标、源(0,0)裁片以及标量/成对/尾部整数核。普通已有绘制路径不因此被宣称全部达到原生逐像素等价。Screen.Snap保存当前面，Sprite.Snap读取保存面；快照跨com保留，初始零面。命令内保留不可变绘制计划，不逐帧累积整屏位图，导出的最新快照供下一com使用；未新增其冷存档序列化。

原两个滑块端点常量已接入；其他既有可移植输入标识仍由宿主映射，不将它们混称为全部原ABI常量。
