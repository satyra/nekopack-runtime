# sysload / continue 原嵌套读档契约

范围：原始研究读取 Sorakoi 的 saveload/system/config/message 技术脚本及 EXE 静态指令；后续补充 TD1～4、ED 的原 SaveInfo 技术脚本和共用引擎。未执行原 EXE，未查看图片、剧情或回想正文。真实读档已接入共享 Rust 引擎与 Android，并在最终 APK6 重跑通过。音乐、CG 动作和合成 replay 返回也已用同一 APK 验证，汇总见 [设备验证记录](../research/shared-native-load-room-verification.json)。

## 原调用顺序

- `com.sysload(args)` → `ViewLoad()` → `ViewSavePage(ID_LOAD)`。原页创建 `Images("saveload")`、背景 Sprite、原 `GUI` / `RadioButton`、动态 `Spots`；`NEWEST_SAVE` 决定初始页；`wait(0)` 后进入输入／绘制循环。八槽×十页。
- 每槽创建 `SaveInfo()`，`file(SaveName(id),"rb")` → `info.Get(f)`。原脚本捕获该槽读取异常；`sdate + stime` 非空才允许读档。它不是以文件存在为唯一可用标准。
- 点击可用槽 → `ViewDialog("LOAD")`。取消只回到原槽位循环。确认 → `NormalLoad(id)` → `sound("OK")` → `LoadData(SaveName(id))`。
- `LoadData`：打开文件 → `SaveInfo()` → `info.Load(f)` → 写 `StoryTitle` / `StoryRoute` → `UpdateCaption()` → `LoadState(f)`。成功返回 true 后执行 `Sleep(1000)`，随后 **`throw null` 退出旧嵌套栈**。返回 false 时已更新元数据／全局标签，函数正常返回。
- `com.continue(args)`：`EnumFiles(SavePath(""),"save*.sav")`，空列表直接返回；非空 `LoadData(SavePath(list[0]))`。这一路没有 LOAD 确认，也不经过 NormalLoad 的 OK 音效，不包含 qsave。
- `CheckNewestData()` 使用相同列表首项的 `slice(4,7).tointeger()` 写 `NEWEST_SAVE`；普通保存 `NormalSave(id)` 直接更新该值。初始默认 0。仅 `NEWEST_SAVE` 更新不足以实现 continue，后者再次枚举。
- `InvokeCommand(ID_LOAD)` 包装 `ViewLoad()`，只在正常返回时继续 `CancelAuto()` / `CancelSkip()` / `wait(0)`；成功读取的 null 展开不能被当成普通 return 然后继续旧剧情命令。

## 文件和序列化边界

Sorakoi `SaveInfo.Save` 的顺序：缩略图，sdate，stime，route，title，text，MessageLog，ClimaxCount，SkipBackData；随后 `SaveData` 调用原生 SaveState。

`SaveInfo.Get` 只消费缩略图及前五个标量到 text，不能改变 MessageLog / ClimaxCount / SkipBackData。`SaveInfo.Load` 消费全部九个字段，其中后三项直接写回全局。`GetSaveThumb` 则 `file(...,"rb")` → `Sprite(f)`，需要与 `thumb.LoadFile(f)` 相同的首项消费行为。文件句柄必须保留独立游标，不能让每次 `Load(f)` 重读首项。

桥接时保留以上原函数，由文件适配器把平台 NKSV 记录投影为原 `file` / `Load` / `Sprite.LoadFile` 数据流。不要声称 NKSV 是原版 .sav 二进制兼容。原生 `LoadState` 才是替换 Rust Machine、场景、音频、历史及系统状态的边界。

## EnumFiles 的 EXE 证据

同一 PE SHA-256：`9a30908aedf0f10b41ebf1a22c16f48eee840f1d949d2f7b7d8394de20a86d2f`。

`0x51ee13` 注册 EnumFiles→`0x57e40c`；`0x57f2a6` 调用 `0x4e5c74` 枚举。WIN32_FIND_DATAA 基址 `[ebp-0x354]`，`0x4e5f05` / `0x4e5f11` 读取 +0x14/+0x18，即 ftLastWriteTime。树比较 `0x4e6cc1` 调用 CompareFileTime(new, existing)，`0x4e6cc8 setg`，较新节点放左侧；`0x4e67b5` 从最左开始，外层顺序 append 文件名。因此按最后写入时间降序，不按槽号、文件名或最后一次读档排序。时间完全相同的排序没有在此研究建立跨平台保证，不应虚构原版的名称次序。

复核：`python research/verify_sorakoi_save_enumeration.py --exe ".local/sorakoi-import/game/ソラコイ/sorakoi.exe"`。

## 当前实现：由原 writer 决定序列

生产入口为 `host/system_load.nut`、`src/story/native_load.rs`、`story/saves.rs`，通过现有 SystemCommand 执行原菜单／LoadData；不另启 SaveUi 代替原页。

原 `SaveInfo.Save` 的字段序列并不统一：

- TD1～4：缩略图 → route → stamp → title → MessageLog。
- ED：缩略图 → sdate → stime → route → title → text → MessageLog。
- Sorakoi：ED 的序列后再写 ClimaxCount、SkipBackData。

`Saved::native_values` 提供公共语义对象 `{fields,globals}`，包含日期、时间、合成 stamp、route/title/text 和真实历史／状态。`SystemLoadProject` 临时绑定 collector 的 `Save` 和缩略图 `SaveFile`，实例化原 `SaveInfo`，只给其实际存在的公共成员赋值，再**调用原 `SaveInfo.Save`**。原 writer 自己生成 metadata 数组的顺序及长度；不按游戏 ID 分派，不根据 Load 次数猜测布局。成功或异常后均恢复原 Save/Sprite/global 绑定；collector 深拷贝数组和表。原 Get/Load 随后读取独立游标，Get 不改变存活历史。

缩略图尺寸来自原 `SaveInfo().thumb.width/height`，不依赖部分游戏才有的 THUMB 常量：TD1～4／ED 为 128×72，Sorakoi 为 206×116。保存场景按这个实际尺寸重新缩放并缓存，`Sprite(f)`／LoadFile 消费同一首项；不会把 128×72 图像假报成 206×116。原 Alpha、Reduction、Fill、带尺寸 Draw 及绘制快照由共用 Sprite／渲染层处理，hover 与 normal 不共享后续可变绘制结果。

## 目录、ACK 与提交事务

1. 原 file/EnumFiles 首次需要目录时输出 `load_catalog` 并 suspend。若存在原 CheckNewestData，在原 handler 入口先运行它；真实 EnumFiles 的 lazy 请求因此发生在 sysload 计算初页之前，不能依赖早期 headless 初始化得到的空目录。
2. Android 在 VM worker 注册槽号、修改时间、日期／时间和 NKSV bytes，再安装 collector 投影及缩略图。普通平台槽 1～100 对应 `save001.sav`～`save100.sav`；quick 101～110 对应 qsave01～10。具体原 UI 展示范围仍由原脚本决定。mtime 使用固定 20 位十进制字符串，避免 32 位 Squirrel 丢失平台毫秒值。
3. `EnumFiles(...,"save*.sav")` 按修改时间降序返回普通槽，排除 qsave；同时间不新增原研究未确认的排序规则。损坏 NKSV 仍保留目录项，在原打开／校验边界显式失败，不把它静默抹除成“无档”。原槽页自己的 try/catch 可禁用损坏槽。
4. 原 LoadState 消费完原 metadata 后输出 `load_state` 并 suspend。`prepare_system_load` 用 `Saved::decode(...,true)` 校验摘要、DAT 身份、Machine、场景与音频，保留 prepared Saved，调用 `SystemLoadAcknowledge(id,true)`。目录与准备的宿主等待只由显式 native ACK 事件 6 恢复；普通 UI 输入／tick 不替代完成确认。
5. 原 LoadState 返回 true 后，原 `Sleep(1000)` 进入 SystemCommand 计时；时间到后原 `throw null` 展开嵌套菜单栈。只有先前成功准备的 load 标记才允许 `SystemLoadHandleExit(null)` 输出 `load_commit`，其他异常继续暴露。拒绝确认可返回 false，不发 commit。
6. StorySession 在正常 finish-system-command 之前分流 LoadCommit，核对 state_id 后直接 `apply_saved`，返回新 Machine 的 packet；不 complete／advance 旧 Machine。成功提交清理目录快照与临时准备数据；旧 token 不再有效。原 LOAD 对话框取消保留外层原页与原协程。

## NKSV9 的真实状态与迁移范围

NKSV 仍是平台格式，不与原 EXE `.sav`／SaveBlob 二进制兼容。NKSV9 在已有 Machine、场景、音频、History、ClimaxCount 等状态上增加原消息的 size/mask/可选 climax，以及真实 `navigation_back` NKSC 选择检查点。name/voice/text 来自实际消息，TD 使用的 face 来自已有 History.face；公共投影同时提供这些已保存语义，避免 TD 原日志访问缺失 face。

读取新存档会验证原消息与 Runtime History 的 name/voice/text 一致，验证每个 BACK 检查点；选择快照自身不嵌套保存整条返回链。`SkipBackData` 投影保持 `{pos,data}` 结构，pos 是对应检查点的历史长度，data 为完整 NKSC bytes 的稳定 `NKSC:<digest>` 不透明引用，完整 bytes 仍持久化在 NKSV 和 Rust 返回栈中；Rust BACK 已验证恢复选择状态、变量及历史。**原 Squirrel LoadBlob 尚不能直接读取该平台 opaque 表示**，不能据此宣称原 EXE blob 兼容或原 SkipBack 全函数已验收。

旧 NKSV1～8 继续可读。旧格式未保存的 size/mask/逐条 climax 不能恢复：当前迁移保留实际文本／语音／History.face，将缺失 size/mask 置为既定迁移默认值 30／空字符串，不声称还原原字体或角色 mask；未保存的 BACK 链为空，明确不可回退。NKSV8 已保存的 ClimaxCount 仍恢复。新旧格式均以真实 Saved 恢复 Machine，不能用空历史表冒充旧档中已经存在的历史。

## 初次审计记录（历史状态，非当前缺口）

以下保留当时发现，供追溯原绘图、文件和异常边界；这些项目后续已接入共用桥与生产 dispatch。

- `SystemCommandFile` 目前仅允许 config.sav / wb；缺 file rb、Load、LoadState、EnumFiles、Sleep。
- `SystemCommandSprite` 缺 `(width,height)` 与 `file` 构造、Clear、LoadFile、Alpha、Reduction；Fill 目前拒绝有尺寸 Sprite。原页顺序是 thumb.Alpha(SAMPLE) → hover.Draw(thumb) → thumb.Fill(white,100) → normal.Draw(thumb)，必须保留绘制时快照语义，后续 Fill 不能把 hover 已画入的缩略图一起改白。
- `SystemGuiSpot` 当前字段 enabled，与原动态槽位的 `is_enable` 不同；需要同一状态别名／原生属性语义。动态槽位还依赖 Sprite 拷贝保留尺寸和位置。
- `SystemGuiGlyph` 当前只接受白色22px，原槽号25px且 Bold，日期／正文18px。应扩展通用字体 plan，不替换原 Glyph.Print 或解析原页重写布局。
- 现有 `SystemCommandStep` 用 `failure=null` 同时表示无异常，不能区分 catch(null)；应分离 caught flag 与异常值，并用原生动作标记控制流退出。未经标记的异常不能静默吞掉。


## 已取得的验收证据与当前边界

- `cargo test -p nekopack-runtime --test system_load -- --include-ignored`：8 项通过，含 lazy 目录／ACK、独立文件游标、拒绝准备、原 sysload 确认取消，以及 TD1～4／ED／Sorakoi 六套真实 SaveInfo writer→Get→Load。该测试里的 GUI 替身仅用于契约断言。
- `cargo test -p nekopack-runtime --lib story::native_load::tests -- --include-ignored`：8 项通过，六套原 LoadData 路径真实恢复 NKSV；另覆盖原 Sorakoi continue、最新普通槽排除更新 qsave、1000ms/null/提交时序、旧 marker 不执行、旧 token 拒绝、损坏档、真实 BACK、NKSV8 迁移及实际缩略图尺寸。
- Android APK6 的 [真实读档报告](../research/verification/native-load-room/real-native-load-final6.json) 为 `passed:true`。使用独立存档目录，实际创建 NKSV、构造与槽号顺序不同的普通槽 mtime 及更新的 quick；已通过原 LOAD 确认取消保留原页、确认后真实恢复、原 1000ms 等待、continue 选择最新普通槽、无档 continue 返回。证据只包含槽、按钮、token、时间、source/line 和摘要，不读取剧情正文。

最终设备构建 SHA256 为 `c42b9ff4098f4d3540c6c9e56a83d5e6ec2de1fcd0b2e585f3507e531c397135`。原 LOAD / continue 的 warm、cold、empty 共 32 步通过；这些结果不等于原 Windows .sav 二进制兼容或全部游戏内容通关。

## 2026-11-14 长流程读回修复

实测从 NKSV5 旧档推进 200 页后生成 NKSV9，原版与汉化版均在标题冷读回失败。桌面真实样本复现原因为 `module source exceeds 1 MiB limit`：恢复时把完整 BACK checkpoint 编码进脚本源码。现在 VM 只接收不透明引用；MessageLog、BACK 与原生读档目录均按条目构建，不提高 VM 预算。Android 进入剧情前先检查原操作返回错误，避免后续 capability 查询以 `no story session` 掩盖根因。

新增超过 1 MiB 的长日志、单 BACK checkpoint 回归；检查连续两次保存读回后的真实 BACK、变量、历史和元数据，以及长目录投影。实际失败档也已在桌面恢复成功；最终 Android 原版和汉化版各 200 页、新档保存／冷读回及原 sysload／continue 32 步均通过，见 [长流程验证记录](../research/save-continuity-verification.json)。
