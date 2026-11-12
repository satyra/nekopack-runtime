# NekoPack Android 开发进度

更新日期：2026-11-12

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
