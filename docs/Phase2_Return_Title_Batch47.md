# Batch 47：剧情菜单返回标题及重新开始

## 已确认的原版行为

2026-11-10 使用 computer-use 的 @oai/sky 操作本机 BLESS 原版：从标题开始到首段普通对白，F2 打开菜单，点击“タイトルに戻る”，直接回到标题，没有出现确认弹窗；再次点击开始，回到同一段开场剧情。随后关闭原版测试进程。只操作普通剧情与菜单，未打开 CG／回想。

这是一次实际窗口观察，单独记录在 `research/original-reset-batch47-observation.json`，不由静态验证器假装执行。当前 Windows 编码下部分文本显示乱码；判断依据是菜单图像、状态转换和原始系统脚本，不从乱码文本推断剧情内容。

`system.nut` 的 ID_RESET 分支调用 `ResetGame()` 后 `throw null`，中断旧调用链。只读 PE 验证确认本机 BLESS 的 ResetGame 注册到 0x40c518，调用 0x4530a8；后者把字面量 title 传入 0x48963c，并设置 0x5510cd 标志。证据固定 EXE SHA-256，见 `research/verify_original_reset.py` 与 `research/original-reset-batch47-evidence.json`。这些证据不说明原版内部是否重建整个 VM，也不证明其所有原生被调用函数、音频及内存行为。

## 实现及生命周期

菜单 RESET 从 unsupported 改为受支持动作。Rust StorySession 标记 return_title_requested，保留当前 packet 供读回核对，但不再接受旧对白文字推进、ACK、模式切换、设置、隐藏或再次开菜单。RESET 不把未确认正文写入已读历史。

Android 接到请求后停止接受旧输入，停止对白轮询、视频宿主、角色动作和动画，关闭音频播放器、释放焦点并清理当前会话的音频缓存。沿用配置与已读历史持久化。专用 worker 调用新操作 39，只有 Rust 已请求返回标题才允许执行；丢弃旧 Story/Title VM 后检查实际原生分配计数为 0。

原生关闭成功后将旧 Activity 的 storyLoaded 清零，再用新的 Activity／worker 创建标题会话。旧 Activity 随后 finish，其 onDestroy 不再重复调用 native close，避免旧线程销毁时误检查或干扰已经创建的新 VM。新会话使用新的诊断目录和 instance ID，沿用同一 SAF 授权、游戏选择、配置与已读配置域；不携带旧页面探针或模态状态。宿主进程保持不变。

标题从现有共用 Rust 引导入口建立，重新开始仍走已有标题 Start → 剧情链路。这是当前 Runtime 的明确生命周期设计，不声称复制原版未确认的 VM 内部所有权方案。本批没有实现剧情存读档、日志、CG／回想或新的平台宿主。

## 验证范围

新增核心测试确认 RESET 是终止性请求，旧页 packet 与已读状态不变，后续推进、ACK、旧菜单操作与模式操作被拒绝。Native 操作 39 返回实际 vm_live_bytes；原有操作 3 格式保持不变，历史生命周期验证不受诊断格式变化影响。

BLESS 模拟器连续三次返回标题：

1. 首段部分文字、尚未确认时返回，重新开始仍为未读。
2. 确认首段并进入第二页后返回，重新开始保留首段已读。
3. 推进至第 13 页的实际 BGM 播放中返回，确认音频释放，再开始并继续第二页。

每次检查旧 VM 分配归零、播放器／待播队列／焦点／动画／缓存清空、新标题创建晚于旧 VM 关闭、进程 PID 不变且 instance ID 改变。新游戏第一页的 source、line、调用深度、步骤、变量、场景与 PNG 均比较既有基线；模式默认关闭，配置保留。旧 text-events 和 host-events 在新标题及重新开始期间不得再增长，也不得出现旧 onDestroy 的第二次 native close。

继续执行本批 APK 上的菜单、隐藏、快捷键、已读快进、自动播放和 57 页普通剧情回归。原版没有确认弹窗的观察仅覆盖本机 BLESS 与当前设置；未验证其它引擎版本的提示行为。音频为播放器状态／资源释放验证，不是波形或人耳复核；native 计数不代表 Android 整体堆的即时 GC 结果。

设备为 emulator-5568：x86_64 + libnb.so ARM64 翻译执行，不是 ARM64 真机。首轮第一次重启成功后，验证器在新目录首次开菜单时读到了尚未发布的快照；修正等待非空快照后，第二轮又捕获菜单转移条件内多次读取跨越 open→closed 的竞态；改为每次判定只读取一份原子快照，再完整重跑，失败轮不记为通过。

## 最终结果

最终 APK SHA-256：`3c9000f4a88013bdf80ca4715542e31263778dfff8acb372c60b71307cdd204a`。

150 个顶层 Rust 测试、7 次隔离宿主原生探针、17 个 Python 测试、原版静态验证、验证器语法检查、fmt、Clippy（all-targets，deny warnings）和 NDK r27d / API 23 ARM64 构建通过。

第 1、2、13 页的连续返回标题／重新开始通过，原生 VM 分配均归零；第 13 页确认返回前 BGM 正在播放，返回后播放器与缓存清空。新会话同进程、不同 instance，首段剧情完全匹配既有页面／场景基线，配置和已读状态按验证条件保留。

菜单、隐藏、快捷键、快进、57 页普通剧情及自动推进 55 页回归通过。全部回归结束后再次独立读取三个旧会话，检查 host 日志最后为 released、无晚于 native close 的事件、正文仍停在 RESET 检查点、无残留音频缓存或二次 native close。独立读回 SharedPreferences 确认快捷键绑定已恢复。

详见 `research/title-reset-batch47-emulator.json` 和 `research/title-reset-batch47-checks.json`。以上设备结果仅代表模拟器 ARM64 翻译执行。
