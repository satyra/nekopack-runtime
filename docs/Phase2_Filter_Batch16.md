# Batch 16：BLESS 夜色角色滤镜与 movie 后章节对白

本批只验证 BLESS 和 PGKM10 原生 ARM64 真机，复用同一 Rust 运行器。目标是执行 `.filter 夜`、正确显示 movie 后的章节对白；没有实现其他滤镜、角色移动或完整存档。

## 原版依据

`research/verify_original_filter.py` 对本机 `Tiny_BoD.exe` 做静态指令校验，记录文件及函数片段 SHA-256，不分发原始 EXE。报告为 `original-filter-batch16-verification.json`。这是原版代码依据，不是原版游戏屏幕的动态截图对照。

- `0x43f8bc` 的参数解析将“夜”设为 packed BGRA `add=0x00000011`、`sub=0x00111600`，无参使用空字符串，落入两个参数均为零的分支。
- `0x43fc60` 的执行器把两个值写入全局过滤状态。仅设置参数，不直接改写现有角色位图。
- 图层构造器给 `image/visual` 和 `image/back` 的标志置 0，给 `image/actor` 置 1。绘制器在图像缓存不存在时加载资源，再根据该标志调用 `0x435e18`；缓存存在时跳过装载/滤镜路径。
- 像素函数使用 `paddusb` 然后 `psubusb`。换算为 RGBA：`R=max(R-17,0)`，`G=max(G-22,0)`，`B=min(B+17,255)`，Alpha 原样保留。不是乘法暗化，不是整屏蓝色覆盖，也不改变背景或全屏 CG。

实现保留“首次绘制采样当前滤镜，随后缓存”的顺序。PNG 可以预先解码，但尚未绘制的角色标记为 pending；首次渲染才原位修改像素并记下应用的滤镜。后续 `.filter`/无参复位只影响之后的资源装载，不能重复染色现有缓存。位置/层序修改保留原来的像素，切换资源或大小目录重新装载。页面及 checkpoint 的 scene 增加全局 filter、每个角色的 applied filter 和 pending 标志，便于区分状态与实际图像。

当前仅接受一个“夜”参数或无参复位；其他模式和额外参数明确报未支持。没有将本批实现宣称为完整 filter 指令兼容。

## 结果范围

BLESS 现在可经过 movie、`.route 第１章`、`.call 012401`、`.title`、`.filter 夜`、BGM、背景及淡入，显示 `012401:11` 的第一句章节对白。桌面有界执行从原有 12 页扩展到 55 页，其中 43 页在 movie 之后；进入 `012402` 后停于 `012402:72` 的 `>>>>>>>` 角色移动修饰符，深度 2、步数 408。未知移动仍显式停止。

`filter-batch16-desktop-verification.json` 校验准确边界及原有 12 页不变。`filter-batch16-layer-verification.json` 使用独立 DAT 读取器、原脚本静态图层重放和 Pillow 的逐通道查表/Alpha 合成，对照全部 55 页像素及图层元数据，其中 23 页包含已应用夜色滤镜的角色。它不复用 Rust 滤镜计算，但也不等于原版渲染器逐像素验证。

已有独立图层校验器通过显式 `--actor-filter` 启用本批模式，旧模式不改变。其源内重放适用于本批这些从脚本起点可重建的静态片段，不作为任意跨章节、分支或动画的参考解释器。

## 测试与边界

核心单元测试覆盖全部 256 级输入的饱和运算和 Alpha 保留、空滤镜、首次绘制取值、缓存不重复修改、资源替换、保留图层的位置修改、背景不被处理及未知参数停机。滤镜原位处理既有 RGBA 缓冲区，不增加一份整图副本。workspace 测试、Clippy/all-targets 警告检查、格式检查及 14 项 Python 测试已通过。

Android 验证器补齐“movie 位于两页对白之间”的等待/跳过流程；不再只在最终未支持边界前等待 movie。可选屏幕采集验证实际 UI 显示指定对白且处于横屏，截图保留本地。最初采集分支因 Python 局部导入顺序错误退出，修正验证器后从头重跑；该未完成运行不计为通过。另修正了验证器在未提供音频参考时仍寻找旧版退出按钮的问题：现有宿主始终使用“结束运行”；最终跳过回归同时提供对应的原始音频参考。设备仍从 SAF 读取原始 DAT 和 `op.mpg`，没有转码或改写游戏脚本。

复现入口：`verify_original_filter.py`、`verify_filter_batch16.py`、`verify_story_layers.py --games-selected 0 --actor-filter`，以及 Android `verify_story.py --games 0 --movie-completion natural`。完整存档、滤镜动画和其他游戏/设备验证不在本批结果中。

下一批优先核对 `>>>>>>>` 的原版角色移动语义，再推进后续 BLESS 对白，不绕过移动指令。

## 最终真机结果

PGKM10（Android 15，原生 ARM64）自然完成电影后，55 页回归全部通过，包含 movie 后 43 页对白和 21 个原始音频资源；4 张转场目标图另外通过独立像素校验。电影解码 2497 帧、每通道 4590720 个 PCM 样本，播放头耗尽后继续剧情。主动跳过复核完成 13 页（含第一句章节对白），电影在 181 帧退出。两次均确认实际屏幕上的章节对白及横屏，结束后受管 VM 内存为零；自然回归另外采集了第 28 页夜色角色画面。

最终 APK SHA-256：`6cd0716f41caaf0bd8903542ef18c504de352834ab1cca9ba5deb31bd3efb82f`。综合结果见 `research/filter-batch16-final-verification.json`，声音事件复核见 `filter-batch16-audio-evidence-verification.json`。Runtime 单元测试 29 项通过；本批没有新增人工听音结论，也没有重跑模拟器或其他三部。所有游戏图片、音频、影片和 APK 留在本地，不提交仓库。
