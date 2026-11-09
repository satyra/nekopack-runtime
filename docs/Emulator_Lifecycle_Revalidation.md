# 模拟器恢复开发基线（2026-11-09）

根据用户安排，后续暂不连接手机，使用已有模拟器继续 Android 移植。当前设备为 `emulator-5568`，内核 `x86_64`、native bridge `libnb.so`；安装的是 ARM64 APK，属于翻译执行，不是真机验证。

本次使用 batch 27 的同一 APK（SHA-256 `c1f10241d8fef4607e5af2a9f9fc6d14e0d9bc2062d2ccdceb8916e00d6634f2`），保留应用数据及 SAF 授权，读取 BLESS 原始资源。没有修改运行时实现，也没有改写已有真机报告。

`verify_lifecycle.py` 新增显式 `--execution translated`，默认仍要求原生 ARM64；两种模式分别检查实际内核架构和 native bridge，并写入报告。页面、像素、状态、音频和动作断言保持不变。

已通过 61 页 JSON/PNG 对照和 5 次后台往返：fade、movie、对白 BGM 两次、角色动作。验证同一 Activity/PID、暂停时有效时钟冻结、后台不派发 ACK/动作帧、同一播放器恢复、视频 Surface 重建后继续、动作完成及 VM 释放后存活内存为 0。报告见 `research/lifecycle-translated-revalidation.json`。

另通过前 12 页进入视频后的后台明确销毁检查，视频关闭原因为 3、解码器无错误、VM 释放后存活内存为 0，未进入第 13 页。报告见 `research/lifecycle-translated-destroy.json`。

UI 自动化曾报告一次临时 null root node，既有有界重试取得有效按钮后继续；没有放宽断言。此结果不代表模拟器上的完整 793 页回归、视频自然播完或声学听感验证，也不能替代原生 ARM64 的性能及设备兼容性测试。

下一步仍是共用运行时的 Config 分类音量和文字/语音点击语义。本次仅建立模拟器基线，没有声称这些功能已经接通。系统杀进程后的存档恢复仍不在当前活会话暂停/恢复的范围内。
