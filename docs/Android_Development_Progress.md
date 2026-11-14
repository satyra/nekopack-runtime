# NekoPack Android 开发进度

更新日期：2026-11-14

## 空恋旧档与连续剧情实测：修复长流程新档无法冷读回（2026-11-14）

用户选择先跳过真机，实际测试设备为模拟器 `127.0.0.1:5577`，本轮没有真机覆盖升级结论。未查看图片、剧情或回想正文，没有进入回想。

从原版、PK02 汉化版各自的真实 NKSV5 快捷存档开始，旧 APK 各推进 200 页后生成 NKSV9；两版均在标题冷读回时失败，Android 显示 `no story session`。真实 6,601,742 字节存档的桌面复现定位到 `module source exceeds 1 MiB limit`：完整 BACK checkpoint 被编码成巨大的 Squirrel 源码。修正共用运行时的恢复和读档目录构建，逐条注入 MessageLog/BACK 元数据，BACK 使用稳定不透明引用，完整 checkpoint 继续保存在 NKSV/Rust；没有抬高 VM 预算。Android 同时先检查进入剧情的原操作错误，避免 capability 查询掩盖根因。原生 LoadBlob 尚未接入这一已有边界不变。

修复后，使用同一 APK 完成：

- 两份此前失败的新档直接冷读回并各继续 3 页，不需要转换或重新开档。
- 原版 NKSV5 → 200 页、一次选择 → 新 NKSV9 → 冷启动读回；保存／恢复均为 `CS01_C_17:740`。
- 汉化版 NKSV5 → 200 页、一次选择 → 新 NKSV9 → 冷启动读回；保存／恢复均为 `CS01_C_18:69`。
- 两版均使用独立 save_probe，原存档目录测试前后全部文件摘要一致。
- 原 sysload／continue 的确认取消、实际恢复、最新普通档排除快捷档、冷启动和空目录共 32 步通过。

工作区 **383 通过、0 失败、27 默认跳过**；额外 native_load 与原技术脚本契约 18 项通过。新增长日志、大 BACK 检查点连续两次保存读回及实际 BACK 状态验证。源码 133 项摘要与构建时一致，设备 APK 和包内 native 库与构建记录一致。

本轮替代包为 `.local/android-probe/nekopack-save-restore-arm64-debug.apk`，SHA256 `33392575680ed8fc0ad6c71184b62263f82cd57a2a5080020402d860ff3ec51c`。此前 `c42b9ff4…` 为本次复现问题的旧包，不应继续作为最终交付包。完整记录见 [连续剧情与恢复验证](../research/save-continuity-verification.json)。本轮修改与验证记录已整理提交；这是两版各 200 页的实测，不是全部路线通关证明。

## sysload / continue 真实恢复与 room 平台动作完成（2026-11-13，历史轮次）

本轮沿原技术脚本接入共用运行时，没有按游戏名称分派。原 `SaveInfo.Save` 决定虚拟读档字段顺序，原缩略图实例决定尺寸；Android 真实目录映射普通槽和快捷槽，LoadState 准备成功后继续原 Sleep(1000)／null 展开，再提交真实 NKSV 恢复，旧协程 token 作废。NKSV9 保留原消息字段与实际 BACK 检查点，旧版本继续读取；不声称 Windows `.sav` 二进制兼容。

room 已接真实音乐资源、播放状态、曲目／类别音量、停止、标题曲恢复，以及 ImageAction／Commit／Render 的 native ACK。ScriptEval 保留原父协程，子脚本 EOF 返回原 token；修复实测发现的返回后 wait(-1) 不重绘，Android 只在真实子页面返回时发送一次窗口消息，并恢复平台回想模式。另修复原音乐页长期循环累积绘制计划导致 work budget exceeded：保留不可变光栅快照及弱引用租约，不增加预算。原音乐页 1,201 帧数值验证中计划恒 10 字节、Rust surface 最多 2、VM lease 最多 1。

**同一 APK 已在模拟器 `127.0.0.1:5577` 完成六组实际回归**：

- 原 sysload 确认取消／确认恢复、真实存档、1000ms 等待、continue 最新普通档排除更新快捷档、冷启动、无档返回，共 32 步。
- 原音乐房真实 Ogg 哈希、播放进度、NEXT/BACK、音量拖动、STOP 释放及退出恢复标题曲。
- 原 CG ImageAction→Commit→ACK→Render 与取消返回；生成固色素材，无原图检查。
- 原回想入口 metadata-only；另以 21 字节合成脚本走生产 op82、EOF、原父页恢复与重绘、CLOSE 返回，断言 Android 回想模式进入／退出；不读取原回想正文。
- 原 config 子页、保存、冷启动持久化及 DEFAULT 重置。
- 原 quit 取消／冷启动确认、VM 释放且无二次确认。

工作区测试 **380 通过、0 失败、27 默认跳过**；额外运行本地原 load 契约 8/8、真实恢复 8/8、含原 ViewRoomScene 的 native_room 5/5。设备安装包、最终 APK、包内各 native 库与构建输出哈希一致。最终调试包：`.local/android-probe/nekopack-native-load-room-arm64-debug.apk`，SHA256 `c42b9ff4098f4d3540c6c9e56a83d5e6ec2de1fcd0b2e585f3507e531c397135`。证据见 [本轮验证记录](../research/shared-native-load-room-verification.json)、[读档研究](Nested_Load_Contract_Research.md)、[room 研究](Nested_Room_Contract_Research.md)。原技术脚本与隔离合成入口保留区分，不据此宣称所有游戏内容通关。本轮源码、回归脚本及验证记录已整理提交；APK 仍作为本地构建产物保留，不入 Git。

## 原嵌套流程推进：quit 已接入，load / room 尚未完整集成（2026-11-13）

本轮没有按游戏 ID 分派，也没有查看游戏图片或回想正文。四项请求尚未全部完成，不能将下列契约测试算作四项 APK 实测。

- **已接入生产链路的 quit**：保留原 com.quit → InvokeCommand → ViewDialog 的嵌套栈、最后一次 wait 和取消分支。原 QuitGame 发出显式终止标记，只有与该标记对应的 null 展开才按正常退出处理；普通 null 和真实异常仍报错。Story 保留终止包，不完成旧 Machine 请求或推进后续剧情。Android 调用原确认之后的关闭入口，不重复确认。挂起系统协程不满足退出自动保存边界，避免 readiness 与实际序列化约束冲突。
- **共用绘图已补充**：带尺寸 Sprite、复合图层 J 的局部画布/裁剪/透明度、G 字体字号/颜色/粗体/边框。原22px白色 T路径保留。新增真实字体光栅及裁剪数值测试，不以查看图片验收。
- **sysload / continue 尚未接到生产加载链路**：新增独立虚拟文件模块、独立读取游标、修改时间降序枚举、LoadState准备确认及原Sleep→null展开契约。原EXE24条指令复核、6项测试通过（含本地原技术脚本；GUI与确认框使用记录型替身）。仍需Android真实存档目录投影、缩略图/原SaveInfo字段完整映射、准备/提交实际Saved及旧token作废，详见[契约研究](Nested_Load_Contract_Research.md)。不是原版.sav二进制兼容实现。
- **room 尚未接到生产嵌套链路**：新增同协程native模块及3项契约测试；尚未将动作解析、播放完成状态、原CG Commit/Render、回想边界接到Story/Android。现有独立鉴赏界面通过过的旧测试不能充当新嵌套流程验收。

本轮工作区回归：356通过、0失败、22默认跳过；额外本地原load契约含ignored运行6/6通过。最终APK为 `.local/android-probe/nekopack-arm64-probe.apk`，SHA256 `aaddd375f858515c5c237009d496f786f1c2526d13a4b0f4d3ee2ec7d989f225`；包内native与构建输出一致。同包原quit取消/确认冷启及原config三子页修改、冷启持久化、DEFAULT重置均通过，见 [验证记录](../research/shared-nested-flow-verification.json)。未提交或推送仓库。

## 原 config 嵌套设置、保存确认与十二槽快捷键（2026-11-13，最新轮）

本轮继续接共用运行时，没有按游戏 ID 分派。原场景 `.config` 调用现在沿原 ViewConfig → ViewConfigSound / ViewConfigDialog / ViewConfigShortcut → SaveConfig 栈执行，子页与快捷键弹出选择不另起设置协程。补齐五/六参数裁切 Draw、Screen/Sprite Snap、SwitchMode、滑块端点常量及无游戏按键的消息唤醒。原生证据脚本核对 65 条指令与 7 组常量；新增 blit 保留原标量、成对和尾部像素舍入规则。

实测中修复：ShortCut.Init 保留新 Spots 实例后公共字段不一致；旧 Sprite 缺少 avail；空 mask 被快捷栏误当有效命中区；原 CheckList 返回整数0/1被保存层拒绝；释放消息被无绘制 wait(0) 吞掉导致弹出列表返回后父页不刷新。输入队列现在保留按下/释放顺序，仅合并相邻拖动。

原 SaveConfig 仍执行 file/Save；共用配置文件边界发出保存请求并等待安卓提交成功 ACK，再返回调用者。NKCF13 完整保存 F1–F12，支持原 CONFIG/DELETE/SCREEN/MENU 动作，并读取旧1–12。此为便携配置持久化，不声称兼容任意原 `.sav` 文件。

测试 APK：`.local/android-probe/nekopack-original-config-arm64-debug.apk`，SHA256 `e7e6bfe854efcbd879291563130a4a4b56540157f64f209ff12627fbea6a0a18`。包内 native 库与构建输出哈希一致，详细记录见 [shared-original-config-verification.json](../research/shared-original-config-verification.json)。

- 工作区 **338 通过、0 失败、20 默认跳过**；涵盖保存 ACK、消息唤醒、快照跨com与有界保留、全十二槽与旧配置、SwitchMode及原裁切整数核。
- 同一 APK、模拟器 `127.0.0.1:5577`：挂载原 system.dat，使用合成剧情入口实际调用原 config。文字速度设73、BGM设41、HK角色静音切换与音量36、QSAVE确认开关、F1=AUTO/F2=LOG 均通过原控件修改；退出剧情、强停冷启动再退出设置，完整配置值一致。
- 原 DEFAULT 确认与 ResetConfig 实测恢复文字50、BGM75、F1=CONFIG/F2=VOICE、角色静音清零和确认开关默认值。
- 同包再次通过系统指令/原确认框/分支与delay冷读档、音频/BGV33项、空恋原版/PK02各三页。已有中文快速存档从 `CS01_C_17:231` 恢复并推进至`:239`，未覆盖用户存档。

以上设置实测是原场景 com config 链路，不等于现有启动器独立 Settings 展示入口已经全部改用此路径，也不等于五款安卓完整设置矩阵。任意 sysload/continue 的原槽位状态替换、room/quit 嵌套动作、像素级命中/popup及其他Glyph样式仍待接入。所有验证只读取配置、事件、坐标、音频状态及哈希；未看游戏图片或剧情/回想正文。未提交或推送本轮工作区。

## 原分支、共享遮罩与嵌套确认框（2026-11-13，后续轮）

继续修改共用引擎，无按游戏名称分派的实现。原闭包参数数决定是否收 rows 和启动分支，替代按命令名猜测；普通函数返回值忽略，分支函数数字返回走原整数语义，`.case` 支持多常量、默认、首个匹配及无匹配跳过。带 PNG mask 的 Trans 恢复硬/柔边逐通道公式、缺失沿用和快照，与 `.wipe` 共用同一个运行时遮罩表面。

同一 SystemCommand 协程新增 Spots/Forcus、Glyph 预览及按钮计划；安卓把实际场景坐标、拖动、确认与取消交给原脚本。原 ViewDialog 的等待与 bool 返回留在原调用栈，不以独立原生对话框代替。没有自动遍历 GUI.Get(Config) 改动原子页时序。

本轮 APK：`.local/android-probe/nekopack-system-branches-mask-arm64-debug.apk`，SHA256 `0efe2ad15a8ff3391431577612d2ad8a79ecb6b5488737022b2864e8a78ee21a`。包内 native 库与构建输出哈希一致；验收记录见 [shared-system-branches-verification.json](../research/shared-system-branches-verification.json)。

- 工作区测试 **325 通过、0 失败、20 默认跳过**；包括注册分支、两方向 Trans/Wipe 共用表面、逐通道像素和确认框调用栈测试。
- 同 APK、模拟器 `127.0.0.1:5577`：原 date 计时/确认、两次 wait、PNG mask 绘制路径、原 ViewDialog 触摸命中后返回第二 case、退出后的系统声、delay 快速存档冷恢复通过。资源为合成色块；执行的是挂载的原 date/ViewDialog 和独立合成注册分支，不冒称原 choice UI 全部走通。
- 空恋原版与 PK02 汉化各新游戏三页通过；音频/BGV 33 项与冷恢复通过。用户已有中文快速存档从 `CS01_C_17:231` 读到 `:239`，未覆盖该存档。

仍待补：原 choice 控件完整绘制、完整 ViewConfig 子页和退出持久化动作、sysload/continue 的槽位与状态替换、room/quit 嵌套闭环、五参数 Draw/Snap/SwitchMode 等已定位缺口。Spots 当前沿用既有矩形命中契约，不能当作任意像素 mask/popup 已完成；Glyph 仅现有白色 22px 预览子集。JPEG mask、负 Trans 时长仍拒绝；未新增任意遮罩表面或挂起 VM 栈的存档保证。技术证据见 [分支](System_Command_Branches.md)、[遮罩](System_Transition_Mask.md)、[GUI 原语](System_Gui_Primitives.md)。未查看游戏图像或剧情/回想正文，未提交或推送。

## 原系统指令、消息对象与音频状态接入（2026-11-13）

本轮继续修改共用运行时，没有新增按游戏 ID 选择实现的分支。补齐 LF 本地标志银行与 reset 确认边界、动态 `.system` 模块加载、原音乐房播放指令、从原 EXE ABI 提取的 BGV 双槽及语音抑制关系。音频按原配置族处理主音量、分类静音、MV 语音期间 BGM 音量与旧版 BGM 压低/恢复，修复 ED 原生内部 MuteBGM gate 的识别。

原注册 `com` 处理器通过保留协程接入：`wait()` 后继续执行原绘制、计时和确认逻辑；Screen 捕获与 Base 场景重绘分开，Screen 捕获包含上一消息实际文字层。按原像素规则实现无 mask Trans、Fill 和快照。`.delay` 不再伪装为零时长 fade，改为延迟场景提交，等待原阈值或语音完成，并支持 NEXT 与冷读档。安卓在文字已完整但 delay 尚未提交时继续计时，不强制重画未变文字。

消息渲染读取当前 VM 的 MsgBox 资源句柄、坐标与面板，而非固定猜测素材；执行原消息前钩子和有效绘制语句，保留被注释的 box 注册、旧 DrawDate 早返回等原行为。ReadedColor 使用原表达式与真实已读状态；未变化的覆盖层复用缓存。原 Images 整档缺失返回空集合，损坏或歧义资源仍报错。对应原语义和覆盖边界见 [System_Command_Coverage.md](System_Command_Coverage.md)、[System_Transition_Semantics.md](System_Transition_Semantics.md)、[System_Message_Layers.md](System_Message_Layers.md)。

配置 NKCF12 保留动态新增角色字典、现代音量/静音族及已读/覆盖层开关，修复冷启动导入丢弃角色键导致 BGV 音量归零。运行时存档新写 NKSV/NKSC8，继续读取 1–7，分别保留 BGV 标签、延迟场景和原 ClimaxCount 状态；不将挂起的 VM 协程截断成可读存档。

最终 APK：`.local/android-probe/nekopack-shared-systems-arm64-debug.apk`，SHA256 `4e73c065e405df9510ef48f42243b2eb1b8ca941d685a36eb524d5db333b4af8`。175 项构建输入前后哈希一致。验证记录见 [shared-system-integration-verification.json](../research/shared-system-integration-verification.json)。

- 工作区测试 310 通过、0 失败、20 默认跳过；五款旧作原版标题桌面回归 5/5，六作原 MsgBox/布局实资源回归通过。它们不代表五款安卓完整剧情回归。
- 同一 APK、同一模拟器 `127.0.0.1:5577`：音频/BGV 33 项及快速存档冷恢复通过；原音乐房第二曲播放、上下曲、循环、停止与退出释放通过。
- 原 date 处理器的自动返回、确认返回、两次输入等待、返回后的系统声音通过；未提交的 10000ms delay 保存、强停重启后在剩余约 8079ms 完成。date 使用独立合成剧情入口和合成色块资源，不能当作原剧情中存在该调用的证据。
- 空恋原版与 PK02 汉化版各从新游戏推进 3 页通过。另读取用户已有 NKSV5 中文快速存档，从 `CS01_C_17:231` 验证到 `:239`，未覆盖该快速存档。

实测期间只读取命令、坐标、播放事件、音量与哈希等元数据，未查看游戏图像或剧情/回想正文。仍未完成 choice/非 null 返回的场景分支协议、带 mask Trans，以及任意 com 深入 GUI/config/sysload/room 的通用闭环；已有独立设置/读档/鉴赏 UI 不能替代这些路径的证明。未提交或推送本轮工作区。

## 共用 WMV 正常播放与空恋进度恢复（2026-11-13）

共用视频层按魔数选择 MPEG 或 ASF，新增固定 FFmpeg 8.0.1 的 WMV/WMA 解码，直接读取现有 SAF 资源。未按游戏名或汉化版分支，未转码媒体。Android 以 AudioTrack 实际播放头驱动视频，采用有界解码/绘制队列、过期帧丢弃、真实 EOF 排空及后台暂停恢复；默认不转储视频帧或 PCM。架构、构建、许可证及资源计数边界见 [WMV_Runtime_Playback.md](WMV_Runtime_Playback.md)。

实际推进修复了共用链上的三个问题：存读档完成音效从当前 `saveload.nut` 的对应函数读取；存档页从原脚本和 MNG 读取每页槽位、分页、文字及缩略图锚点；原资源查无语音时按原引擎分支无声继续，已有资源的读取或解码错误仍报错。标题进入读档时停用原标题动画调度，并保留首个错误，避免次生回调覆盖根因。存档身份校验没有放宽；设备挂载顺序不同于桌面排序，检查点按设备实际清单重新生成并验证。

同一 ARM64 debug APK `ae898b3adc8ad9792627d14758a97eab9920b38d59e54d3884509a98585dfd61` 在专用 Android 9 模拟器 `127.0.0.1:5577` 完成：

- 原版和汉化 WMV 独立播放器全片验证，各解码 3387 帧；原版含 Home 后恢复。实际音频输出非零、播放头排空，耗时符合媒体时长。
- 从各自真实运行时电影前存档 `CS01_C_06:2040` 进入原剧情命令，分别播放 `op.wmv` / `op_chs.wmv`，自然结束返回 `CS01_C_07:19`。原版呈现 3386、丢弃 1 帧；汉化呈现 3384、丢弃 3 帧，最大已记录呈现时间差分别 162 / 152 ms。没有用 MegaSkip 或桌面电影 ACK 代替播放证明。
- 旧 MPEG 全片回归：3832 帧全部呈现、无丢帧、音频播放头排空。测试临时复制的媒体核对 SHA 后移除。
- 从此前 `CS01_C_16:955` 恢复，原版继续 20 页至 `CS01_C_16:1036`；汉化版继续 100 页至 `CS01_C_17:231`。两版原版存档页首尾分页分别为槽 1–8 / 73–80，快捷存档后强停冷启动读回，源脚本及行号均一致。临时导入槽已移除，快捷存档保留；设备最后停在汉化版恢复后的进度。未声称主线通关。

本轮没有查看游戏图片或剧情/回想正文，只读技术元数据。桌面普通 ACK 重建到此前进度的过程不是 Android 连续通关记录；两类验证明确区分。最新 `cargo test --workspace --lib` 为 243 passed / 15 ignored / 0 failed，另有本轮实际资源与存档页验证。195 个构建输入 SHA 与已测 APK 构建记录一致，交付 `.local/android-probe/nekopack-sorakoi-wmv-arm64-debug.apk` 与设备测试包 SHA 相同。

完整摘要及复验报告哈希见 [sorakoi-wmv-playback-verification.json](../research/sorakoi-wmv-playback-verification.json)。原版 WMA2 两次 PCM SHA 不同但长度、非零样本数、峰值一致，逐样本差异尚未测量；汉化 PCM SHA 一致。不能将此描述成原版音频逐样本一致或已确认音质问题。本轮仅模拟器实测，未冒充真机覆盖。

## 空恋原脚本排版与 NEXT / BACK（2026-11-13）

共用运行时现执行原 `message.nut` 的消息参数解析和 Glyph 初始化，导出姓名装饰、字号/坐标、正文动态字号、宽度与行距。空恋默认正文/姓名 30、LineSpan=8、姓名 `[0,0]` 和旧五款 22px/原字节长度居中分别由挂载脚本决定，没有游戏名分支。六份原消息代码的默认/大中小/数值字号与实际字体栅格合成测试通过。

NEXT 按原 MegaSkip 逻辑执行到下一选择；BACK 保存选择确认前的私有 NKSC 快照并恢复未完成的选择。实际跳转补齐共用的标签/条件 goto、整数表达式赋值、MNG 视觉组合与半透明混合、SoundHook 音频别名和无参 BGV 双通道停止。不可见转场不再构建中间 PNG；变量、资源名和最终场景仍正常更新。原 ViewDialog 只有显式 false 才免确认，结构化配置缺少 BACK 键时仍须确认，已修正宿主判断并验证取消不回退。

原 `.wipe` 数值 ABI 在六份原 EXE 中实际存在差异：旧五款整数毫秒，空恋浮点秒。运行时只读同目录 EXE，通过 RTTI、构造器数值分支与原生转换函数识别能力，不执行原程序、不使用游戏名或版本阈值猜测；秒制语义纳入存档身份。游戏库存档可读性检查与启动传递同一组语义输入。游戏目录应保留原 EXE。原 `.fade` 的进一步浮点差异记录为边界，未据此声称全部转场语义已统一。

最终同一 ARM64 APK 在专用 Android 9 / x86_64 + libnb 模拟器 `127.0.0.1:5577` 上完成原版和 PK02 汉化版各 35 页：前三页默认排版、NEXT 到 `CS01_C_16:844`、首次选择、BACK 取消保持当前页、再次 BACK 确认后恢复同一 source/line/scene/slots、重新选择和额外 30 页。两版均推进到 `CS01_C_16:955`，全部页面裁切计数为零；35 个对应位置的正文哈希全部不同，确认实际使用各自文本。只读取技术元数据与设备内计算的哈希，没有查看游戏图、剧情/选项正文或回想。

`cargo test --workspace --lib` 为 238 通过、13 按需忽略；另外执行六原 EXE 的 ABI 探测、六份原排版脚本及实际字体合成栅格测试。旧五款桌面标题启动 5/5 通过，不冒充本 APK 的五款全量安卓回归。测试器修正了菜单边缘缩放取整导致的误翻页，以及 `am -W -S` 返回后新进程暂未出现在 pidof 的观测窗口；探索性与确认框修正前的运行不计入最终两版结果。

交付文件：`.local/android-probe/nekopack-sorakoi-layout-navigation-arm64-debug.apk`，SHA-256 `df7b9a5ba593245055e50ec9a12583c593a24904b99c41f5a1ca0a1e1d970811`。APK 内 native 库与当前构建产物一致，现有签名验证通过；记录包含 160 个构建源码输入的 Git blob ID。完整结果见 [本轮验证记录](../research/sorakoi-layout-navigation-verification.json)，语义与限制见 [NEXT/BACK 记录](Sorakoi_Navigation_Semantics.md)，复现入口为 `platforms/android/regression/verify-sorakoi-layout-navigation.py`。

尚未通关。当前视频解码器仍为 MPEG-1/MP2，WMV 常规播放没有接入；本轮 NEXT 越过 OP，不能算影片播放验证。带参数 BGV、完整设置/鉴赏与所有消息动画也没有在本轮扩大为已完成。导航确认遵循原条件，但使用宿主弹窗，不是原 dialog.mng 图像复现。

## 空恋原版启动与 PK02 汉化加载（2026-11-13）

共用运行时新增 PK02 字节容器读取和资源覆盖，按原 DLL 的文件名优先、哈希别名其次、未命中才读 DAT 的顺序加载。已命中补丁的读取错误直接报告。原版模式排除 PK02，补丁参与版本／存档身份；Android 扫描及启动均纳入 `.pk2`。没有按游戏名分支，也没有执行随附 Windows EXE/DLL。

本轮修复原脚本暴露的共用契约：带行块与 `.case` 的标题协程、裸 `.title` 清空标题的区别、循环／reset、图像宏、紧凑快捷图标标签、结构化确认配置，以及消息框可选装饰帧。PK02 路径按原始字节精确匹配，不再破坏 CP932 双字节尾部的 `0x5C`。实现证据与边界见 [启动语义记录](Sorakoi_Runtime_Startup.md) 和 [PK02 格式记录](PK02_Format.md)。

专用 Android 9 模拟器 `127.0.0.1:5577` 在同一 APK 上完成：原版标题→开始→连续十页、PK02 汉化版标题→开始→连续十页；两版各强制停止后重新启动并推进三页。十页中八页的设备内文本哈希不同，两版重启后的前三页哈希各自一致，确认实际覆盖加载及冷启动稳定性。只传回哈希和运行元数据，未查看图像、正文或回想。正常游戏库扫描另确认识别一款游戏和 `sorakoi_chs.pk2`。

`cargo test --workspace` 通过；其中 runtime 147 通过／9 按需忽略，script 29 通过，archive 19 通过／1 按需忽略。按需执行真实 PK02 的全部 151 条目读取、101 个脚本解码／版本隔离、实际字体的首个正文页内存渲染均通过。旧五款原版标题桌面回归全部通过；这不替代五款在本 APK 上的完整 Android 回归。

验证包：`.local/android-probe/nekopack-pk02-arm64-debug.apk`，SHA-256 `665b9f94340093b9049cd8e8a08a2843beb3af3ff99a721f4ea0099ef9eb6038`。这是调试验证包，不替换下文的发布候选。回归脚本为 `platforms/android/regression/verify-pk02-startup.py`，可审计结果见 [本轮验证记录](../research/sorakoi-pk02-startup-verification.json)。

该启动批次尚未覆盖完整路线、鉴赏和全部设置；当时的姓名/正文布局与 NEXT/BACK 缺口已由上方后续批次继续处理。启动批次记录本身只证明启动与短段剧情，不宣称完整兼容。PK02 容器本身不规定文字编码，当前混合编码处理基于已验证的汉化来源契约；多 PK02 包尚未定义叠加顺序，明确拒绝自动猜测。

## 当前实现

- Android 启动器提供游戏库、最近游玩、搜索、导入和单游戏详情。通过系统目录选择器扫描，先列出识别的游戏与补丁，再确认导入。详情页管理游戏版本和存档，继续入口取决于可读存档。游戏内右侧抽屉按功能分组，存读档打开原版界面，独立快捷面板默认关闭。
- 共用运行时接入原版 `ViewConfigPage` 的协程、输入、绘制和确认流程。标题和剧情都能打开设置。逐角色语音、环境音量、文字样式等配置由运行时解释，Android 负责输出和持久化。
- 共用运行时已接入标题鉴赏入口、解锁状态和持久化，以及 DAT 补丁加载。游戏资料、用户存档、商业资源和诊断截图保留在本地。
- Android 原版设置拖动时合并排队的移动事件，保留按下、松手和取消的顺序。拖动帧使用原合成器输出的 RGBA，松手时提交最终设置。

## 验证记录

- BLESS 在安卓模拟器通过原始 DAT 完成一条主线。共 12,355 个对白页、60,323 个脚本步骤，返回标题并在重启后确认通关标记保留。运行条件和验收边界见 [剧情路线执行验收](Route_Execution_Progress.md)。
- 五款样本 BLESS、BIRTH、BRAVE、BLACK、Endless Dungeon 已在同一版 ARM64 APK 上验证设置与重启恢复，使用实际语音播放与静音、环境音增益和文字渲染数据核对。部分角色与 BIRTH 的环境音通过独立测试场景验证；BIRTH 的 KA 项此前借用 BLESS 录音，现已定位本作 EP02 原录音并完成同 APK 的播放／静音及重启补测。
- Tyranor 参考版在模拟器验证了横屏游戏库、最近游玩、搜索、详情、目录选择、导入及重启保留，竖屏验证了底部导航和搜索。游戏内验证了右侧抽屉、原版存档页、显示子菜单和独立快捷面板，打开菜单期间剧情位置保持一致。
- 滑块修复版通过 116 项运行时测试（另有 4 项忽略），完成 Android ARM64 构建。模拟器上约 0.85 秒的 Endless Dungeon 设置滚动拖动，从按下到呈现最终位置由约 5.5 秒降至约 1.3 秒。BGM 滑块最终值在强制停止并重启后保留；Endless Dungeon 设置确认、取消、返回剧情，以及 BLESS 设置翻页和滚动通过回归。

## 交付边界

发布准备补充（2026-11-12）：新增独立 release 构建、版本号、关闭调试及外部 MainActivity 入口，并将完整许可快照打入 APK；原创源码采用 GPL-3.0-only。共用运行时本轮未修改。沿用原证书，密钥重新加密并在仓库外备份。签名候选 `0.1.0-preview.1` 位于 `.local/android-release/nekopack-arm64-preview.apk`，SHA-256 为 `c44f940415b1685ce343f68a47380bb729e06fe2d594203ef3494f4e9f80fa78`。

该候选在 PGKM10／Android 15 覆盖安装成功，游戏库可见条目和最近游玩记录一致；`run-as` 及外部启动 MainActivity 被拒绝，游戏库内部启动 Endless Dungeon 原版标题、切后台返回、右侧菜单通过。包内 45 份许可／来源文件哈希一致。此轮是发布打包和正常入口冒烟测试，不等于重测五款中文剧情、存读档和全部设置；下述 `9f784e…` 的深度回归仍属于旧调试包。记录见 `research/android-preview-release-verification.json`，正式发布前完整核对项见 [发布指南](RELEASING.md)。没有创建公开 Release 或版本标签。

介绍视频为原创功能示意，生成工具位于 `tools/promo/`，成片 `.local/promo/nekopack-intro-zh.mp4` 为 77.67 秒、1080p／30fps、中文系统配音与字幕，全片解码及字幕安全区检查通过；不含游戏图片或正文。

最新本地 APK 位于 `.local/android-probe/nekopack-translated-speaker-fix-arm64.apk`，SHA-256 为 `9f784ea0063783643be6386a890bfb863e2eca5aba42b82af357637d13723a7c`。本版修复汉化姓名渲染被强制转成 CP932 导致中断的问题。129 项运行时测试通过、7 项忽略，另有真实字体／消息框的合成中文姓名渲染测试通过。五款汉化版的实际非 CP932 姓名页、存档重启读取、Bold 修改重绘与恢复均通过；五款原版另通过 5 次冷启动和 10 次后台恢复。

上一构建 `b9d41aa7…91e97cf6` 在 emulator-5572 完成五款 86 个控件、172 个播放／静音状态的语音矩阵，全部通过。每组原版设置修改后强制停止并重启，播放检查实际进度、非零输出增益与资源哈希，静音检查无语音请求；8 个测试进程及配置还原均正常结束。本轮未复现启动超时。完整结果见 `research/android-five-game-voice-matrix-final.json`，原始日志位于 `.local/android-probe/voice-matrix-source-adapter-final/`。该轮没有重测五款所有设置；此前设置和启动器验收保留各自构建的边界。BIRTH KA 的借用录音缺口已由同 APK 的原录音补测闭合；前一矩阵报告保留当时的缺口标记，补充证据见 `research/birth-original-voice-device-verification.json`。滑块延迟数据来自模拟器，真机帧率仍待测。本轮验证没有查看游戏图片或回想正文。

## 核心语义复核（2026-11-12）

共用运行时按原版 PE 的加载失败分支修正以下行为。

- 遮罩首次缺失时使用原版初始化为零的遮罩缓冲，后续缺失保留上次遮罩。存档的空遮罩名恢复为相同初始状态。
- 背景资源缺失时保留当前背景，删除特定脚本位置的资源名改写。损坏图片继续暴露解码错误。
- Data 缺失返回空数组，Ini 缺失返回空表，删除鉴赏分页文件名特例。非法路径、资源歧义和坏编码继续报错。

该阶段曾将角色语音开关统一改为按原始音频文件名判断，并加入 BIRTH XX/VE 互斥测试；这一统一规则已被下文的原始 message 解析与语音表达式执行取代，不能作为当前五款通用语义。资源代理调用全局编译函数改为显式根表查找，避免缺失资源返回空容器后把全局函数名再次当成资源查询。真实 VM 的现存与缺失资源集成测试及原版 BLESS 启动测试通过。

可复验原版证据见 research/verify_original_missing_mask.py、research/verify_original_missing_background.py、research/verify_original_missing_data.py 及对应 JSON。前两项核对 BRAVE 的原版二进制，Data/Ini 核对四个唯一二进制，覆盖五款样本（BIRTH 与 BLESS 的原版二进制相同）。本轮还重新运行即时 BGM 停止、默认增益、环境音、角色队列、括号取消、影片和返回标题的既有原版断言。

新增行为通过运行时测试，合计 122 项通过、4 项忽略。原版验证采用 PE 控制流、导入和初始化指令证据，设备验证采用状态、音频进度及文字渲染统计。

仍需独立核实汉化包指纹匹配、资源过滤和优先级调整。当前这些代码属于现有补丁兼容处理，尚不能视为原版资源加载规则已完整还原。

当时的汉化证据显示，Endless Dungeon 的 ed_chs.dll 钩住 CreateFileA/W，将 script.dat 和 system.dat 整档重定向到对应 _chs 文件；此前按资源提升优先级并过滤启动脚本／系统脚本的策略与此存在差异。该处“暂未修改”的历史边界已被下文整档重定向实现取代；原汉化启动器未在当前语料中找到，DLL 注入链仍未完整确认。

该阶段回归使用同一 ARM64 APK，SHA-256 为 06475ca6870957cdda54792ef86ac29efff10023bd5a1b350fbc92fde70551c0，产物为 .local/android-probe/nekopack-core-semantic-arm64.apk，现已由顶部新构建取代。BLESS、BIRTH、BRAVE、BLACK、Endless Dungeon 均通过原版设置修改与重启保留、既有存档读取、对白推进和实际语音播放进度检查。该构建每款使用一个实际语音样本验证正常播放路径；完整角色矩阵属于此前及该阶段中间构建的测试，未在此构建重跑全部角色。

BIRTH 的一次测试恢复阶段启动超时，进程退出且 crash buffer 为空，重新执行后通过。该事件未定位到确定原因，仍需后续长时间运行复测。

BIRTH XX/VE 专项在上述历史构建上通过两次重启复测：XX=1、VE=0 时目标语音实际播放，XX=0、VE=1 时目标语音静音。该结果对应当时按音频名判断的实现，已由后续原脚本语义修正取代，不代表当前实现的验收结论。测试完成后恢复最初的 BIRTH Bold=0 及其他该次改动的配置。

## 整档重定向与启动诊断补充

Endless Dungeon 汉化 DLL 的 CreateFileA/W 钩子已按整档重定向接入共用挂载机制。安卓游戏库扫描、存档可读性检查和游戏启动传入实际档名，命令行入口使用同一挂载接口。命中的 script.dat/system.dat 被对应 _chs 档完整替换，原档不会参与资源查找，也不会因自身损坏阻止有效替换档加载。替换目标缺失或损坏会明确失败。已删除这两个汉化档的启动脚本过滤、系统脚本过滤和人工 priority=1。

新增通用显式重定向 API 与具名挂载 API。原版模式继续挂载原版，汉化存档身份按实际生效档案计算；此前混合挂载产生的汉化存档可能无法通过新身份校验，当前未提供迁移。其他四款已有汉化补丁 profile 的行为保留。

共用混合编码解码器识别 .map 语法块，保留条件标识的 CP932 编码，避免不再过滤原始启动脚本后出现变量名乱码。中文 system/saveload 代码按已核对的混合编码读取。125 项常规运行时测试通过，另有本地真实 ED 档案的标题初始化测试通过。原 DLL 指令与档案控制代码证据见 research/ed-archive-redirect-evidence.json 及其验证脚本。

安卓模拟器已验证整档汉化标题、原版设置、首段剧情、新建存档、强制停止后读取同一存档，以及切换原版。临时汉化测试档已清除，原版目录恢复。

历史启动超时没有足够记录确认根因。设备仍保留 before/restart 会话，精确 restore 会话目录不存在；run_id 长度合法，原测试未保存 ActivityManager 启动返回值。12 次交替冷启动与完整 BIRTH 设置重启／恢复均成功。公共测试入口现检查启动退出码、Status 和进程存活，保存 stdout/stderr；未增加自动重试或固定延时，也未对无证据的生产生命周期做改动。详见 research/android-startup-timeout-diagnostic.json。

随后另行发现：测试隔离目录缺失时，目录 provider 会抛出 NPE。目录读取边界现提供重新授权或检查目录位置的错误提示，并保留异常栈。这是有独立证据的新问题，不能据此认定前述历史启动超时的原因。

## 原始 message 解析与语音表达式修正

原始脚本的语音开关表达式存在差异：BLESS、BIRTH、BRAVE、BLACK 调用 `IsAvailVoice(m.voice)`，Endless Dungeon 调用 `IsAvailVoice(m.mask)`。最新 `message_adapter` 执行各档原始 parser 与原始 voice 表达式，沿用脚本本身的数据关系，不按游戏名建立分支，也不再统一改为按音频文件名判断。五款原始 message 脚本的三项专项测试通过；运行时测试共 128 项通过、6 项忽略。

当时的 `b9d41aa7…91e97cf6` APK 已完成 Endless Dungeon 整档重定向及游戏库验证。五款矩阵全部通过：Endless Dungeon 18 个开关／36 个状态，BLESS 16／32，BIRTH 18／36，BRAVE 18／36，BLACK 16／32。每个测试进程开始前核对安装 APK 与锁定文件的 SHA-256，未混用构建。BIRTH KA 录音借用缺口已在下述补测闭合；旧混合中文存档身份不迁移的边界保持不变。


## BIRTH KA 原录音与连续启动／后台恢复复测（2026-11-12）

BIRTH 原版 `system.dat` 的 `script/EP02.txt` 中有 21 条连续 KA speaker 引用，和原版 `system/voicemap.txt` 的 KA 角色映射一致。原 `voice.dat` 存在 `KA_EP02_0000.ogg` 至 `KA_EP02_0020.ogg`；此前数字场景前缀扫描遗漏了 EP02。仅扫描命令、角色头和音频容器元数据，没有阅读剧情／回想正文或查看图片。

选择的原录音 `KA_EP02_0000.ogg` 为 34,055 字节、单声道 44,100 Hz、约 3.1465 秒，SHA-256 为 `a4ec2d6cc1bf1abff0089d82574d8789e6ecdffae5b456e01906b5823f8cf21e`。隔离场景替换为这条本作录音，仍使用当时的 `b9d41aa7…91e97cf6` APK；KA 开关关闭时无语音请求，开启时实际播放器前进、输出增益 0.79、播放字节 SHA 与原档一致，两组均在修改原版设置后强制停止并重启。FO/MY 同时重测通过，结束后恢复设置。原资源可用 `research/verify_birth_ka_original.py` 复验；结果见 `research/birth-ka-original-resource-evidence.json` 和 `research/birth-original-voice-device-verification.json`。未覆写旧矩阵的历史证据。

五款各完成 10 次进程冷启动、20 次 Home 后恢复，共 50 次启动、100 次恢复、150 次暂停采样，全部通过。每次 `am start -W -S` 后分别验证 session 与标题就绪，再进入有持续音频的原剧情页；BLESS 为第 13 页、BIRTH 为第 6 页，其余为第 1 页，选页按通用音频元数据进行。后台两次采样确认音频位置稳定，恢复确认相同 Activity／进程、相同音频 token／播放器、实际播放位置继续前进，且剧情坐标不变。最后一次恢复后再次暂停采样，证明最后一次恢复也实际前进。标题就绪范围为 2,281–8,750 ms。前后安装包哈希均为 `b9d41aa7…91e97cf6`，无需更换 APK。

新增 `platforms/android/verify_startup_lifecycle.py` 可重复执行这套测试。正式记录见 `research/android-startup-lifecycle-verification.json`；完整逐阶段数据位于 `.local/android-probe/startup-lifecycle-resumed.json`，其 SHA 收录于验证记录。

期间修复的是测试工具问题：初始元数据读取的 ADB 子 shell 等待 stdin 结束，改为 `sh -c`；正式批次通过 27 项后，Windows 报告文件被读取时拒绝原子替换，已用持有读句柄的最小案例复现 WinError 5，改为直接写报告并监控独立日志。原中断时 BRAVE 进程仍存活、标题已就绪、error.txt 不存在；接续明确保留原 27 条通过记录和故障记录，重新执行未完成项，没有自动重试掩盖应用失败。因此 50 项是两段正式执行合计，不能描述为工具全程无中断。

测试准备阶段模拟器曾全部退出、ADB 无在线设备，恢复原 LDPlayer9 实例后核对数据与 APK。该外部环境中断、两次工具故障均与旧 BIRTH 启动超时区分。此次未发现新的共用引擎故障，没有为未经复现的生命周期问题添加补丁；旧超时仍因原 ActivityManager 输出缺失而无法确认根因。本轮仅在 Android 9 模拟器验证，不代表真机／系统杀后台场景均已覆盖。

## 提交前 APK 与源码一致性核对

提交前使用当前源码重新运行 `platforms/android/build_probe.py`。新构建整包 SHA-256 为 `bd690fcf6445a40314d0154714e902441d5494e5d516a7be239f2d361f81c5ac`，与当时经过设备验证的 `b9d41aa7…91e97cf6` APK 比较，全部 17 个 ZIP 条目内容逐字节一致，包括 DEX、三个原生库、Android 资源与清单、许可证及包内签名文件。两包签名验证通过，证书 SHA-256 相同；部分 ZIP 条目时间戳不同，因此整包哈希不同。交付继续使用经过五款设备验证的 `nekopack-core-redirect-voice-arm64.apk`（`b9d41aa7…91e97cf6`）。

`research/android-final-apk-source-verification.json` 记录两个 APK 的逐条目哈希、签名证书和 146 个源码输入的 Git blob ID。复验脚本为 `platforms/android/verify_apk_match.py`，应先执行构建再比较；它不会自行构建。矩阵、原录音隔离场景、整档重定向与游戏库扫描的本轮脚本已整理到 `platforms/android/regression/`，所需外部资源和本地元数据见该目录 README；连续启动／恢复脚本保留在 `platforms/android/verify_startup_lifecycle.py`。游戏 DAT、原录音、用户存档、APK 和签名私钥不纳入 Git。


## 汉化姓名渲染中断修复（2026-11-12）

用户正常播放时报告 `speaker not encodable in active CP932 profile`。根因是 `DialogueRenderer::prepare` 为复原原脚本 `(name_width - name.len()*11)/2` 的居中计算，把已经解码成 Unicode 的所有姓名再次固定编码为 CP932。汉化的“姬”“红”“妮可”等姓名无法按该编码表示，因而在实际绘制前报错。此前原版语音矩阵和汉化首段检查未覆盖这些具体姓名，是测试覆盖缺口。

修复由 `DatGame` 根据当前页实际选中的脚本资源确定姓名行编码：已验证混合汉化脚本的姓名行使用 GB18030，原版仍使用 CP932。编码后的字节长度传给渲染器，Unicode 字形绘制保留，原姓名居中公式保留；首次显示和原版设置关闭后的样式重建使用同一路径。没有逐游戏分支，也没有用问号替换姓名、使用 UTF-8 长度或直接吞掉编码错误。独立 `dialogue_overlay` 比较工具增加显式编码参数。

新增 `platforms/android/regression/verify-translated-speaker.py`：实际添加汉化档、从标题推进到非 CP932 姓名页，验证绘制完成、存档、强停重启读取、Bold 切换重绘与恢复；五款全部通过。脚本在设备端过滤正文和 UI 无关字段，只读取姓名与页坐标；只清除本次新增补丁，测试存档使用隔离命名。最初恢复设置时遇到旧 presentation JSON 残留，已改为等待实际设置控件出现再读状态，未修改运行时来迁就测试。

结果见 `research/translated-speaker-render-verification.json`。原版回归在这一个新 APK 上完成五款各 1 次冷启动、2 次后台恢复；旧版的 86／172 语音矩阵和 50／100 稳定性记录仍属于旧版，不冒充本次全量重跑。新 APK 经当前源码再次构建，17 个包内文件与已测修复 APK 完全一致、签名证书一致，详见 `research/android-translated-speaker-apk-source-verification.json`。

用户随后确认问题出在第一部《黑白羽翼》（BLACK）中文版的新游戏，已在其连接的 PGKM10 真机（Android 15）覆盖安装同一 `9f784ea0…13723a7c` APK 实测。通过原 `system.dat` 哈希确认手机 `Documents/NekoPackValidation/3` 为 BLACK，临时加入对应 `patch202.dat` 后从标题新游戏推进至第 21 页 `P01:102`，实际汉化姓名正常渲染；存档、强停重启读取、Bold 修改重绘及恢复全部通过。使用隔离测试存档命名，未写入既有存档槽，结束移除本次临时补丁并再次核对已安装 APK 哈希。记录见 `research/translated-speaker-black-phone-verification.json`。设备脚本新增目录／SAF document 参数，推进点击使用实际横屏 stage 尺寸，避免真机 `wm size` 的竖屏物理尺寸误导。
