# 第八批：ARM64 真机验证

日期：2026-11-06。基线：f6c5403b4e90f840d02bdaf5aaba48cebcfecd22。

## 结论与设备

已在用户连接的 PGKM10 真机完成四部曲回归：系统为 Android 15 / API 35，内核 aarch64，ABI 含 arm64-v8a，native bridge 属性为 0。验证进程自报 Rust aarch64、64 位，页面大小为 4096。**本批为原生 ARM64 真机执行，与此前 x86_64/libnb 翻译执行分开记录。**

安装包仍仅包含 arm64-v8a 库，NDK r27d、C++ 共享运行库及 ELF 对齐配置沿用原构建。未验证 16 KiB 页面或其他厂商手机。真机通过不等同完整游戏运行时验收。

## 原始资源与 SAF

通过手机系统文件选择器授予 `Documents/NekoPackValidation` 目录权限，未申请全盘访问权限。四款游戏按 0–3 子目录放置原始资源，共 17 个 DAT、10,390,905,744 字节。手机上逐个计算的整包 SHA-256 与原始档案审计一致。

目录包含前两页剧情实际需要的 image、system、script、sound、voice 及补丁包；BLACK 本次没有语音请求，未据此扩大到后续语音覆盖。运行器继续经 SAF 描述符直接读取 DAT，解包 UTF-8 fixture 只用于独立桌面对照及旧 FFI 入口。

首次系统授权后，独立重启并复用持久授权成功；挂载数分别为 4、5、4、4。四款标题执行日志与桌面原始 DAT 路径逐字节一致，无效菜单输入均拒绝。第一次授权后过早切换测试、未等到完整报告的运行不计入通过结果；最终 SAF 报告来自另一次完整运行。

## 真机暴露的问题及修正

首次 BLESS 音频验证失败：MediaPlayer 已报告开始，200 ms 的唯一进度采样为 0。原测试不能据此区分真实停滞与启动后的播放时钟延迟。

修正限定于诊断观测与测试等待：

- 宿主每 200 ms 观察一次，首次非零进度后停止；最多 25 次，不无限轮询。播放器已关闭、通道已替换或不再播放时停止观测。
- 测试在点击下一页前有界等待真实进度或完成事件，窗口为 10 秒。仍要求进度大于 0，不把 isPlaying 或“准备成功”单独视为播放正确。
- 不更改播放器启动、音量、资源、脚本完成时机或 VM 语义，不修改历史通过报告。

重新运行后，BLESS 语音采样由 0 变为 135 ms，BIRTH 语音由 0 变为 189 ms，BRAVE BGM 由 0 变为 187 ms；另两个音频请求首次采样已为正。说明本次失败源于单次采样过早，而非验证范围内持续播放停滞。保留首次失败日志和当时 APK 哈希供追溯。

报告生成工具增加明确的 `native ARM64` 分类，要求 aarch64 内核且 native bridge 为空或 0；x86_64 仍标记 `translated ARM64`，其他情况保持未分类，不凭设备名称推断。

## 回归结果

- 四部曲各前两页，共八页；真实 UI 点击“继续”。元数据、源文本行及最终 PNG 与既有桌面对照一致。
- 五个音频请求的原始字节 SHA-256 与独立 DAT 参考读取器一致；每个请求都有开始及非零播放进度证据。四次淡变均出现中间帧，并在规定时长后确认。
- 用户另行确认“能听到正常声音”。此为人工听感补充，与自动状态/哈希断言分开，不扩展为音质、声道、音量或低延迟验收。
- 在淡变中按 Home：取消效果，不发送迟到确认；在 BGM 播放中按 Home：释放播放器、清除运行时临时 Ogg、结束会话。两种情况 native close 均成功，VM 受管 live bytes 为 0。
- 真机 FFI 入口通过 32 轮变量/回调/panic/预算失败销毁、238+74+97=409 个分配故障点、四部曲标题及无效选择。每个对应检查点的受管 VM 内存回到 0。
- 同一个最终 APK 在原 x86_64/libnb 模拟器重跑四部曲八页、音频及淡变通过，仍明确标记翻译执行。
- Python research 8 项及修改后的验证脚本编译检查通过。Rust/C++ 核心本轮未修改；Android APK 已重新构建、安装并实际执行。Kotlin 音频焦点旧接口的既有 deprecated 警告仍保留。

## 证据

- `research/android-arm64-device-initial-audio-observation.json`：首次失败，旧 APK。
- `research/android-arm64-device-story-verification.json`：修正后的真机剧情、音频、淡变。
- `research/android-arm64-device-lifecycle-verification.json`：真实 Home 中断与销毁。
- `research/android-arm64-device-ffi-verification.json`：真机 FFI、预算和分配故障回归。
- `research/android-arm64-device-saf-verification.json`：持久授权重启及完整标题日志比较。
- `research/android-arm64-device-dat-verification.json`：17 个手机原始包校验。
- `research/android-arm64-device-emulator-regression.json`：最终 APK 的模拟器回归。

修正后的执行报告使用同一组 APK/native 库哈希。游戏 DAT、音频、图片、APK、签名文件及完整设备日志未提交仓库。测试资源保留在手机的专用验证目录以供后续回归。

## 尚未验收与后续顺序

当前后台策略仍是结束本次运行，没有恢复进度。没有覆盖进程强杀后的缓存清扫、来电/蓝牙切换、长期 BGM 循环、多音效混音、完整剧情、真实标题菜单交互或全进程内存泄漏。live-byte 计数只覆盖 VM 受管原生内存。

下一步继续运行器主线：扩大实际剧情命令和多图层覆盖，随后建立明确的暂停/恢复与存档状态契约。真机和模拟器保留两条回归路径，不因真机首轮通过而关闭原有错误路径测试。
