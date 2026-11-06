# 第六批：Android 静态剧情预览与点击恢复

日期：2026-11-06。基线：b4ac93b6f6b44a1c1cbd7a3a7e984a168b5efae1。

## 实际交付

在既有 SAF 原始 DAT 路径上，复用标题启动得到的 Machine、Variables、SystemVm，继续执行到文本等待点。Android 显示 Rust 合成的背景/立绘和脚本文本，点击“继续”确认当前页并恢复同一个会话。四部曲各验证前两页，共八页，均通过真实 UI 点击推进，不通过重新执行标题或跳到已知行号实现。

运行环境仍是 **API 28、x86_64、libnb.so 的 ARM64 翻译执行**。不是 ARM64 真机，也不是完整可玩游戏运行时。标题菜单目前仍使用已有 headless 输入选择 1；本批的真实点击从剧情文本等待点开始。

## 支持边界

- Machine 增加显式启用的文本 profile；默认启动解析保持严格。三字段 `:姓名:语音:角色` 返回 Speaker 请求，文本返回 Text 请求。Speaker 的第三字段被保留在请求中，预览不使用其演出语义。
- pending Text 在 `complete()` 前保持原位置。重复 poll 不花费额外指令预算、不推进 PC。StorySession 用页序号确认输入；过期或不存在的序号被拒绝。姓名和语音只绑定当前文本页，下一页没有新 Speaker 时作为旁白。
- `.title` 仍调用真实 Squirrel 注册函数。静态预览支持单参数 `.img0` 背景、`.img1` 单立绘，以及 `.img 白/黒` 纯色覆盖。其他图像槽、宏、坐标参数、滤镜和未知命令明确停止，不能据此宣称通用演出已完成。
- 剧情图片是 DAT 内 `image/back/*.png`、`image/actor/*.png`。Rust 解密、PNG 解码和 RGBA 合成；Android 只显示传回的静态结果。立绘使用 PNG oFFs 像素偏移加逻辑画面中心，逻辑尺寸来自资源而非设备屏幕常量。
- 中心锚点是当前静态预览 profile，也与本机既有移植研究的坐标说明相符。本批没有运行原版截图对照，不能证明原版演出等价。独立参考程序用于像素校验，不进入 Rust 或 APK 的执行依赖。
- `.bgm`、`.se`、`.fade`、`.wait` 只记录为 effects，没有播放声音或执行时间插值。UI 明示“无音频／转场”。语音名称保留但不播放。不支持完整 MNG 动画、存档、回看、选择支线或完整剧情。
- 文本使用第五批既有 CP932-compatible 解码 profile，未验证所有汉化编码或文字标记；Android TextView 不是原版文字排版系统。

## Android 生命周期与 FFI

StorySession 由专用单线程 Executor 对应的 Rust thread-local 槽持有；Kotlin 不持有 VM 指针，不将 !Send VM 转移到其他线程。同步 JNI 操作只有 open、poll、next、close。SAF 原始 PFD 在 open 返回后关闭，Rust 拥有复制的描述符直到会话关闭。

点击期间按钮禁用，下一次点击携带刚显示页的序号。返回值为 JSON 元数据、换行、PNG 的字节包；不用 JNI modified UTF-8。所有输出仍由 Rust 对应 free 入口释放。

JNI 错误或捕获到的 Rust panic 清空会话；本层将所有错误视为结束预览，包括过期输入。Rust 核心自身拒绝过期序号时不改变 pending 页。正常关闭及 Activity onDestroy 在同一工作队列释放会话，避免 UI 线程析构 VM。onDestroy 不保存进度，重建 Activity 会从头启动；这不是存档恢复。进程被系统杀死的恢复尚未实现。

RGBA 限制为单次 64 MiB，沿用 DAT 单资源和 VM 预算。不是完整的宿主总内存预算；同时存在背景、立绘、合成图、压缩结果和 Java Bitmap。PNG 字节包是验证宿主的交付方式，尚未引入 wgpu 或实时渲染接口。

## 验证证据

1. 四个完整原始 image.dat 通过 ADB 复制到此前已获 SAF 授权的各游戏目录，设备整包 SHA-256 与已有原始档案审计相同。没有解包媒体 fixture、改写 DAT 或把游戏图片提交到仓库。
2. 每款桌面从原始 system/script/patch/image 档案生成两页。Android 同样从 SAF 读原始包，两页元数据与 PNG 字节均与桌面一致；文本另核对已有参考脚本的对应物理行。
3. 自动化通过 UI hierarchy 定位已启用的“继续”和“结束预览”按钮后点击，不直接调用推进接口代替触控。宿主每页重复 poll 并比较整个字节包；退出时受管 VM live bytes 为 0。
4. 独立 Python 档案参考读取器和 Pillow 对八页 RGBA 逐字节校验通过。参考输入来自原始 DAT；这一校验验证静态合成规则，不是原版截图相似度或游戏整体正确性证明。
5. 新增核心测试覆盖文本 profile 显式启用、pending 保持、确认推进、无效 Speaker、文本预算、过期输入、未知命令停止、立绘中心偏移和纯色覆盖保留图层。
6. 既有 workspace 测试、桌面及 Android ARM64 Clippy（-D warnings）、fmt、Python research 8 项通过。同一 APK 的旧入口再跑四部曲标题、无效选择、32 轮 FFI/预算销毁和 409 个分配故障点通过。

报告：

- `research/android-story-preview-verification.json`
- `research/story-preview-pixel-verification.json`
- `research/android-story-image-dat-verification.json`
- `research/android-story-ffi-regression.json`

## 复现入口

桌面：`nekopack-cli preview-story OUTPUT_DIRECTORY PAGE_COUNT DAT_FILE...`。列表显式给出所需原始档案，避免把配置类同名扩展文件误当档案。

Android：先按第五批完成系统选择器授权；启动 MainActivity，传入 `--ei story_game 0 --es run_id UNIQUE_ID`（四部曲回归目录编号为 0–3）。单个游戏目录也可直接使用。默认显示第一页，点击继续，结束预览释放会话。

`platforms/android/verify_story.py` 驱动真实点击并比对页面，依赖已获授权的目录；不会创建授权。`research/verify_story_pixels.py` 使用本机已有参考工具及 Pillow 做独立像素验证，参考工具不打包进应用。

下一步应完善运行器真实宿主的事件契约，优先处理声音/时间等待与多图层请求，逐步扩大剧情覆盖；不能继续用静态预览的立即确认方式替代真实音频与转场完成事件。
