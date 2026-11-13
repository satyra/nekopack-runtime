# 原生系统过场与屏幕缓冲契约

2026-11-13 静态恢复。证据对象为 `.local/sorakoi-import/game/ソラコイ/sorakoi.exe` 的 x86 PE，以下地址均为该文件首选 ImageBase 下的 VA，仅用于复核，不可作为游戏 ID 或运行时固定地址。没有执行原 EXE、读取剧情正文、查看图片或播放音频。

原系统调用方：`.local/sorakoi-import/analysis/message.nut` 的 `com["date"]`（244–299 行）。本文确认无 mask 的淡入过场；未声称所有旧引擎已经具有相同 ABI。

## 缓冲与捕获

原引擎有持久的当前绘图缓冲 `0x74BA74`、过场开始快照 `0x7427B4`、过场结束快照 `0x7427A4`，以及独立的 Screen.Snap 快照 `0x74BA84`。这些是不同对象，不能把过场建模为交换两个仍会变化的图层引用。

- `TransBegin()`：桥 `0x57C7A0` → `0x4055B0`，调用 `copy(0x7427B4, 0x74BA74)`。
- `TransEnd()`：桥 `0x57C7B4` → `0x4055C8`，调用 `copy(0x7427A4, 0x74BA74)`。
- 两者只替换各自快照，不改变当前画布，不重置计时器、时长或模式。
- 深复制函数 `0x68C818` 分配新像素对象及缓冲，复制图像尺寸/步长元数据，在 `0x68C8CF–0x68C8E1` 经 `0x6EB280` 复制像素字节，再替换目标的引用计数对象。不是共享 source 像素。

## TransSet 与 RenderTrans

`TransSet` 的桥 `0x57C874` → `0x4055E0`，共享过场状态对象 `0x75E8B0`。参数读取的契约：

1. 参数 1 为整数 duration，缺失/整数读取失败时值为 0。
2. 参数 2 为可选 mask 字符串。空 mask 采用 mode 1；非空采用 mode 2；参数 3 不区分大小写等于 `fade` 时为 mode 3。
3. 状态 `+0x0C = mode`、`+0x14 = duration`。读取 `Config.Effect`（键 `0x7424F1`，读取器 `0x5757A4`），为 false 时将 duration 改为 0。
4. 调 `0x625078` 重置状态 `+0x10` 计时器。该函数与 elapsed 读取器 `0x62508C` 都调用 `0x7419F6` → IAT `0x7D05CC` → `WINMM.DLL!timeGetTime`。**duration 单位为毫秒**，不能套用其他命令的秒制。

`RenderTrans` 注册桥 `0x57C7C8`：若第一个用户参数为 Squirrel integer（type `0x05000002`），将其直接作为 p 调 `0x405660`；否则调 `0x40562C` 使用计时器：

```text
p = duration != 0 ? trunc((elapsed_ms << 8) / duration) : 256
```

核心 `0x405660` 写入当前绘图缓冲 `0x74BA74`：

```text
p <= 0:   current = copy(begin)
p >= 255: current = copy(end)
1..254, mode 1:
  for each byte i:
    current[i] = (begin[i] * (255 - p) + end[i] * p) >> 8
```

混合函数 `0x40130C`，MMX `pmullw`/`paddw`/`psrlw 8` 为直接证据。所有像素字节均参与，包括第四通道；分母为 256，权重和为 255。不能用浮点 `/255` 或平台默认 source-over 混合代替。端点分支为精确复制，不做中间帧的取整暗化。显式负 p 和大于 255 的 p 由端点分支处理，没有先写回 clamp 后的参数。

原 native 核心在 p < 255 时返回 AL=1，p >= 255 时返回 AL=0；**Squirrel 桥丢弃该 bool，返回 0 个脚本值**。过场自然达到结束不会隐式重新捕获快照。脚本后续 Draw/Fill 按调用顺序叠加在 RenderTrans 写出的画布上。

mode 2 跳至 `0x401384`，mode 3 跳至 `0x4013FC`，mask 加载为 `0x405410`。后续已恢复 PNG mask 加载及 mode 2/3，并与 .wipe 共享持久表面；逐通道公式、缺失沿用及验证见 [System_Transition_Mask.md](System_Transition_Mask.md)。JPEG 仍明确拒绝。

## Sprite 与 Screen 的不同语义

`Sprite.Screen()` 的注册桥是 `0x581DEC`（注册名 `0x75DB79` / `0x75DC05`）。`0x581E66–0x581E71` 直接深复制当前缓冲 `0x74BA74` 到 Sprite 自身。捕获后当前屏幕变化不能改变该 Sprite。

`Sprite.Base()` 的桥 `0x58314C`（注册名 `0x75DBF7`）：若自身未分配像素，先用 `0x68C660` 创建 4 字节、1600×900 图像；调用 `0x5FD0E0` 准备原场景状态，再调用 `0x60992C(sprite)` 合成：

1. 通过 `0x68CB24`/`0x68C9D0` 获取长度/像素指针，`0x6EB483` 将目标像素清零。
2. `0x5D04D4(0x76620C, sprite)` 绘制第一固定场景层。
3. `0x5D12F0(sprite)` 遍历 `0x7685B8` 的层链表，每个对象调用同一 `0x5D04D4`。
4. `0x5D04D4(0x7660F4, sprite)` 绘制最后固定场景层。

因此 Base 是重绘原场景到 Sprite，既不是只清空图层，也不是复制当前含 UI/过场叠加物的屏幕。固定层的业务名称本次未独立证明，不凭地址将其命名为某种角色/背景。

`Screen()` 是当前绘图缓冲的 API 视图，以下函数写/读同一 `0x74BA74`，不是每个 Screen 实例维护一份独立的像素：

- `Screen.Draw(sprite, x=0, y=0, alpha=255)` native 桥 `0x57F544`，基本形式在 `0x57F630–0x57F641` 调 `0x69439C(current,x,y,sprite,alpha)`；另有缩放/裁剪形式，此文不扩大其已证参数契约。
- `Screen.Fill(color=0, alpha=256)` 桥 `0x57F8D8` → `0x6CE27C(current,color,alpha)` → `0x6CE3BC`。alpha <= 0 不修改画布；alpha > 255 调 `0x6CE33C` 按 DWORD 精确填入 color；1..255 先令 `q=alpha+(alpha>>7)`，RGB 每字节 `out=old-((old*q)>>8)+((color*q)>>8)`。因此 alpha=255 已完全替换 RGB。这与 Trans 公式不同。第四字节不是直接保留原 alpha：该分支计算 `oldFourth-((oldFourth*q)>>8)+(q&255)`，经 `packuswb` 饱和到字节；不要把原存储第四字节直接当平台标准直通 alpha。原当前屏幕最终按 RGB/GDI 路径呈现。
- `Screen.Render()` 绑定写入 `0x52438A`，native `0x57F508` → `0x405578`。当过场 mode 为 0 或 duration <= 0 时调用场景更新 `0x5FD0B0` 并 `0x60992C(current)`；否则调用计时 `RenderTrans`。这不是单纯提交当前画布。
- `Screen.BaseFast()` 绑定写入 `0x529E26`，native `0x57FA74` → `0x5FD0E0` + `0x609974(current)`；后者清空后只画两个固定层，略过中间链表。
- `Screen.Snap()` 绑定写入 `0x52B047`，native `0x57FA8C` → `0x48412C` → `0x68D34C(snapshot,current)`。`0x68D34C` 直接包装深复制 `0x68C818`。
- `Sprite.Snap()` 桥 `0x583218` 在 `0x583292` 调 `0x68D34C(sprite,0x74BA84)`。它复制上次 Screen.Snap，而不是当前画布。
- `Screen.Update()` 绑定写入 `0x52C268`，native `0x57FA98` → `0x484110` → `0x6BE468` → `0x51828C`。它将当前缓冲呈现至窗口，**不清空当前像素，也不隐式更新 Screen.Snap**。全尺寸直接读取 current，窗口缩放另用 `0x7C7258` 中间图像；最终 `0x517924` 读取像素并调用 GDI 呈现。

## 对原 date 协程的实现约束

原脚本先创建 Sprite 并 `Screen()` 捕获，随后将 eyecatch 各层 Draw 到该 Sprite；调用 `ClearImage()` 后当前画布 Fill 白色，连续 Begin/End/Begin 捕获白色，然后 Draw(base)、End 捕获结束画面，`TransSet(2000)` 开始淡入。

循环中即使先 `s.Draw(base)`，随后的 `RenderTrans()` 也会用 begin/end 快照混合**替换**当前画布；随后 logo 与黑色 Fill 才叠上去，最后 Update 呈现。不能把 RenderTrans 当 no-op、在最新图层列表上加全局 alpha，或在 Update 后清空当前画布。跨协程 yield 必须保留当前画布以及两份过场快照的内容与顺序。

可用合成像素单测无需查看媒体：A=100/B=200、p=128 应为 149；p=0 精确为100、p=255 精确为200。Capture 后修改源不得改变快照；Update 后 Screen() 捕获应仍得到呈现像素；Base 与 Screen 应分别引用场景合成结果和当前屏幕结果。

## 可复核材料与边界

精简反汇编输出保存在 `.local/sorakoi-import/analysis/system-transition-evidence.txt`。可用 `audio_contract_disasm.py dis <VA> <hex length>` 复核。地址依赖当前 PE；通用实现应绑定原脚本 API 与验证过的 ABI/图像模型，不可写入游戏专名条件。

旧 EXE 的 Trans ABI 差异及 mask 像素处理已另记于上述 mask 文档；负 duration 行为保证和未知 Screen 扩展参数仍未覆盖。这里的 1600×900 是本证据文件的分配常量，不是所有游戏的通用固定分辨率。

## EnableDelay / DelayAction 与 Story 接入

`EnableDelay` 的名字注册在 `0x51EE36`，桥 `0x57F4BC` → `0x60DDF4`；后者只将全局 `0x7660F0` 写为 1。`Screen.DelayAction` 的桥 `0x57FAA4` → `0x60DDFC`：先检查该 flag；未启用则返回，启用则清 flag，遍历九个场景槽，交换/提交暂存的场景动作。它不是 fade，也不直接推进下一句。

原 `message.nut` 441–447 行提供消费时机：非零 `DelayTime` 使用严格 `DelayTime < t.Get()`；值为零时等待文字显示结束，若该句实际播放语音，还需 `!IsPlayVoice()`。跳读设置 `DelayTime=0`。ID_NEXT 分支先 `s.Commit()`，其中 `0x40555C` 调 `0x5FD0E0` 提交场景，清过场模式与时长；完成本句后再将 `DelayTime=0`。

Story 的 `story/system_commands.rs` 现将原注册 `com` 命令放到同一 SystemVm 协程中运行。已经由原生命令路径处理的命令仍优先；其余命令仅在 `com` 实际注册时进入桥。无脚本名或游戏名分支。

- 即刻返回 null：完成 Machine 请求并继续。非 null 返回若没有已恢复的分支契约则明确报错，不能将任意 integer 当选择结果。choice block 仍不在此通用完成协议内。
- 协程 yield：Machine 保留原请求；packet 为 `kind=system_command`，具有稳定 `token`、`command`、累计 `elapsed_ms`、`delay_ms`、`closed=false`、`has_frame`。有帧时换行后附 PNG；无帧不产生替代画面。
- `system_command_step(token, elapsed_ms, event)` 的 elapsed 为该 token 的累计毫秒；Story 检查单调性，转成 delta 给脚本虚拟时钟。event 0/1/2 分别为计时、确认、取消。`wait(-1)` 不因纯计时调用续跑；正 delay 未到时也不续跑。poll 本身不唤醒协程。
- 原系统音效进入一次性队列，由 `next_system_command_sound()` 消费；不占用或提前完成 Machine 的 pending 请求。
- ClearScene 清除真实场景层、映射角色和待运行运动；ToggleAuto 更新 Story 的自动模式。
- EnableDelay 保留延迟前已经呈现的场景像素；新场景仍由原指令构造，在消息循环满足上述条件后开始显示，并从提交时刻开始该场景运动。page.motion 的 `delayed=true` 要求宿主继续请求 page_frame，即使没有 actor motion 或 Effect 已关闭。等待期间 `finished=false`；关闭 Effect 只让提交后的演出直接达到目标，不能跳过延迟等待。NEXT 的 Commit 直接提交目标状态。
- 存档 v7 在原 v6 数据后保存可选延迟状态：旧场景 PNG、DelayTime、累计时间与可选提交点。v8 再追加可选的命名系统状态；当前已恢复 `ClimaxCount`，依据原 `saveload.nut` 的 `SaveInfo.Save` / `Load`（物理行 41 / 53）成对 `::Save(f,ClimaxCount)` / `ClimaxCount=::Load(f)`。没有该 global 的系统不记录它。恢复在消息 hook 前写回；hook 收到 `InRestore=true`，保持原不递增规则。读取兼容 v1–v7，特别保留已经由中间 APK 写出的 v7 格式。挂起的 system-command 协程没有可持久化 VM 栈，因此存档明确拒绝，不能截断请求；延迟场景已回到消息状态时可以正常保存/恢复。

捕获接入：桥区分 scene/Base 和 current/Screen 两个输入。Story 在消息 ACK 前保留 renderer 的实际可见文字、消息框与原消息叠层像素及原点；Screen 的 current 输入由最近呈现的场景和这份叠层合成。page_frame 也保留最新场景运动帧。新的视觉效果完成、系统绘制帧、reset 或 restore 会清除旧文字缓存，音频完成不会清除。Base 始终独立重绘场景。合成像素测试确认 Screen 得到上一句叠层，而 Base 得到新场景，不依赖读取或查看原图片。

原 MsgBox 在每页 prepare_layout 前以原 VM 中的真实 sprite/几何更新，不固定绑定 `msgbox.mng`。消息 hook 在新页 begin 时执行一次，随后只按累计时间差执行 draw；关闭设置刷新当前图层但不再执行 begin。overlay 的 SpriteSnapshot 未变化时不重新栅格化、不重设 overlay，避免静态叠层每帧分配整幅 RGBA。`message_animating` 告诉宿主文字显露数量不变时是否仍需刷新原动态叠层。

最终本轮运行库验证：`cargo test -p nekopack-runtime --lib` 为 **207 passed / 0 failed / 18 ignored**。日志 `.local/system-integration/story-system-tests.log`；包含多次 wait 保留 token、累计时钟转换、非 null 返回拒绝、完整延迟存档恢复、旧 5/6/7 存档兼容、原消息计数冷恢复及 Screen/Base 分离的合成测试。设备测试由主代理串行执行，本文不将静态或单元验证替代设备验收。

## Images 整个资源缺失

`Images` 构造函数绑定在 `0x535BD9`（写入 native `0x585D64`）。构造函数先清自身 sprite vector，在 `0x585E9F–0x585EA7` 调 `0x46F070`，忽略其 bool 结果；之后依据 vector 长度计算元素数。

`0x46F070` 的 `0x47057C` 调资源加载器 `0x4568DC`。`0x470584–0x47058C` 在返回 AL=0 时走清理/返回 false 分支，目标 vector 保持空；只有成功分支 `0x470691–0x47069E` 调 `0x68D900` 填充 images。资源加载器逐候选调用 `0x4F3C08`，最终失败支路 `0x457BF0` 返回 false。下层挂载遍历完全未找到时，`0x4F6BBd–0x4F6BF9` 结束遍历并返回 false。

因此原 `Images(name)` 在整个文件不存在时得到空 images；这不授权忽略已存在但损坏的 MNG 或读取/解码错误。技术片段另存 `.local/sorakoi-import/analysis/native-images-missing-evidence.txt`。成员名称不存在时的 `_get` 返回契约属于另一层，不由此结论自动推导。
