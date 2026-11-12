# Android 预览版发布指南

本轮面向直接分发的 Android ARM64 预览版，不是应用商店正式上架。源码许可和签名方案已确认；完成最终签名构建、真机验收和源码交付核对后，才创建版本标签与公开 Release。准备文档本身不代表已经发布。

## 2026-11-13 交付整理

`0.1.0-preview.1` 发布包使用 SHA-256 为 `c44f940415b1685ce343f68a47380bb729e06fe2d594203ef3494f4e9f80fa78` 的签名 APK。后续修改仅涉及视频工具、文档和交付材料，APK 构建源码与已记录的 `76f0af8` 交付基线一致。

本地发布目录为 `.local/releases/0.1.0-preview.1/`，包含 APK、采用《Carefree》配乐的介绍视频、对应提交源码 ZIP、音乐署名、发布说明和 SHA-256 清单。源码 ZIP 另收录 Cargo.lock 锁定且逐个校验 checksum 的 registry `.crate` 源码发行包；这些依赖保留各自许可。工具链与构建步骤见下文，不能把源码归档称为已包含 Android SDK/NDK 的离线构建环境。

签名候选已通过 PGKM10／Android 15 覆盖安装、游戏库可见记录保留、正常标题启动、后台恢复、菜单和发布入口限制检查。五款深度回归仍属于此前调试构建，不能视为本签名包已全量复测。详细证据见 `research/android-preview-release-verification.json`。此轮整理和推送源码，不自动创建公开 Release 或版本标签。

## 已确认的许可与签名方案

- 项目原创源码采用 `GPL-3.0-only`，仅 GNU GPL 第 3 版；完整原文见 [LICENSE](../LICENSE)。第三方组件继续保留各自的许可与修改声明，外部游戏资源及补丁不在本项目授权范围内。
- 本次沿用旧测试版签名密钥，保持相同包名。旧密钥已重新加密导入 PKCS12，导入前后证书 DER 完全相同，密钥和仓库外备份已设置用户专属目录权限；覆盖升级与数据保留仍须用最终签名 APK 验证。发布前比较新旧 APK 的签名证书 SHA-256，不用文件名判断签名身份。私钥与密码须备份，不放入仓库、视频、源码包或 Release。
- 二进制交付同时提供该版本完整对应源码的下载链接与归档：包含本项目源码、保留的第三方源码和声明、Cargo.lock、构建与安装脚本，以及取得所需依赖和工具的说明；不得以其他提交的源码代替。公开 Release 将源码归档与 APK 放在同一下载位置，并记录提交 ID、文件哈希及构建步骤。第三方许可快照不能代替对应源码。

## 构建

已验证环境的示例参数（请替换为自己的路径）：

```powershell
$buildArgs = @(
  '--ndk', 'D:/codex/tools/android/android-ndk-r27d',
  '--sdk', 'D:/codex/tools/android/sdk',
  '--jdk', 'D:/codex/tools/android/jdk-17.0.20.1+1',
  '--kotlin-lib', 'D:/codex/tools/android/gradle-8.7/lib',
  '--cargo', "$env:LOCALAPPDATA/RustGNU/cargo/bin/cargo.exe"
)
python platforms/android/build_probe.py @buildArgs --variant debug
```

调试验证包位于 `.local/android-probe/nekopack-arm64-probe.apk`，允许外部验证入口，仅用于开发和专用测试设备。确保 Rust 环境能找到对应工具链。

发布预览包需要显式签名参数：

```powershell
python platforms/android/build_probe.py @buildArgs --variant release `
  --version-code 1 --version-name 0.1.0-preview.1 `
  --keystore /private/path/release.p12 --key-alias YOUR_ALIAS `
  --password-file /private/path/password.txt
```

密码文件为 UTF-8 单行文本，位于调用者保护的目录，不写入终端参数或构建报告。当前接口使用相同的 keystore／key 密码。发布脚本不自动选择密钥，也不在缺少密钥时退回 debug 签名。

仅检查发布构建而不生成可安装签名包时，可生成**不能安装的检查产物**：

```powershell
python platforms/android/build_probe.py @buildArgs --variant release --unsigned
```

发布输出位于 `.local/android-release/`；`build.json` 记录版本、类型、是否签名、原生库与 APK 哈希。不得把 `*-unsigned.apk` 或 `aligned.apk` 当安装包发布。

## 发布构建的边界

- `debuggable=false`、`MainActivity exported=false`，仅游戏库作为外部启动入口。
- 预设版本 `0.1.0-preview.1`／versionCode `1`；后续 versionCode 必须递增。
- 保持历史包名 `org.nekopack.probe`，减少已有数据迁移；包名中的 probe 不代表发布版开启调试。
- 最低 API 23，当前 target 28，ARM64，构建检查 16 KiB ELF 对齐。提高 target SDK 必须另外回归目录授权、生命周期和界面行为。
- 运行诊断默认关闭，用户可主动开启；不要宣称诊断文件绝不含游戏内容。

## 交付核对

1. 项目 GPL-3.0-only 原文、第三方完整许可、来源与修改声明已齐；APK 内包含 `assets/licenses/project/LICENSE`。
2. 工作区测试与构建模式测试通过；真实资源回归按专门说明执行。
3. **用即将发布的签名 APK** 在真机从游戏库完成导入、新游戏、中文姓名、存读档、设置和后台恢复。调试包结果不能替代此项。
4. 检查覆盖升级保留数据；`run-as` 调试和外部直接启动 MainActivity 被拒绝，但游戏库仍能内部启动游戏。
5. 包内不含 DAT、游戏图片、存档、私钥；许可索引和完整文本存在。
6. 固定最终文件名、SHA-256、版本、提交 ID 与测试范围；同一 Release 提供该 APK 的完整对应源码归档及构建说明，再创建 Release。

备份功能目前是导出，不宣称已具备完整导入；`.nksv` 不等于原作 `.sav`。发布说明须区分已验证功能、历史构建结果与未覆盖设备／游戏。

## 相关资料

- [实现与验证记录](Android_Development_Progress.md)
- [回归脚本](../platforms/android/regression/README.md)
- 问题模板：`.github/ISSUE_TEMPLATE/bug_report.yml`
- 视频生成源码：`tools/promo/README.md`，成片在本地输出目录；不提交生成帧或系统字体。
