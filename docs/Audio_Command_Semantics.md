# 音频配置与 BGV 原生契约（静态证据）

2026-11-13。仅读取系统脚本和 Windows PE 反汇编；未执行游戏、未读取剧情/回想正文、未播放媒体、未查看图片。地址均为首选映像虚拟地址。此文是 ABI 技术证据，不是听感一致性结论。

## 必须区分的两套音频 ABI

不能将所有 `Config.MuteBGM` 映射为 `voice_duck`，也不能将 `VolMV` 翻译成影片音量。

旧 ABI 的 `MuteBGM` 是语音期间压低背景音乐的开关；新 ABI 的同名配置字段是背景音乐分类静音，而原生函数 `MuteBGM()` 在语音播放时把背景音乐切到独立的 MV 音量。

识别应依据原系统脚本/原生注册与函数行为，不使用游戏 ID。旧脚本既有 `if(Config.MuteBGM)MuteBGM()`，也有无条件 `MuteBGM()`：ED 原 message.nut:305/:382 属于后者，但 config.nut:38/:45 只有旧五分类、VolMAS 与 MuteBGM 开关。原 ed.exe 注册 0x51E791–0x51E79D 将 MuteBGM 绑定 0x5736BC；该桥调用 0x62743C，后者 0x62743C–0x627449 先读取字符串 0x7725F3 的 MuteBGM 并判断，再决定语音压低状态。因此条件可以在脚本外部，也可以在原生函数内部；无条件调用不能作为新 ABI 的专属指纹。复核反汇编保存 `.local/system-command-coverage/ed-mute-bgm.txt`。

新 Config 同时具有 `MuteMAS/MuteBGM/MuteVO/MuteSE/MuteENV/MuteBGV/MuteSYS/MuteMV` 与对应 Vol 字段；识别仍须要求完整配置族与真实原调用。旧族须具有五分类与 MuteBGM 开关，允许条件或无条件调用。仅 `VolMAS` 存在不足以判定新 ABI，也不能据此自动乘主音量。最可靠判据仍是原 RestoreVolume/MuteBGM 的 ABI，完整配置族用于脚本侧核对。

若契约尚未识别，应保留明确的未知契约错误，不以新字段缺失默认值或游戏名称猜测。配置字段是否实际作用也应服从相应 ABI，不能仅因旧 Config 出现 VolMAS 就自动乘主音量。

## 新 ABI：主音量和分类静音

输入：`.local/sorakoi-import/game/ソラコイ/sorakoi.exe`；系统脚本 `.local/sorakoi-import/analysis/config.nut`。

`RestoreVolume` 注册：字符串 `0x75D2AF`；绑定 `0x57C418`；其 `0x57C41B` 调用 `0x448554`。

可实现算法（百分比均整数，乘积除 100 向零截断）：

- `master = MuteMAS != 0 ? 0 : VolMAS`，`0x44855B–0x448579`。
- 对 `BGM/SE/VO/ENV/BGV/SYS/MV`，`effective[k] = Mute[k] != 0 ? 0 : trunc(Vol[k] * master / 100)`。
- 字符串表 `0x745C90–0x745D01` 依次为 MuteMAS、VolMAS、MuteBGM、VolBGM、MuteSE、VolSE、MuteVO、VolVO、MuteENV、VolENV、MuteBGV、VolBGV、MuteSYS、VolSYS、MuteMV、VolMV。
- 分类值分别应用到 BGM `0x7A26A4`、ENV `0x7A274C`、BGV1 `0x7A27F4`、BGV2 `0x7A289C`、SE `0x7A2944`、VOICE `0x7A29EC`、SYS `0x7A2A94`。统一分类设置函数 `0x697138`，所有本次调用的时间参数为 0。
- MV 有效值保存 `0x745404`，BGM 有效值保存 `0x745408`，BGV 有效值保存 `0x74540C`。MV 不是独立播放器槽。
- `RestoreVolume()` 首先将 BGM 应用为 BGM 分类值；后续 `MuteBGM()` 再按语音状态切换。不要凭空将配置恢复步骤替换成异步百分比 duck。
- `sound()` 在 `0x4466DD–0x446745` 同样按 MuteMAS/VolMAS 与 MuteSYS/VolSYS 算 SYS 分类值，因此系统声也受主音量与 SYS 静音影响。

新配置 UI 的额外联动属于原脚本逻辑（config.nut:386–389），不是所有 ABI 的底层恒等式：`VolMV=min(VolMV,VolBGM)`；若 `MuteBGM` 则设置 `MuteMV=1`；`VolENV=VolSE`、`VolSYS=VolSE`。脚本没有相应将 MuteENV/MuteSYS 复制为 MuteSE。运行原 UI 时应由脚本执行这些关系。

## 新 ABI：MV 是语音期间的背景音乐值

`MuteBGM` 注册字符串 `0x75D2A5`，函数桥 `0x57C40C`，实际 `0x447F4C`。

- `0x447F62` 调 `0x447F40`，后者检查 VOICE 槽 `0x7A29EC` 是否播放（`0x6970E8`）。
- `0x447F6F–0x447F87`：语音在播则读取 `0x745404`（MV），否则读取 `0x745408`（BGM），立即应用到 BGM 槽 `0x7A26A4`。
- 此分支不读取 Config.MuteBGM；它使用 RestoreVolume 预先算好的有效值。
- `message.nut:422` 在消息循环每轮无条件调 MuteBGM，`:513` 在完成 NEXT 分支也调用。调用顺序在 StopVoice/FadeVoice 之前，不能用“有对白文本”代替“VOICE 槽实际仍在播放”。

例如原默认 master=100、BGM=75、MV=50，语音在播时 BGM 分类从 75 换成 50；这不是再乘 50%，也不是影片音量。MuteMV=1 表示语音期间 BGM 分类值为 0。最终仍叠加音频命令自身音量与包络。

`VolMV` 的所有原生读取只有 `0x448691`，其结果只经 `0x745404` 在上述语音分支使用。本次未恢复影片本身的音量契约，不能以 VolMV 名称推定它。

## 新 ABI：BGV 与普通语音的排斥

MuteBGM 函数还会依据普通语音记录的名称分别抑制 BGV，而非对全部 BGV 一律 duck：

- `0x447F92` 从语音记录对象 `0x7A2A04` 取名称字符串副本。
- `0x44836E–0x4483D6`：仅在 VOICE 正在播放时，分别查找子串 `AJ`（字符串 `0x745C8A`）和 `HM`（`0x745C8D`）；查找函数 `0x44847C` 是子串查找，未见前缀限定。
- `0x4483DC–0x448415`：含 AJ 则 BGV1 分类设 0，否则恢复 `effective[BGV]`；含 HM 则 BGV2 分类设 0，否则恢复 `effective[BGV]`。均即时设置。

AJ/HM 是这个原生二进制中的字面量，不能推广成所有引擎的固定角色名。完整移植应把它们纳入从原生 ABI 恢复出的通道/语音标签关联，而不是按游戏 ID 写适配。

## 旧 ABI：可核验的背景音乐压低规则

旧 `Tiny_BoD.exe`（现有 `.local/disasm_bless.py` 的相同输入）的证据：

- `MuteBGM` 实际入口 `0x446A64`，先读取 Config.MuteBGM（字符串 `0x550B03`）。为假直接返回。
- 为真时 `0x446A73` 调 `0x446A58` 检查 VOICE 槽 `0x584598` 是否播放，然后调 `0x4C05C8`，目标 BGM 槽 `0x5843A0`。
- `0x4C0605–0x4C0624`：语音在播，设置独立因子 50、时长 0。
- `0x4C0627–0x4C0648`：语音不播，设置因子 100、时长 1000 ms。
- 旧 RestoreVolume `0x446A88–0x446B0F` 将五个分类音量直接送各槽，未使用新 ABI 的分类静音族。

因此旧分支应保留“分类音量 × 独立语音压低因子”的模型；新分支应使用“BGM/MV 两个有效分类值中选择一个”。切勿把二者合为一个含义不明的 voice_duck 布尔值。本次 1000 ms 是原生参数证据，未进一步验证插值曲线或声学包络。

## 带参数 .bgv：精确 token 编译契约

2026-11-13 续查完成。关键纠正：`ch/tag/name` 是 BgvCommand 的内部参数表键，**不是命令行的三个命名参数**。没有发现 `ch=2`、`tag=AJ`、`name=...` 的特殊分支，不能将它们作为已证实语法实现。

虚表 `0x748070` 的首项是编译器 `0x449AE8`，次项是执行器 `0x450828`。编译器按顺序遍历传入 token 向量（`0x449BFB–0x449C33` / `0x44C085–0x44C099`），本 ABI 的完整核心算法是：

```text
ch = 0
tag = ""
name = ""
for token in args, 从左到右:
    if token 按原生不区分大小写字符串比较等于 "AJ":
        tag = token       // 保留输入大小写
        ch = 2
    else if token 按原生不区分大小写字符串比较等于 "HM":
        tag = token       // 保留输入大小写
        ch = 3
    else:
        name = tag + "BGV" + token
emit BgvCommand { ch, tag, name }
```

静态证据：

- `0x449AFE–0x449B06` 将 ch 清零；`0x449B2F–0x449B6D` 和 `0x449BA1–0x449BD9` 初始化 tag/name 空串。
- `0x44AC58–0x44AC76` 比较 token 与 `AJ`；成功复制 token 到 tag，并在 `0x44B302` 写 ch=2。
- `0x44B311–0x44B341` 比较 token 与 `HM`；成功复制 token 到 tag，并在 `0x44BA73` 写 ch=3。
- 比较函数 `0x6EB838` 在 `0x6EB868–0x6EB874` 对两字节都经大小写映射表再比较，另一路调字节大小写转换 `0x6F2FC8`；因此匹配不区分 ASCII 大小写。赋值使用原始 token 指针，未将 tag 规范化大写。
- 未匹配 token 的 `0x44BA82–0x44BAC0` 连续调用两次字符串连接函数 `0x446788`，第一次左值是 tag、右值是字面量 BGV（`0x745D57`），第二次左值是上次结果、右值是 token。`0x4470E3–0x447100` 证明 helper 将右值复制到左值旧长度之后，没有插入斜杠或其它分隔符。
- `0x44C372–0x44C378` 将最终 ch 写入待输出值；后续依次以 ch/tag/name 键写内部参数表；`0x4505CF–0x4505DC` 提交编译结果。

因此以下都是来自原生算法的合成例子，未读取任何剧情命令正文：

- `.bgv` → `{ch:0, tag:"", name:""}` → 同时停止 BGV1/BGV2。
- `.bgv AJ` → `{ch:2, tag:"AJ", name:""}` → 只停止 BGV1。
- `.bgv HM` → `{ch:3, tag:"HM", name:""}` → 只停止 BGV2。
- `.bgv AJ 001` → `{ch:2, tag:"AJ", name:"AJBGV001"}` → 播放 sound/bgv/AJBGV001。
- `.bgv HM 001` → 相应播放 HMBGV001 到 BGV2。
- `.bgv 001` → ch 仍为 0，即使 name=BGV001，执行器也进入双停止分支，不加载该名。
- `.bgv AJ 001 002` → 后一个普通 token 覆盖 name，最终 AJBGV002。
- `.bgv 001 AJ` → 最终 ch=2/tag=AJ/name=BGV001。角色 token 不追溯重建之前的 name；不能重排参数。
- `.bgv AJ 001 HM` → 最终 ch=3/tag=HM/name=AJBGV001；角色最后一次赋值决定槽和角色音量，普通 token 最后一次赋值决定 name。

这不是“第一个参数一定角色、第二个参数一定编号”的固定位置语法；是上述顺序扫描。最小合法子集可只支持裸停止、单角色停止、角色+单后缀播放，但完整算法无需引入命名参数猜测即可实现。AJ/HM 和 BGV 字面量属于从当前原生 ABI 恢复出的数据，不应以游戏 ID 分支写入通用 runtime。

本次对 PK2 清单中的 101 个 script 路径读取对应原 script.dat，仅统计 `.bgv` 开头命令 token/行号，命中 0；补丁版本亦命中 0，原字节任意位置 `.bgv` 子串也为 0。因此上述语法结论来自原生编译器，不声称有现有剧情触发覆盖。输出保存 `bgv-original-command-tokens.json` 与 `bgv-patch-command-tokens.json`。未输出其它命令或剧情上下文。

## BGV 执行器、缺参及角色音量

- 执行入口 `0x450828`。内部键 ch=`0x745D7A`、tag=`0x745D7D`、name=`0x745D81`，分别在 `0x451021`、`0x4521BF`、`0x453901` 查询参数映射。
- 编译器始终输出三项，裸命令默认 ch=0/tag=name空。执行器自身映射不存在时也以零值/空串插入：ch 分支 `0x451099–0x4510A4`；tag 分支 `0x452237`；name 分支 `0x453979`。这不意味着 source 有命名参数直通入口。
- `ch=0` 走 `0x454EDB` 的双 BGV 停止分支，不调用角色音量函数，不加载 name。
- 非零 ch 在 `0x4555EE–0x4555FD` 算 `record = 0x7A2644 + ch*0xA8`；句柄在 record+0x60。原生编译器只产生 0/2/3，故可直接以枚举区分全停/BGV1/BGV2；无需暴露任意整数槽索引。
- name 非空（`0x455603–0x455608`）则先以 tag 调 `0x575C60` 取得角色音量，然后停止旧槽、检查 MegaSkip、从 sound/bgv 加载 name，loop=1 播放。角色音量送 `0x6971B8`，pan=0 送 `0x697238`。分类增益仍来自独立 VolBGV/MuteBGV。
- name 空时 `0x455D05–0x455D0F` 停止选定槽并清名称状态。
- 角色函数 `0x575C60` 直接读 Config.VoiceMute[tag] 和 Config.VoiceVolume[tag]，不调用 Squirrel VoiceVolume(tag)，因此不存在脚本 XX fallback。
- `0x575DE6–0x575DFB`、`0x575FE3–0x575FF8` 使用原 tag 查询。字段查找失败进入诊断日志 `0x4EAE0C`（其 `0x4EAE38→0x4EAC18` 为日志输出路径，不是 throw），然后继续。
- 数值读取函数 `0x69CD14` 只接受 native numeric 类，浮点经 `0x6F4248` 转整数；不成功时 VoiceMute 局部值在 `0x575E9A` 置0，VoiceVolume局部值在 `0x576097` 置0。最终 `0x576099–0x5760A5` 返回 `mute != 0 ? 0 : volume`。
- 因此缺失角色音量项导致返回0，不能擅自改成100或XX；缺失静音项按局部0处理，仍读取角色音量。普通源命令无角色 token 则 ch=0，根本不会进入此查询。

## 保存的复核材料

- `.local/sorakoi-import/analysis/audio_contract_disasm.py`：可复跑的静态反汇编辅助程序。
- `audio-restore-volume.txt`：新 RestoreVolume 完整函数。
- `audio-mute-bgm.txt`：语音状态、BGM/MV切换与BGV名称匹配函数。
- 原来的 `bgv-execute.txt`、`bgv-execute-tail.txt`：参数和播放路径。

最初证据恢复阶段不修改生产源码；后续授权实现范围仅限 engine_abi.rs 及 engine_abi/bgv.rs。全程不操作 ADB。

## 已实现的共用 BGV ABI 探测与 token 解析

`crates/nekopack-runtime/src/engine_abi/bgv.rs` 导出 BgvProfile、BgvChannel、BgvResolved 与 probe_bgv，经 engine_abi.rs 重导出。

- 从 Borland BgvCommand RTTI 前缀找到唯一 parse/execute 虚表；无此类型返回 None，类型存在而处理器未知或冲突返回 Err。
- 验证上述编译器分支、ch=2/3指令、顺序遍历、两次字符串连接、执行器双停止/槽地址计算/循环播放结构。标签和资源名前缀从原指令中的 PE 字符串地址读取；生产代码没有 AJ/HM/BGV 值或游戏 ID/hash 白名单。
- 独立查找 MuteBGM 的原生注册桥和被调函数，恢复两个 voice_suppression_substring，并校验其停止/抑制目标地址与 BGV 执行器两槽吻合。即使编译器角色标签与抑制字面量不同也能保留各自语义。
- BgvProfile.parse 接受现有字节 token 列表，使用 UTF-8 输入边界，将原顺序扫描转成 channel/tag/name；不重排、不补角色、不把内部字段当用户命名参数。
- 该探测器支持已核验的 Borland 指令布局及地址/常量变化；不声称识别任意编译器重排。遇到不同布局需要增补原生证据与对应 decoder，而不是采用默认游戏行为。

验证：独立 Rust harness 的7项测试全部通过（现有Wipe四项、新BGV三项）。已实际运行默认 ignored 的本地原PE测试：追加新字符串区并移动指令指针，将两个编译器标签改为 QX/ZY、抑制子串分别改为 SX/SY、资源前缀改为 ALT，probe 仍正确读出互相独立的五个值；将原 ch=2 的指令改为4后 probe 明确失败。token 测试覆盖单停、全停、顺序、大小写保留和最后赋值行为。

这些测试不执行游戏，只读取和修改内存中的 PE 副本；源 EXE 未更改。测试入口 `engine_abi::bgv::tests::original_and_changed_constants`，环境变量 `NEKO_ENGINE_ABI_SORAKOI` 指向本地原 EXE。

## 共用脚本协程 sound 的宿主增益协议

原现代 `sound()` 已证明按调用时 Config 的 MAS/SYS 计算有效分类值；旧 ABI 直接采用 SYS，不能因单独存在旧 VolMAS 就乘主音量。SystemCommand 原先只记录 Config.VolSYS，并把它放进 `volume_percent`；而 Android 对 operation77 的声音使用 `playUiAudio` / `ui_audio` 路径，该路径要求 `category_percent` 并使用单位命令增益。因此旧包在此处是协议字段不符，也不能通过给它再乘一次 SYS 来修复。

现在 sound 调用立即捕获音频字段快照；快照获取 body 与 audio_config 共用，Rust 同一 Config 解析器校验完整族并计算有效 SYS。函数随后改 Config 不会改变已触发声音的值。宿主包输出 `category_percent=effectiveSYS`，不把分类音量冒充命令音量；Android host_master 仍是独立用户宿主覆盖因子。该修复不硬编码游戏、角色或默认现代字段。

合成测试检验 master=67、SYS=50 得到33，随后SYS=3得到2，SYS或MAS静音得到0；旧VolMAS=1仍保留旧SYS=45。新桌面 `system_command_smoke` 使用独立fixture执行原date计时退出、date确认退出、probe_wait两次输入及返回后的system sound，只打印token/时刻/序号/包长度等技术元数据，不保存图像或正文。
