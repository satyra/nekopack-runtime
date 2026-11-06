# 第九批：静态多图层与连续剧情边界

日期：2026-11-06。基线：`63e25147cc13ca470fa02a44d1c46cf884b32f71`。

## 范围

本批保持 Android 优先，在同一 Rust StorySession 上扩展静态图层及一项必要的音频等待操作。平台宿主继续从 SAF 文件描述符读取原始 DAT；没有把解包图像、转码脚本或测试 fixture 放进实际剧情运行路径。标题仍由已有 headless 系统桥选择“开始”，不是原版标题 UI 交互验收。

## 实现

- 新增内部 `scene` 模块，保存背景、纯色覆盖及 img1–img9 立绘槽。支持清除单槽、imgx 保留并重排指定槽、清空立绘，以及空 img/page 清屏。
- 静态参数支持 front/back/keep、center、left/right 1–4。立绘锚点以逻辑画布中心为基准，位置档位为四部曲研究资料中的 160 逻辑像素；PNG oFFs 叠加到锚点。未传位置时保留该槽的锚点。
- 从当前挂载 DAT 读取 imagemap.txt，展开“大”等宏；支持普通及 big 子目录，不将汉字宏硬编码到核心。当前仅支持单层宏展开。
- 保持逻辑画布尺寸来自 title.mng；不再让每次背景图加载改变逻辑画布。背景及立绘驻留 RGBA 总量限制为 64 MiB；这不是进程总内存上限，解码临时数据、画布、PNG 和 Android Bitmap 仍另占内存。
- `.bgm fade 秒数` 成为宿主等待请求。Android 按单调时间线性降低音量，释放 BGM 后确认；Rust 未收到匹配 token 不推进。没有指定时长的 `.bgm fade` 仍停止，未猜测默认值。
- 页面附带图层、数值变量槽、调用深度及累计步数。错误附带脚本位置；CLI 写出边界检查点，Android 在销毁前保存检查点、销毁后报告受管 VM live bytes。检查点用于诊断，不是可恢复存档。

移动动画、条件图像、未知目录、全屏 CG/EF、wipe、movie 等仍明确报错，不通过忽略命令扩大通过页数。keep、imgx 重排和未实际走到的其他槽号仅有合成单元测试，不声称四部曲实路径均已覆盖。

## 实际推进范围

修复前桌面探测：BLESS 12 页后卡在空 img；BIRTH 6 页后卡在空 img1；BRAVE 10 页后卡在显式 BGM 淡出；BLACK 14 页后卡在“大”参数。

修复后持续请求至首个不支持命令，实际得到 77 页：

- BLESS：12 页；从 P01 正常返回 story1，G800 置为 1；在 story1:10 的 movie 停止，深度 1、累计 196 步。页数不变，但已经走过原清屏阻塞及子调用返回。
- BIRTH：8 页；在 012401:52 的全屏 EF 图像请求停止，深度 2、183 步。尚未执行紧随的 wipe。
- BRAVE：41 页；经过三角色位置、前后关系、换表情及清层；在 010901:180 未指定时长的 BGM 淡出停止，深度 2、340 步。
- BLACK：16 页；big 立绘读取成功；在 P01:71 的全屏 CG 请求停止，深度 2、195 步。

这些是已执行文本页与停止边界，不是完整章节或游戏完成。此前八页的 PNG 字节及原有元数据字段保持一致。

## 独立验证与本批发现

`verify_story_layers.py` 使用独立 DAT 参考读取器，从真实脚本重新推导静态图层和宏，再用 Pillow 合成。全部 77 页的图层元数据和 RGBA 像素一致。它验证本批静态策略的两套实现一致，不是原版引擎截图、转场曲线或 MNG 动画验证。

语音扩大覆盖后发现两项问题，均保留边界而未跳过：

1. BRAVE 的 324 ms 短语音在真机已触发完成回调，但 currentPosition 为 0。原先只接受正位置的验证器因此停止。现在分开记录正位置推进、达到媒体时长后的完成回调，以及边界前刚启动即被释放的音效。完成证据要求相同 token、正媒体时长、真实完成事件且单调经过时间达到时长；仅 playing/ack 不算完成。新增测试拒绝提前完成、错误 token 和零时长。播放器代码没有为此伪造进度。
2. BLACK 原先两页不请求语音，设备仅放了四个非语音 DAT。扩展后在 P01:32 正确报缺失 `voice/RA_P01_0000.ogg`。补入本段实际需要的原始 voice1.dat（270,161,386 字节），手机和模拟器整包 SHA-256 均与原始清单相同。之前 17 包记录保留，本批只新增这一个包；未补造 fixture。

短音效若在最后文本页之后启动、紧接着遇到不支持命令，会被错误清理立即释放；报告明确标注 terminated_at_boundary，不计为已听完或已观察到位置推进。

## 验证结果

最终 APK 在两条执行路径各完成一次完整回归，所有本批最终设备报告的 APK/native 库哈希一致：

- PGKM10 真机：Android 15 / API 35，aarch64、native bridge=0，原生 ARM64。
- 现有模拟器：Android 9 / API 28，x86_64/libnb，明确为 ARM64 翻译执行。
- 每台设备逐页点击、比较 77 页文本来源、页面元数据及 PNG 字节；边界检查点也与桌面一致。四个边界均没有生成下一页，受管 VM live bytes 为 0。
- 每台设备 34 个音频请求（26 语音、5 BGM、3 SE）与独立原始 DAT 音频哈希一致。其中 32 个有正播放位置证据，一个短语音仅有达到时长后的完成证据，一个 SE 在边界中止。后两项没有混记成正位置推进。
- 7 次画面淡变及 1 次 BGM 淡出均出现中间采样，并在请求时长后确认。BGM 淡出同时验证播放器释放。
- 两台设备均在画面淡变中、BGM 播放中、BGM 淡出中按 Home；三种情况释放音频、清除临时 Ogg、结束 VM；被取消效果没有迟到确认。不声称后台恢复。
- 两台设备均重跑既有 FFI 探针：32 轮回调/预算失败/销毁，238+74+97=409 个分配故障点，四部曲标题及无效菜单选择，所有对应受管 VM 内存检查为 0。此入口仍用 fixture，与原始 DAT 剧情路径分开说明。
- Rust 全工作区测试、全目标桌面 Clippy、Android ARM64 Clippy 通过；研究 Python 单元测试 9 项通过。native_probe 的单独 ignored 项由 7 个受限子进程测试实际调用，不计作未执行的通过项。
- NDK r27d 构建成功；C++ shared 库及 16 KiB ELF 对齐沿用原方案。设备实际页面大小仍为 4096，不等于 16 KiB 真机验证。Kotlin 旧音频焦点接口的既有 deprecated 警告仍存在。

最终 APK SHA-256：`e4c159d3396305cf56c8df973cd8a2780d8bb1a00d0a9356a2160e17fbc302ae`。新的完整音频覆盖只有自动状态与资源字节证据，未将上一批用户对声音的确认扩展为本批全部语音的人工听感验收。

## 复现与证据

- `research/verify_story_boundaries.py`：生成桌面连续页，必须核对页数、错误文本、位置、深度、步数和 BLESS 返回变量；非零退出码本身不算通过。要求空输出目录，避免旧页面污染。
- `research/story-batch9-arm64-device-verification.json`、`research/story-batch9-translated-verification.json`：最终 77 页、音频及边界。
- `research/story-batch9-arm64-lifecycle-verification.json`、`research/story-batch9-translated-lifecycle-verification.json`：三种实际 Home 中断。
- `research/story-batch9-arm64-ffi-verification.json`、`research/story-batch9-translated-ffi-verification.json`：最终构建的 FFI、预算失败及故障注入。
- `research/story-batch9-desktop-verification.json`：桌面原始 DAT 执行、每页哈希及边界。
- `research/story-batch9-pixel-verification.json`：独立静态图层及像素验证。
- `research/story-batch9-audio-reference.json`：独立读取器的音频字节哈希。
- `research/story-batch9-short-audio-observation.json`：首次短语音验证失败事件，非完整通过记录。
- `research/story-batch9-missing-voice-observation.json`：补包前两台设备的缺资源错误及 VM 清理证据。
- `research/story-batch9-additional-dat-verification.json`：两台设备新增原始包校验。

核心复现命令（路径按本机配置提供）：

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p nekopack-android-probe --target aarch64-linux-android -- -D warnings
python -m unittest discover -s research -p 'test_*.py'
python research/verify_story_boundaries.py --cli CLI --games ORIGINAL_GAMES --pages EMPTY_DIR --output REPORT
python research/verify_story_layers.py --games ORIGINAL_GAMES --reference-root REFERENCE_READER --pages EMPTY_DIR --output PIXEL_REPORT
python research/reference_story_audio.py --games ORIGINAL_GAMES --reference-root REFERENCE_READER --pages EMPTY_DIR --output AUDIO_REPORT
python platforms/android/verify_story.py --adb ADB --serial DEVICE --desktop EMPTY_DIR --fixtures SOURCE_REFERENCE --audio-reference AUDIO_REPORT --until-boundary --output DEVICE_REPORT
```

## 后续边界

优先继续查证全屏 CG/EF 的资源路径和覆盖/清除语义，再实现对应静态显示与必要的 wipe；查证默认 BGM 淡出时长后再开放无参形式。视频另作独立能力，不伪装成功。后台仍结束会话，不是暂停恢复；不在本批引入桌面宿主、完整 MNG 或存档重构。

画面和音频的原版一致性、完整 VM 状态保存、16 KiB 真机页面、全进程内存泄漏及完整游戏兼容性仍未验收。游戏资源、页面正文、图片、音频和 APK 仅保留本地验证目录，不提交。
