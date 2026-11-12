# Android 本轮回归脚本

这些脚本由本轮 `.local/android-probe/input-test/` 中实际使用的版本迁入，测试资源、生成 fixture、原始日志和 APK 继续保留在 `.local/android-probe/`，不提交商业资源、正文、图片、存档或 APK。所有命令均从仓库根目录运行。

本轮固定 APK SHA256：`b9d41aa7699b10409bb552e404ede12fdbf92e32f1e35a96a2dbcc2d91e97cf6`。矩阵在每个子进程启动前核对本地与设备已安装 APK 的哈希；单项脚本本身不提供这一固定检查。

## 前置条件

- Python 3.11+（使用 `hashlib.file_digest`），安装 Pillow；共享设备探针为已跟踪的 `platforms/android/verify_menu_sounds.py`。
- 设备脚本中的 ADB 路径为 `D:/codex/tools/android/sdk/platform-tools/adb.exe`。ED 和资源库命名挂载脚本、sequels helper 的设备编号固定为 `emulator-5572`；在其他环境运行前调整脚本常量。
- 设备安装 `org.nekopack.probe` 调试包，并通过资源库选择 `/storage/emulated/0/Download/NekoPack-BLESS`。五部原作、相应设置与路由存档、隔离音频 fixture 必须已部署；这些脚本会操作应用设置、测试存档和进程，适合专用测试设备。
- 本地 `.local/android-probe/remaining-fixtures/index.json` 及其 `file` 指向的存档；路由设置测试将这些存档导入设备。`--plan-only` 会检查这些文件存在，但不会访问 ADB。
- 本地 `.local/android-probe/isolated-fixtures/manifest.json` 以及隔离 fixture 的实际文件。矩阵运行时按 manifest 检查设备文件 SHA256。
- 本地 `.local/android-probe/remaining-extra-voice-assets.json` 提供隔离脚本的角色 → cue 顺序，必须与当前设备隔离 fixture 一致。当前 BIRTH KA 应为 `KA_EP02_0000`。`--fixture-manifest` 仅覆盖矩阵的 fixture 预检和报告，不会改写该元数据或部署资源。
- 重建 fixture 另需 `.local/android-probe/remaining-route-index.json`（`dat` 指向合法持有的原作归档）、上述 extra voice 元数据，以及编译好的 `target/x86_64-pc-windows-gnu/release/nekopack-cli.exe`。BIRTH 原作 KA 还需 `.local/android-probe/birth-ka-original-resource-evidence.json`。
- ED 两个命名归档测试另外依赖 `D:/codex/td-port-workspace/游戏/Endless Dungeon/Endless Dungeon/` 下的 `script_chs.dat`、`system_chs.dat`，并要求设备目标目录起初没有这两个文件。测试结束会移除本次添加的文件。

## 不连接设备的校验

```powershell
python -m compileall -q platforms/android/regression
python platforms/android/regression/verify-five-game-voice-matrix.py --plan-only
python platforms/android/regression/verify-five-game-voice-matrix.py --plan-only --games BIRTH --fixture-manifest .local/android-probe/isolated-fixtures-birth-original/BIRTH-ENV-TEST/manifest.json
```

## 固定 APK 的五部矩阵

```powershell
python platforms/android/regression/verify-five-game-voice-matrix.py --serial emulator-5572 --apk .local/android-probe/nekopack-core-redirect-voice-arm64.apk --sha256 b9d41aa7699b10409bb552e404ede12fdbf92e32f1e35a96a2dbcc2d91e97cf6 --fixture-manifest .local/android-probe/isolated-fixtures-birth-original/BIRTH-ENV-TEST/manifest.json
```

默认报告目录为 `.local/android-probe/voice-matrix-时间-随机标识/`，包含 `summary.json` 和每个子任务日志。可用 `--output` 指定一个尚不存在的本地报告目录。该矩阵顺序执行独立子进程，覆盖真实设置点击、进程重启以及语音开启/静音两个状态。

可单独运行本轮各项（依赖相同资源和设备状态）：

```powershell
python platforms/android/regression/verify-remaining-settings.py emulator-5572 BIRTH audio
python platforms/android/regression/verify-isolated-original-audio.py emulator-5572 BIRTH
python platforms/android/regression/verify-ed-archive-redirect.py
python platforms/android/regression/verify-library-named-mount.py
```

`verify_voice_matrix_support.py` 输出结构化 `MATRIX` 证据；`verify-sequels-device.py` 是 ED 脚本通过 `runpy` 读取的已跟踪 helper。该 helper 自身运行生成的临时存档仍在 `.local/android-probe/input-test/`。

## 重建 BIRTH 原作 KA fixture

```powershell
python platforms/android/regression/build-isolated-audio-fixtures.py --games BIRTH --birth-ka-original --output .local/android-probe/isolated-fixtures-birth-original
```

构建器拒绝覆盖已存在的目标 fixture 目录。该命令会从本地归档提取选定音频，并复制运行所需原作归档，只能输出到本地忽略目录。加 `--deploy --serial emulator-5572` 才会部署；部署也拒绝覆盖设备上已存在的同名目录。需要先自行准备明确的空目标目录，不能用上述命令覆盖本轮证据。BIRTH 的覆盖 manifest 是 `.local/android-probe/isolated-fixtures-birth-original/BIRTH-ENV-TEST/manifest.json`。

## 历史矩阵与 KA 补测

`research/android-five-game-voice-matrix-final.json` 保留历史结果：86 个控制项、172 个状态通过，但当时 BIRTH KA 借用了 BLESS 资源，故 `all_original_audio_verified` 为 false。不能把这份历史矩阵当成 BIRTH 原作 KA 已通过的证据。

`research/birth-original-voice-device-verification.json` 是同一 APK SHA 下的独立补测：BIRTH FO/MY/KA 的原作音频、每次设置后的进程重启、静音/播放和非零增益；KA cue 为 `KA_EP02_0000`，音频 SHA256 为 `a4ec2d6cc1bf1abff0089d82574d8789e6ecdffae5b456e01906b5823f8cf21e`。原作来源证据见 `research/birth-ka-original-resource-evidence.json`。补测补充历史矩阵，不回写历史结论。

本次脚本迁移只进行静态编译和两个 `--plan-only` 检查，未因此重新执行设备测试。已有设备验证以对应 research 记录为准。

## 汉化姓名编码回归

`verify-translated-speaker.py` 验证真实汉化脚本中无法编码成 CP932 的姓名，设备端仅提取姓名、源脚本和行号，不传回正文或图片。`--phase all` 另外检查存档重启读取、Bold 修改后的重绘和恢复。测试补丁只在缺失时添加；已有同名档必须哈希匹配，结束只删除本次添加的文件。

```powershell
python platforms/android/regression/verify-translated-speaker.py --serial emulator-5572 --game BLESS --max-pages 40 --phase all
```

`--game` 支持 BLESS、BIRTH、BRAVE、BLACK、ENDLESS，补丁位置可用 `--patch-root` 和 `--endless-root` 指定。本项已在新姓名修复 APK `9f784ea0063783643be6386a890bfb863e2eca5aba42b82af357637d13723a7c` 上完成五款测试，详见 `research/translated-speaker-render-verification.json`；顶部历史语音矩阵的固定 APK 不因此更改。

真机目录可通过 `--remote-directory`（ADB 文件路径）和 `--document`（同一目录的 SAF document ID）指定，仍使用应用已有目录授权。PGKM10／Android 15 的 BLACK 汉化新游戏、存档重启和样式重绘已通过，见 `research/translated-speaker-black-phone-verification.json`。
