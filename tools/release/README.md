# Android Release 打包

从指定 Git 提交打包对应源码和已完成签名、验证的交付文件。脚本不下载依赖、不签名、不执行真机测试、不发布到网络。

```powershell
python tools/release/package_release.py --commit <完整提交ID> `
  --verification research/android-preview2-release-verification.json
```

默认版本为 `0.1.0-preview.2`，输出 `.local/releases/0.1.0-preview.2/`；输出目录必须不存在，避免旧文件混入。`--verification` 必填，必须包含匹配的 `version` 和 `apk_sha256`；构建 JSON 必须为相同版本的签名 release 并包含准确 APK 哈希。两份报告若包含 `source_commit`，必须与指定提交相同。打包器不会将历史动态测试自动视为新包已验证。

可覆盖参数：`--version`、`--output`、`--apk`、`--build`、`--verification`、`--notes`、`--source-check`、`--video`、`--credits`、`--upload-notes`、`--cargo-cache`、`--ffmpeg-dir`。`--notes` 接受 UTF-8 文件；省略时生成引用验证报告范围的说明。路径默认基于仓库目录；Cargo cache 默认是 `%LOCALAPPDATA%/RustGNU/cargo/registry/cache`。

`--source-check` 默认读取 `research/verification/save-continuity/source-git-sha256.json`，逐项校验构建输入哈希与所选提交 git archive 文件相同，并作为 `BUILD-SOURCE-INPUTS.json` 交付。它用于把已有构建输入证据绑定到最终提交；输入清单的真实性仍由实际构建与验证流程负责。

源码 ZIP 的项目文件全部来自 `git archive --format=zip <commit>`，包括锁文件、已提交 vendor、脚本和许可；工作区未提交内容不会混入。补充的 `dependency-sources/cargo/*.crate` 覆盖该提交 Cargo.lock 全部 registry 依赖，逐个匹配 checksum。FFmpeg 完整 tar.xz 必须匹配该提交 build_ffmpeg.py 固定 SHA-256，同时带上实际 `configure-android.txt` 和 `android/build-manifest.json`。不会以许可证快照代替源码。Git 外部依赖不支持时直接报错。

源码内 `SOURCE-MANIFEST.json` 记录项目逐文件哈希、git archive 哈希、外部源码哈希及数量；`BUILD-SOURCE.txt` 解释构建依赖和重建方法。脚本重开 ZIP 检查 CRC 和每个文件的哈希，拒绝游戏/存档/密钥常见后缀、本地数据目录和未审查的栅格图像。此路径检查是最低限度检查，不替代提交前的人工源码审阅。

外层包含 APK、视频、音乐署名、视频上传简介、GPL 原文、第三方声明、构建/验证报告、源码 ZIP、说明、manifest 和 SHA256SUMS。SHA256SUMS 覆盖除其自身外的全部交付文件；manifest 的 files 覆盖生成 manifest 前的有效载荷，避免循环哈希。

先完成最终源码提交，再构建/记录来源并用实际 APK 验证，最后运行本工具。Android SDK/NDK、Rust、JDK、Kotlin 和本地签名私钥不在源码包内，完整工具链另行安装。
