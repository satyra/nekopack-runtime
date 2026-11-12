# NekoPack 中文介绍视频

可复现的原创功能示意短片。生成横屏 1920×1080 / 30 fps MP4，中文系统配音与硬字幕；另附独立 WAV、SRT、封面、分镜 JSON、联系表和渲染日志。

## 运行

在仓库根目录、Windows PowerShell 中运行：

```powershell
python tools/promo/render_intro.py
python tools/promo/validate_intro.py
```

依赖 Python、Pillow、imageio-ffmpeg，以及 Windows 的 `System.Speech` / `Microsoft Huihui Desktop` 中文语音。脚本使用本机已安装依赖，不下载模型、字体或素材。没有上述中文语音时直接报错，请安装所需 Windows 中文语音后重跑。

输出写入 Git 已忽略的 `.local/promo/`。`--preview-only` 仍生成配音、分镜、字幕、封面与联系表，但跳过视频编码。重新运行会覆盖同名生成物。

## 素材与表达边界

- 所有场景均为 Pillow 程序绘制的原创几何图形与功能示意，不使用游戏图片、CG、回想正文或游戏音频。
- 猫形播放标识依据仓库 `platforms/android/res/drawable/neko_mark.xml` 中的几何设计重新绘制。
- 中文文字采用 Windows 本机微软雅黑字体渲染；仓库不包含或分发字体文件。
- 配音来自本机 Windows System.Speech 中文语音，无外部商业音频、背景音乐或在线 TTS 调用。
- 每一幕均标明“功能示意 · 非游戏录屏”；界面不代表实际 APK 的精确截图。
- 叙述限定为 Android ARM64 预览阶段，不宣称正式发布、全部兼容或五款全部通关。视频事实来自制作时仓库与任务提供的验证记录；后续实现变化时应更新 `SCENES` 后重新生成。

## QA

`validate_intro.py` 检查全片可解码、H.264 / yuv420p / AAC、1920×1080 / 30fps、视频与独立 WAV 时长，以及全部字幕在 1760px 安全宽度内。`contact-sheet.jpg` 是八场景视觉审阅入口，`cover.png` 为 1080p 无字幕封面。字幕硬烧录在距底边 47px 以上的面板内，并保留独立 UTF-8 SRT。逐幕采用淡入淡出与位置缓动。

已知限制：系统语音带合成音色，英文名称和技术缩写读音受 Windows 语音引擎影响；本片没有真人配音，也不展示游戏实际运行画面。配音按每幕两段字幕展示，SRT 是分段阅读字幕，不是逐字口型对齐字幕。
