# Android ARM64 验证宿主

这是 Kotlin → JNI → 同一套 Rust/Squirrel 核心的最小验证宿主，不是游戏播放器。APK 仅含 `arm64-v8a`，不包含游戏资源。JNI 调用在单个后台线程内创建、执行并销毁 VM，不跨调用保存 VM 指针或 Rust 借用。

`native` crate 只用于验证，显式启用 `nekopack-script/validation` 故障注入功能；普通 workspace 默认成员不包含它。后续产品宿主不应启用该 feature。

## 构建（Windows）

需要 Rust GNU 及 `aarch64-linux-android` 标准库、Android NDK r27d、SDK platform 35/build-tools 35.0.0、JDK 17，以及 Kotlin 1.9.22 编译器/标准库 jars。本机复用 Gradle 8.7 的 `lib` 目录提供 Kotlin 编译器，不需要下载 Android Gradle Plugin。

```powershell
$env:RUSTUP_HOME="$env:LOCALAPPDATA/RustGNU/rustup"
$env:CARGO_HOME="$env:LOCALAPPDATA/RustGNU/cargo"
$env:Path="$env:CARGO_HOME/bin;$env:Path"
rustup target add aarch64-linux-android
D:/app/python/python.exe platforms/android/build_probe.py `
  --ndk D:/codex/tools/android/android-ndk-r27d `
  --sdk D:/codex/tools/android/sdk `
  --jdk D:/codex/tools/android/jdk-17.0.20.1+1 `
  --kotlin-lib D:/codex/tools/android/gradle-8.7/lib `
  --cargo "$env:CARGO_HOME/bin/cargo.exe"
```

脚本为该子进程设置 NDK 编译器和 Rust target linker，清除通用 RUSTFLAGS，显式设 API 23 和 ELF 16 KiB 对齐。产物在 `.local/android-probe/`：测试 APK、本地 debug 签名、ELF 检查与 SHA-256。NDK `libc++_shared.so` 随测试 APK 打包，两层 native 库均有可检查的 DT_NEEDED 依赖。APK 使用解压 native libraries 的安装模式，检查 ELF 对齐不等于已验证 16 KiB 设备运行。

构建使用的 Kotlin 标准库遵循其 Apache-2.0 许可；NDK C++ runtime 的许可声明随 NDK `NOTICE`/`NOTICE.toolchain` 提供。Squirrel 源码许可及改动清单仍保留在 `vendor/squirrel2`。这些工具链和测试二进制不纳入 Git；产品发行前须连同实际分发依赖整理相应 notices。

## 设备验证

先运行桌面 `research/verify_title.py` 得到可信参考结果，再执行：

```powershell
D:/app/python/python.exe platforms/android/verify_probe.py `
  --adb D:/codex/tools/android/sdk/platform-tools/adb.exe `
  --serial emulator-5568 `
  --fixtures .local/title-verified `
  --reference research/title-android-preparation-verification.json `
  --output research/android-arm64-probe-verification.json
```

验证器安装/更新 `org.nekopack.probe`，使用每次唯一的私有目录复制 fixture，并从设备读回逐字节比对。随后运行 32 轮 FFI/预算失败销毁、逐分配点故障注入及四部曲标题链路。每次结果带唯一 run ID，文件原子发布；旧 adbd 的 exec-out 退出码不能单独作为成功依据。

运行结束后，脚本只清理该次 `/data/local/tmp` 暂存目录；应用私有目录中保留测试副本与日志，供本机排错。公共 JSON 不含游戏脚本，只记录工具链、库哈希、环境、测试状态及剧情边界/变量状态哈希。APK、签名和游戏文件不提交。

本次在线设备是 x86_64 Android 9，通过 `libnb.so` 翻译 ARM64。真机、16 KiB 页面设备、SAF、音频和图像演出均不属于已通过范围。详细结果见 `docs/Phase2_Android_ARM64_Batch4.md`。
