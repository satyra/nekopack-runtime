# 原版启动所需的共用引擎语义

本文记录本地合法样本 `sorakoi.exe` 的静态证据与系统脚本契约。未执行附带 EXE/DLL；只读取系统 nut、启动脚本和资源标签/尺寸元数据，未查看游戏图、回想或剧情正文。实现不按游戏名分支。

## EnableDebugMode

原 EXE 在 `0x51ee54` 注册 `EnableDebugMode`，原生入口为 `0x57f4c8`。调用链为 `0x49473c → 0x462f04 → 0x7418cc`，最后跳至 IAT `0x7d0434` 的 `SHELL32.DLL!DragAcceptFiles`，实参为主窗口句柄及 TRUE。其意义是允许调试文件拖放，并不启用作弊，也不是 Squirrel 的 `sq_enabledebuginfo`。

可移植 host 记录 `Host.debug_file_drop=true`；设备文件导入由对应 UI 负责。

## Images 的空集合和缺失帧

原系统 `message.nut` 在类字段初始化时使用 `Images()`，并在构造器中访问 `NAME` 和 `WAIT#0` 等标签。该版本原版 `msgbox.mng` 的元数据只有 `BASE`、`*NAME`、`*TEXT`。这些可选图层的缺失是原版系统的正常契约。

原 EXE 的 `Images._get` 注册点在 `0x533840`；随后 `0x5349b7` 将实际实现 `0x584ecc` 写入方法分发表。该实现从 `0x585084` 开始先构造原生 `Sprite`，再按键解析和填充图像。未匹配的标签保留空 Sprite。共用 host 因此将空集合构造及缺失标签查询实现为空 Sprite，而不是任意忽略资源读取错误。集合自身不存在、解码失败等错误仍由资源边界报告。

Squirrel 2.2 的脚本表默认委托不提供新版的 `setdelegate` 方法；桥接层只增加 `NekoTableDelegate(table,delegate)` 来建立这一集合的 `_get` 语义，原生 API 校验两个实参均为表。

## Data 与语音声明

`x_rooms.nut` 使用 `data(name,"system","\t")`，TAB 列内的空格属于字段本身。默认 `data(name)` 保留已有空白切分；显式目录/分隔符通过通用资源请求传递，不能把 TAB 列改成所有空白切分。

旧系统的消息解析器声明 `local voice = IsAvailVoice(...)`；新系统使用 `local voice = VoiceVolume(m.mask)`，随后执行 `if(voice)` 决定是否播放。消息适配器复用系统自身的参数解析和原函数，以 Squirrel 真值转换导出布尔决定，保留零/正音量结果的含义。

## 原生标题控制语法

启动脚本通过 `.loop` 包围标题菜单；`.title` 只有后跟菜单行块及 `.case` 才构成菜单分支；旧版裸 `.title` 也可用于清空剧情标题，不能仅按有无参数判断。机器、标题目的地扫描与汉化行编码共用这一结构判定。公共机器保留 `title` 请求及行块，其分支选择及 `.end` 继续使用公共分支扫描。循环终点通过源代码中 `.loop` 与 `.end` 的词法配对回跳，不增加存档字段，也不清除外层分支边界。

标题中的 `? result = a | b | c` 使用声明变量的按位 OR。标题目的地从实际分支的 `.load`、`.config`、`.sysload`、`.continue`、`.room`、`.quit` 识别并导出元数据，不固定设置、读档或退出按钮编号。新标题通过独立协程执行系统自己的 `com["title"]`，以原来的 BG、logo、brand、BindR、动画计时和输入逻辑绘制并确认。`TitlePageInstall`/`TitlePageStep` 复用设置页绘制原语，逐步导出图元和命中区；每步恢复全局并保留独立协程状态，允许打开设置页后继续。原函数返回的 id 才完成机器待选分支。旧 `.map` 标题继续运行系统提供的真实 map 处理器。

## 启动过渡

原版启动代码包含 `.wipe WIPE002 1 fade`。标题初始化的无窗口 host 与已有 img/fade 一样保留 wipe 请求与时间元数据，最终绘制标题。正文交互的 wipe 仍走现有实际转场实现。

## 本地复现入口

`cargo run -p nekopack-runtime --example title_smoke -- [--translated] [--start] DAT [DAT_OR_PK2 ...]`

工具只输出启动成功、包字节数和可选首个正文包的种类；不会输出正文或保存游戏图。

## reset 与图像宏

`.reset` 入口 `0x4A7470` 释放六个音频槽、清九个图像/场景槽，并经 `0x4799E4 → 0x477C08` 清临时 flag、重应用音量；不重置脚本 PC 或 loop/call 控制栈。变量解析器 `0x47AED0` 的字符分支及 `0x47E87A–0x47E89E` 给出地址公式 `(2*local_flag+flag_suffix)*1000+index`：G、GF、L、LF 分别对应 0–999、1000–1999、2000–2999、3000–3999。reset 的 `memset(global+0x2058,0,8000)` 只清 L/LF。当前支持的 L bank 在 host 成功确认 `.reset` 后归零，G/GF 和别名保留。正文以单次通用 reset effect 请求设备停止音频和清显示。

旧图像宏由 `imagemap.txt` 提供；新系统 `macro.nut` 自己读取 `list_bg.txt`、`list_act.txt`、`list_sg.txt`，构建 BG_MAP、ACTOR_MAP、ACTOR_DEF，再由真实 `com["@"]` 生成 ImageAction。没有旧映射表时必须存在脚本宏处理器，不能凭文件名替代旧表或静默跳过图像。

新背景宏发出 `ImageAction("1 "+BG_MAP[id])`。人物从槽 2 起，生成 `#container&body&face&extras`。共用映射器沿用原脚本的展开结果，将同容器各部分合成；旧 `#container` 加 SG 表情及跨容器附件仍保留原路径。保存结构继续使用已有 containers/expression/extras 字段。

## 验证边界

新增合成测试覆盖：空 Images 与缺失可选帧、VoiceVolume 的零/正值决定、循环与嵌套菜单边界、循环内 checkpoint 恢复、reset 确认前后 L/G/GF 区别、标题协程状态隔离和透明度截断、脚本背景展开及同容器 BODY/FACE 合成。这里只证明这些契约，不表示新引擎的所有动作、音效宏、回想、分支或完整剧情均已覆盖。

LF 变量尚未开放为公共地址语法；当前只清已支持的 L bank。新的背景展开目前只接受单资源名，额外背景修饰符仍明确拒绝。既有映射器对运动符号的限制继续保留；未经核对的语法不自动映射成默认动作。

## 原快捷菜单

旧快捷菜单系统以 `SIDEMENU` 指定顺序，使用 `SLOTn` 相对几何及 `* BASE`。新 `shortcut.nut` 没有 `SIDEMENU`；真实 `ShortCutIcons.Init` 用 `Spots(Images("shortcut"))` 并按 `SHORTCUTS` 做 `BindR`，实际资源只有 `=save`、`+save` 等紧凑标签，不承诺 HOTSPOT、SLOT、disabled 帧或全部动作都存在。将其套进旧 SLOT 格式会错误要求不存在的 `* BASE`。

共用 Spots 支持紧凑及空格分隔的前缀。对于没有 SIDEMENU 的脚本契约，侧栏执行原 `ShortCut.Update`，查询它实际绑定的 spot、启用/选中状态与图像 ordinal。运行时仅在调用边界提供真实快速存档、路由、语音、自动及跳过状态，并随后恢复临时函数绑定。绘制和命中区来自真实 spot；没有独立激活区时使用这些命中区的包围范围，保留 `KeepIcon`。

侧栏支持 SAVE/LOAD/QSAVE/QLOAD/AUTO/SKIP/LOG/CONFIG/VOICE/RESET/HIDE/QUIT，并已接通原脚本的 NEXT/BACK：NEXT 执行到下个选择，BACK 恢复选项确认之前的快照，不能替换成翻页或历史记录。细节见 `Sorakoi_Navigation_Semantics.md`。合成测试覆盖原 Update 的决定、紧凑标签、禁用与未支持按钮、拖出取消及函数绑定恢复。`title_smoke --side` 可在开始后复现侧栏并继续推进到首个正文页，输出仅为元数据。

## 对话框字体加载

新版原 `MessageBox` 读取 BASE、可选 NAME 装饰框、*NAME 与 *TEXT；原 msgbox 元数据只有 BASE/*NAME/*TEXT。原 Images 对缺失装饰帧返回空 Sprite，不能把 FRAME 强制当作必要资源。共享 BASE 布局现允许 FRAME 装饰缺失，保留旧版存在 FRAME 时的合成，同时仍要求 BASE 与两个文本锚点唯一有效。

消息排版现由挂载的原 `message.nut` 导出：执行其参数解析以及 `Glyph` 初始化、姓名 Print 和正文 Build 设置，记录姓名文本装饰、字号、坐标、正文宽度、动态 size 与 LineSpan。空恋的默认正文/姓名字号 30、姓名括号与 `[0,0]` 坐标，以及旧五款默认 22 和原字节长度居中，均来自各自原函数，不根据游戏名或画布大小选择样式。姓名长度仍按该页源编码计算；绘制使用 Unicode。保存恢复和关闭设置后的重新排版使用同一路径。

验证只读取行数、字号、锚点、字形原点和裁切统计，不显示或导出剧情图片。六份原系统代码以合成文字验证默认、bigbig/big/small/数值字号；实际字体另做内存栅格检查。它们证明已覆盖的布局契约，不等于已复原消息处理器中的全部动画或整部游戏。

`text_render::tests::local_original_font_reaches_first_rendered_page` 可使用 `NEKO_FONT_GAME` 和 `NEKO_TEST_FONT` 本地运行：安装实际字体、处理首批音频事件、渲染首个真实正文页，仅在内存校验 PNG，不保存或显示剧情/图像。合成几何测试覆盖 BASE-only、可选 FRAME、缺失必要锚点及重复装饰帧。
