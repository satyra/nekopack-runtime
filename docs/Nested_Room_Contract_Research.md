# 同协程嵌套 room 的集成契约

原始研究只读 `sorakoi/x_rooms.nut`、`gui.nut` 等技术模块。实际原包没有独立 `room.nut`／`x_music.nut`：三个房间及音乐控制定义合并在 `x_rooms.nut`。未查看图片、剧情或原回想正文。共享引擎及 Android 已接入。最终 APK6 的原音乐房、CG／回想入口边界与合成 replay 启动、返回、重绘、关闭均已实测通过，见 [设备验证记录](../research/shared-native-load-room-verification.json)。

## 原调用链

`com["room"](args)` 按参数设置初始页（默认 CG），循环调用 `ViewRoomCG`、`ViewRoomScene` 或 `ViewRoomMusic`。子页返回另一个页 ID 时原循环继续；返回 0 时 room 返回原调用者。音乐页退出后，原 room 恢复进入前的 `Config.VolBGM`，调用 `RestoreVolume()` 和 `PlayTitleBGM()`。

音乐页首次 `StopMusic()`；使用原 `CheckButton`／`VolumeBar`／`Spots`／`Glyph` 创建控件和条目；每个 wait 循环做 Update、绘制、IsPlayMusic 检测，把 VolumeBar 值写回 Config 后 RestoreVolume。选曲 `PlayMusic(name,false)`；播放按钮选当前或下一合法条目；停止清空当前条目；NEXT/BACK 切曲。非循环歌曲结束后用 Timer 最多等待 3000ms，再按 repeat 状态选曲。每步必须向 `SystemRoomMusicState(bool)` 同步实际播放状态，不能把已提交播放等同于仍在播放。

CG 子页点击条目后调用 `ImageAction("1 white")`、`Screen().Commit()`，再逐页 `ImageAction("0 #"+name+"&"+page)`、`Screen.Render()/Update()`。回想选择依次 ScriptReset、ResetRoute、AssignImage、StopMusic、设置变量、ScriptEval。原栈必须在对应 native 动作处等待，不能先继续绘图／返回再补做动作。

## 当前共享实现与动作协议

`host/system_room.nut` 的 `SystemRoomBindings()` 合并入现有 `SystemCommandStep` 临时 root bindings，不创建另一条 RoomInstall UI 线程。`story/native_room.rs` 消费原动作，`StorySession` 保留原 SystemCommand、按钮及 token，Android 在 VM worker 接收真实资源输出与 ACK。

通过 `SystemCommandHost.actions` 输出 `U` 行，顺序保持原调用顺序：

- `room_music_play\t<name UTF-8 hex>\t<loop 0|1>\t<track volume 0..100>`
- `room_music_stop`
- `room_music_volume\t<track volume 0..100>`
- `room_image\t<original action hex>`
- `room_commit`
- `room_assign_image\t<name hex>`
- `room_script_reset`
- `room_replay\t<scene name hex>`

`RestoreVolume` 保持 `restore_audio` 协议，更新 Config 类别音量；曲目音量独立保留。Play 从真实 `sound/bgm/<name>.ogg` 读取并校验 OggS，经有序音频队列交给平台播放器，输出 loop、曲目音量、类别音量和资源 bytes。Stop／Volume 作用于实际播放器；播放进度／结束状态再回送原 IsPlayMusic。独立 title room 的 P/M/V 音频管线仍是已有实现，不能以它可运行代替同协程 room 的实际验收；当前同协程路径已有独立设备证据。

CG 的 ImageAction／Commit 已落到实际 Scene。宿主在 native 图像边界刷新当前 scene snapshot 后才 ACK，原 Render 随后获得更新场景；不是只记动作名称。设备夹具把原列表首条 CG／缩略图替换为程序生成颜色，保留原菜单、条目、按钮与浏览代码。此项证明原 CG 调用与显示／返回链，不声称已查看原 CG 内容。

## 控件、绘图与不可变 surface

初次审计记录的前置缺口是：is_enable/is_rect 与原 Spot 字段不匹配；Glyph 只支持旧22px；带尺寸 Sprite.Draw 缺裁剪；sized Fill 被拒绝；Commit/Render 尚未绑定真实场景。它们是历史发现，不能继续表述为生产 dispatch 尚未集成。

当前共用层支持原 Spot 状态、原 GUI 类，以及 G 字体图层和 J 带尺寸复合层。G 内部序列为 `["G",text,x,y,size,color,bold,border]`；原音乐房的标题／信息字号、颜色和粗体由原脚本提供。Sprite 拷贝和带尺寸 Draw 保留尺寸、位置及裁剪，Fill／透明度操作不能回写此前已经绘入另一个目标的内容。ID_HIDE／ID_NEXT／ID_BACK／ID_RESET 继续使用原已加载常量，不与资源条目 ID 混用。

每帧绘制结果使用不可变像素 surface 保留：Rust 侧共享 `Rc<RgbaImage>`，VM 侧 V 图层引用带生命周期的 surface lease；不把上一帧仍被引用的像素原地改写。渲染完成后，仅当 current plan 与刚渲染的 frame 仍相同时，`SystemCommandRasterized` 才把该计划压缩成 surface 引用。这样原脚本在 Update 之后继续 Draw 的操作不会被错误丢弃，也不会让每次 Screen().Draw 不断嵌套旧计划。VM 弱引用与当前活跃引用共同决定旧 surface 回收，Rust 同步清理不再存活的缓存。

[桌面长循环报告](../.local/system-integration/nested-room-fixture/desktop-long-loop-verification.json) 对原音乐页运行 1201 帧：plan 长度保持 10，最多 2 个 Rust surface、1 个 VM lease，opaque_assets 为 0。该数据验证本夹具的有界保留，不是所有游戏／图像尺寸的性能保证。

## 回想入口与合成 replay 的验证边界

`ScriptEval` 发出 `room_replay` 后 suspend；普通 tick 不能把它当作回想已完成。保留两种明确路径：

- `room_metadata_only=true`：设备停在原 ScriptEval 动作边界，不请求加载回想资源。这条边界已在最终 APK6 通过。
- 生产 Android 路径：不设置该开关，调用 operation 82／`start_system_room_replay`，保留原 Machine、SystemCommand 及 VM host 状态，启动所选脚本；脚本完成后 `finish_system_room_replay` 恢复调用者并完成原 ScriptEval 等待，目标是返回同一 room token／原控件，再由原 room 关闭回到调用剧情。

本次生产路径验证只使用独立生成的 replay：builder 的 `--boundaries --synthetic-replay` 在原 LIST_SCENE 首条**技术名称**下打包 `.native_replay_probe\n`，setup 注册合成 Screen.Fill／Update／wait(250) 命令，随后 EOF。原回想正文既未读取也未打包；夹具不携带原 script.dat，manifest 保存生成脚本及归档摘要，打包后回读仅这段已知合成 bytes。原系统 archive 仍保持不变，缩略图／CG 是生成资源。

`verify-system-room-replay.py` 校验独立夹具目录清单、每个文件摘要和同一 APK 身份，只读按钮／动作／token／source/line：音乐→原 Scene 首条→合成 native_replay_probe 新 token→EOF 返回原 room token 和原控件→关闭回到 `story1:4`。它不传 metadata-only 开关，也不使用截图或原回想文本。

APK5 实测曾发现：合成 child 已结束，原 ViewRoomScene 在尾部 wait(-1) 等窗口消息，Android 没有重新唤醒它。现由 native_child_return_id/token 标明真实子页面返回；Android 去重后恢复调用方回想模式，只有原父 token 的无帧永久等待才补一次窗口消息 event5。ACK 优先，普通 poll/timer 不会越过等待，下一条命令也不会被误唤醒。[最终 APK6 合成 replay 报告](../research/verification/native-load-room/real-room-replay-final6.json) 为 `passed:true`：生产 op82 启动合成脚本，EOF 返回原 token、相同控件并重绘，CLOSE 继续合成调用方；平台回想模式进入/退出均已断言。

## 已取得的设备证据

- [APK6 原音乐房报告](../research/verification/native-load-room/real-room-final6.json)：`passed:true`。同一原 room 协程完成条目／NEXT／BACK；实际原 Ogg 摘要、非循环播放与进度匹配；原 VolumeBar 更新真实 gain；STOP 释放播放器；恢复原 title BGM 并返回合成调用者。
- [APK6 CG／回想入口边界报告](../research/verification/native-load-room/real-room-boundaries-final6.json)：`passed:true`。音乐→CG 保持原 token；ImageAction→Commit→ImageAction→native ACK→Render 实际显示生成像素；取消返回原 CG 页，再切 Scene；原 ScriptEval 在 metadata-only 边界挂起而未读回想资源。
- 纯合成 `tests/system_room.rs` 与共享引擎单元测试仍用于动作、资源、等待和保留栈契约；它们与上面的 Android 音频／控件／CG 证据各有范围，不能互相替代。

以上设备项目使用同一 APK，SHA256 `c42b9ff4098f4d3540c6c9e56a83d5e6ec2de1fcd0b2e585f3507e531c397135`。CG 使用生成的固色图，回想正文由 21 字节合成脚本代替；没有查看原图片、读取原回想正文，也不据此宣称原回想内容全部验证。
