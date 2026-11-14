# NekoPack 连续实机画面展示片

## 可选轻快配乐版

用户选定的轻快版使用 Kevin MacLeod 的《Carefree》（尤克里里、吉他与打击乐），官方曲目页标明 CC BY 4.0：<https://www.incompetech.com/music/royalty-free/index.html?isrc=USUAN1400037>。将官方 MP3 保存到 `.local/promo-v2/music/Carefree.mp3` 后运行 `python tools/promo/replace_showcase_music.py`。输出独立的 `nekopack-showcase-carefree.mp4`，视频流保持不变，音乐截取到片长、调整响度并淡入淡出。原始合成音乐版仍保留。

发布轻快版时，把生成的 `.local/promo-v2/music/CREDITS.txt` 署名、来源链接、CC BY 4.0 链接和修改说明加入视频简介；此音乐不适用项目源码 GPL 许可。脚本会验证完整解码和视频流哈希，并生成独立交付记录。

本工具读取真实 APK 录屏，使用 FFmpeg 合成 1920×1080、30 fps 的展示片。浅米白背景、墨紫字体、Android 横屏设备轮廓与连续录屏为主；不含系统 TTS、示意游戏画面、借用的参考视频或配乐。片头与片尾共 8 秒，相邻镜头各有 0.5 秒淡化衔接。配乐由 NumPy 合成原创轻柔 ambient，含淡入淡出，最终 AAC 自动检查削波。

## 素材与配置

录屏首选横屏 1920×1080、30 fps；其他横屏比例会完整等比放入屏幕，不裁切操作区。不得录入宿主桌面、鼠标、通知或个人文件。录制场景应连续操作，给每次变化留出阅读时间。模拟器录制必须明确标注，不能称为真机。

将配置保存到 `.local/promo-v2/scenes.json`，素材保存在同目录的 `captures/`：

```json
{
  "title": "NekoPack",
  "subtitle": "Android 上的视觉小说兼容运行时",
  "capture_label": "Android 模拟器录制",
  "scenes": [
    {"id":"library", "file":"captures/library.mp4", "title":"作品，触手可及", "tag":"游戏库", "start":0, "duration":15, "source":"当前源码构建的 APK，在 Android 模拟器中录制；连续浏览游戏库。"},
    {"id":"import", "file":"captures/import.mp4", "title":"从本地，开始故事", "tag":"本地导入", "start":0, "duration":15, "source":"同一 APK 与模拟器；选择合法自备本地素材并导入。"},
    {"id":"detail", "file":"captures/detail.mp4", "title":"每个故事，都有入口", "tag":"作品详情", "start":0, "duration":15, "source":"同一 APK 与模拟器；打开作品详情，展示可用操作。"},
    {"id":"menu", "file":"captures/menu.mp4", "title":"常用操作，就在手边", "tag":"快捷菜单", "start":0, "duration":15, "source":"同一 APK 与模拟器；打开运行时菜单。游戏画面须获授权。"}
  ]
}
```

`legal_note` 是可选的顶层片尾说明；含已授权游戏画面时应设为 `游戏画面版权归原权利人所有 · 请自备有权使用的资源`。片尾固定标注“NekoPack 引擎兼容项目 · Android ARM64 预览版”。`crop` 可选，例如 `[0,24,1280,696]` 表示裁去 1280×720 录屏顶部的 24px 系统状态栏；必须如实保留 APK 内容。`file` 相对配置文件，`start` 和 `duration` 单位为秒。`id`、`file`、`title`、`duration`、`source` 必填。标题保持约 8–14 字；tag 是右上角极轻的场景标记。每段 `source` 应准确描述 APK/录制环境/操作内容。录屏长度不足时直接报错，不会循环、冻结尾帧或伪造操作。四段各 15 秒时成片约 65.5 秒；四段各 16 秒时约 69.5 秒。建议最终 60–80 秒。

## 生成与验收

在仓库根目录执行：

```powershell
python tools/promo/render_showcase.py
# 可选：--scenes <配置路径> --out <输出目录> --crf 19
```

依赖 Pillow、NumPy、imageio-ffmpeg，以及本机微软雅黑；字体不会打包分发。视频编码 CRF 默认 19。每个镜头先独立合成，再通过 FFmpeg 淡化衔接，不使用 Python 逐帧绘制整片。

输出：

- `nekopack-showcase-zh.mp4`：正式视频（H.264 / AAC）；
- `cover.jpg`：优先选择 menu 镜头中点作封面；无 menu 则用首个实机镜头，可由顶层 `cover_time` 指定成片秒数；
- `contact-sheet.jpg`：片头、每幕中点、中段、片尾联系表；
- `source-manifest.json`：录屏绝对路径、SHA-256、原始规格、剪辑范围、场景来源说明、成片规格和整片解码结果；
- `render/`：可复核的背景、分段、原创音乐与抽帧。

工具自动执行整片解码、分辨率与时长校验。交付前仍需人工检查联系表和实际视频，确认标题、模拟器标签与画面操作一致，关键文字可读，且没有被误切的界面、未授权游戏片段或虚构兼容/通关结论。不要依据静态抽帧宣称已经验证所有动态操作。


## v3：新增空恋原版与 PK02 汉化标题

`scenes-v3.example.json` 是可提交的纯配置示例，复制到 `.local/promo-v3/scenes.json` 后使用；录屏、配乐、成片、背景图和联系表保存在忽略的 `.local/`，不随源码分发。示例依赖仍保留的 `.local/promo-v2/captures/` 六段旧素材，以及新目录下 `captures/sorakoi-original-title.mp4` 与 `captures/sorakoi-translated-title.mp4`。两段新素材各取 1–7 秒，在旧开场后加入；含淡化成片约 88.1 秒。

旧镜头 APK 为 `c44f940415b1685ce343f68a47380bb729e06fe2d594203ef3494f4e9f80fa78`；新增空恋镜头 APK 为 `33392575680ed8fc0ad6c71184b62263f82cd57a2a5080020402d860ff3ec51c`。逐幕 `capture_metadata` 写入来源清单，不把旧 BLACK 和 UI 镜头视为新 APK 证明。空恋仅展示原标题与汉化标题，不录回想或正文。片尾“空恋原汉各 200 页 · 一次选择 · 冷启动读档已验证（非通关）”依据 `research/save-continuity-verification.json`，只关联新 APK 的空恋测试；标题录屏本身不证明这些操作或通关。

继续使用已经保存在本机、SHA-256 为 `8433b770a630d9b1594fd484442c677907ece899a4d149954cd2e74fd733e311` 的 Carefree，不需要重新下载。生成命令：

```powershell
python tools/promo/render_showcase.py --scenes .local/promo-v3/scenes.json --out .local/promo-v3
python tools/promo/replace_showcase_music.py --out .local/promo-v3 --music .local/promo-v2/music/Carefree.mp3
python tools/promo/validate_showcase.py --out .local/promo-v3
```

`outro_subtitle`、`verification_note` 可定制片尾文字；未指定时保持原版。渲染器计算标题与右上标签间距，发现重叠直接报错，来源清单保存这些边界数据。最终验证脚本完整解码轻快配乐版，核对 1920×1080 / 30 fps、时长、双声道 48kHz 音频和削波，计算片尾文字宽度，并对空恋原始录屏与成片执行 blackdetect 和亮度统计。`qa-report.json` 只代表技术检查；不打开联系图或游戏图时必须明确写出未进行视觉审阅，不得称视觉验收通过。`source-manifest.json` 对应换配乐前的成片，`carefree-delivery.json` 与 `qa-report.json` 对应最终轻快配乐版；发布时使用最终 MP4，并保留 `music/CREDITS.txt` 全部署名。
