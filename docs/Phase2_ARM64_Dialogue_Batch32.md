# Batch 32：Android 15 ARM64 真机对白回归

验证设备为 PGKM10，Android 15，内核架构 `aarch64`，ABI 包含 `arm64-v8a`，native bridge 为 `0`。本批是原生 ARM64 手机结果，独立于 Batch 31 的 x86_64/libnb 翻译执行记录。使用已有 SAF 授权读取 BLESS 原始 DAT，未切换到预解包资源。

## 真机发现与修复

手机的 `/system/fonts/NotoSansCJK-Regular.ttc` 为 **32,355,424 字节**，超过旧版 24 MiB 输入限制。安装 Batch 31 APK 后确实得到 `font exceeds 24 MiB input budget`，不能进入对白。

将字体输入上限统一为 Rust 公共常量 `MAX_FONT_BYTES = 40 MiB`，JNI/FD 读取沿用该常量并最多读取上限加一字节，以便明确拒绝超限输入。测试检查超限返回的具体错误，而非仅检查任意失败。40 MiB 是字体文件输入限制，不是进程总内存上限；原有字形缓存和图像预算未调整。没有新增字体资源，也没有将手机字体打包进 APK。

## 验证器修正

画面及交互验证器增加 `--execution native|translated`，断言设备架构和 native bridge，报告明确记录实际执行类别。

手机视口的缩放为 `7/6`，水平起点为 `507⅓` 像素。旧截图比较先合成再缩放，并将视口取整；这与 Android 对背景、预乘 Alpha 覆盖层分别进行双线性采样后合成的行为不同。旧比较出现 MAE 4.31、95 分位误差 26，未记为通过。修正比较模型后，三页截图 MAE 分别约 **0.243、0.242、0.245**，95 分位约 **0.51**；保留原有 MAE<5、95 分位<20 的门槛，未放宽阈值。

新增合成色块、缺失覆盖层、单列脉冲、非整数视口及错位负例的解析测试。真实截图比较仍独立于桌面/Android 覆盖层逐像素比较，避免只验证导出的 PNG 而没有验证实际屏幕。

交互测试的两次未通过尝试也未计入成功：一次恢复前台后的测试点击没有出现在应用日志；另一次 UIAutomator 等待辅助功能空闲时，文字已自然显示完，错过了部分显示窗口。最终测试将职责分开：第 2、4 页断言第一次点击补全、第二次点击推进；第 3 页独立检查后台冻结、恢复后的部分文字、自然完成以及随后单次点击推进。每次补全文字都必须观测到应用点击事件，不能以自然完成作为点击成功证据。恢复前台后重新获取按钮坐标。

## 验证记录

- `research/dialogue-render-batch32-phone-verification.json`：三页具名/匿名/换行对白，手机字体在桌面与手机产生完全相同的覆盖层像素；实际手机截图通过比较，关闭后 VM live bytes 为 0。非法 MNG 坐标的桌面错误路径继续通过。
- `research/dialogue-render-batch32-phone-controls.json`：KeepVoice 0/1 两组各五页，检查逐字、两阶段点击、后台文字计时、语音保留/停止以及显示未完成时关闭。
- `research/dialogue-render-batch32-phone-lifecycle.json`：BLESS 前 61 页、五次后台/恢复，包含视频后的剧情、BGM 与角色动作，核对页面元数据和场景 PNG，检查 VM 释放。
- `research/dialogue-render-batch32-phone-audio.json`：前 13 页的真实系统音频焦点转移/恢复、重复淡出以及 VM 释放。
- `research/dialogue-render-batch32-phone-destroy.json`：推进前 12 页进入视频，退后台触发会话销毁，检查视频关闭原因、无后续剧情推进、无迟到事件、音频释放与临时音频文件清理。
- `research/dialogue-render-batch32-checks.json`：构建、测试、字体大小与初次失败记录；用户本轮明确反馈“声音正常”，与播放器状态验证分开记录。

Rust workspace 的 **124 项顶层测试及 7 次隔离子进程探针**、Clippy 严格检查、17 项 Python 验证器测试及 NDK ARM64 构建通过。顶层列表中作为子进程入口的 `native_probe` 标记 ignored，实际由七个安全测试启动执行；不将其重复算作独立顶层用例。历史记录中的 131 是顶层与子进程执行次数合计。各手机报告核对同一 APK SHA-256。

## 保留的边界

字体仍为宿主 Noto CJK 替代，不能据此宣称 MS Gothic/GDI 字形一致。富文本、ruby、日文禁则及完整游戏菜单仍未完成。本次未重跑四部曲或完整长篇剧情，结果仅覆盖上述 BLESS 范围；后续设备回归优先使用此 ARM64 手机。
