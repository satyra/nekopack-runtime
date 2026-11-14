# NekoPack Runtime

在 Android 上运行 NekoPack 游戏资源的开源兼容运行时。已验证样本包括 Tiny Dungeon 四部、Endless Dungeon 和空恋（Sorakoi）。共享 Rust 核心负责封包、脚本和演出语义，Android 宿主负责游戏库、输入、显示、音频与目录授权；标题、设置、存读档等界面沿用原版系统脚本。

**当前阶段：Android ARM64 开发预览版。** 这是持续开发的兼容项目，不代表所有同引擎游戏均可运行，也不是原游戏或其厂商的官方产品。

## 主要功能

- 游戏库、最近游玩、搜索、扫描导入和单游戏详情。
- 原版菜单与设置、存读档、自动播放、快进、语音／BGM／环境音控制。
- 右侧快捷菜单、鼠标与手势设置；独立快捷面板默认关闭。
- 已验证汉化档的加载、中文姓名显示、原版／汉化版存档区分。
- 空恋 PK02 汉化补丁加载、文字排版与 NEXT/BACK、WMV 播放。
- 目录重新授权和存档**导出备份**；目前不宣称已实现完整备份导入流程。

项目不提供游戏、汉化补丁、CG、语音或字体文件。请自行准备有权使用的资源。

## 安装与导入

1. 使用 ARM64 Android 设备，最低 API 23。当前验证包含 Android 9 模拟器及 Android 15 真机，不代表所有设备均已覆盖。
2. 从项目 [Releases](https://github.com/satyra/nekopack-runtime/releases) 获取明确标注版本和 SHA-256 的安装包。发布准备期间，开发记录中的验证包不能自动视为正式发布包。
3. 在游戏库选择“添加游戏”，通过系统目录选择器授权游戏目录。
4. 核对扫描结果和补丁识别情况，再启动。保留原始文件结构，不要把多个游戏的 DAT 混在同一目录。

升级前建议从游戏详情页导出存档备份。相同包名、相同签名的更新可以覆盖安装；**不要先卸载旧版**。本项目存档为 `.nksv`，不是原作 `.sav`；更换资源或补丁可能影响存档身份校验。

## 已验证范围

样本包括 Tiny Dungeon 的 BLACK（黑白羽翼）、BLESS、BIRTH、BRAVE，Endless Dungeon，以及空恋（Sorakoi）。

- BLESS 在一次已记录的构建中完成一条主线；不能由此推断其他四款均已通关。
- 已记录五款角色语音矩阵、原版设置及重启恢复测试；BIRTH KA 原录音的缺口已另行补测。
- 最新姓名修复构建验证了五款汉化版的实际中文姓名页、存档重启和文字样式重绘；BLACK 中文新游戏还通过 PGKM10／Android 15 真机复测。
- 不同构建的结果分开记录。历史 50 次冷启动／100 次恢复、86 个语音控件／172 个状态，不能移用为以后每个 APK 的验收结论。
- 2026-11-14：空恋原版、PK02 汉化版在同一调试 APK 上各从 NKSV5 旧档推进 200 页，完成选择、新档保存和冷启动读回；原存档未改变。这是连续流程验证，不是整条路线通关。
- 本轮修复长流程新档恢复超过 VM 源码限制的问题；此前失败的两份存档直接恢复成功，原 sysload／continue 32 步回归通过。详见 [长流程与恢复验证](research/save-continuity-verification.json)。本轮尚未完成真机覆盖升级验收。

版本、APK 哈希和限制见 [Android 开发记录](docs/Android_Development_Progress.md)。测试脚本与资源前置条件见 [Android 回归说明](platforms/android/regression/README.md)。

## 项目介绍视频

新版展示片在原有 BLACK 标题、普通开场和启动器操作镜头中加入空恋原版、汉化版标题实录，沿用《Carefree》轻快配乐。所有画面均标注 Android 模拟器录制，旧镜头与新增镜头的 APK 来源分别记录；不包含回想内容，也不用于比较启动性能。

制作说明、可复用配置及音乐署名见 [视频说明](tools/promo/SHOWCASE.md)。仓库提交制作代码与验证记录，不附带商业游戏录屏、音乐或游戏资源。

## 当前限制

- 仅交付 Android ARM64；Windows、Linux／Steam Deck、macOS 属于后续方向。
- 当前 target SDK 为 28，编译 SDK 为 35。尚未按应用商店要求完成 target 升级和渠道验收；这次准备面向直接分发的预览版本。
- 汉化识别仅覆盖已有证据的补丁格式，不保证任意补丁或任意游戏自动兼容。
- 真实样本测试需要本地资源和参考工具，仓库不会附带这些内容。
- 历史一次启动超时缺少原始启动日志，根因仍未确认；后续批次未复现，不将其描述为已定位修复。

## 构建与测试

需要 Rust、C++ 编译／归档工具。Android 还需 NDK、SDK、JDK 和 Kotlin 编译器。当前 Android 构建脚本使用 Windows 工具路径，不能把命令原样当作 Linux 构建步骤。

```powershell
cargo test --workspace
python -m unittest discover -s research -p 'test_*.py'
python -m unittest discover -s platforms/android -p 'test_build_variants.py'
```

发布预览构建与可调试验证构建已分开。发布构建关闭调试和外部测试 Activity 入口，不会自动生成发布签名。命令、签名与交付核对步骤见 [发布指南](docs/RELEASING.md)。

资源检查示例：

```powershell
cargo run -p nekopack-cli -- inspect-archive path/to/system.dat
cargo run -p nekopack-cli -- inspect-overlays path/to/system.dat path/to/patch.dat
```

`read-entry` 等命令输出原始二进制，不应经过会转码的文本管道。研究记录保留在 [封包规格](docs/NekoPack_Archive_Specification.md)、[脚本分析](docs/NekoPack_Script_Analysis.md) 和 [架构](CROSS_PLATFORM_ARCHITECTURE.md) 中。

## 反馈问题

请提供安装包版本／SHA-256、设备与 Android 版本、游戏及补丁版本、新游戏或读档、触发步骤、错误文字。不要上传完整游戏资源、个人存档或整段剧情。可使用 [问题模板](https://github.com/satyra/nekopack-runtime/issues/new/choose)。

## 许可与第三方组件

项目原创源码采用 **GNU GPL v3，仅第 3 版（`GPL-3.0-only`）**，完整条款见 [LICENSE](LICENSE)。第三方组件保留各自的许可与声明；此许可不涵盖外部游戏资源或汉化补丁。分发 APK 时，应同时提供该版本的完整对应源码、构建脚本及依赖说明，具体交付步骤见 [发布指南](docs/RELEASING.md)。

第三方来源和修改说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。Android 包内附对应第三方许可快照，不包含商业游戏资源。实录视频的游戏画面与配乐保留各自权利及署名，不适用项目源码许可。
