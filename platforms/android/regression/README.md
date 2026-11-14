# Android 本轮回归脚本

## 共用系统命令与音频（2026-11-13）

- `build-system-audio-fixture.py` 保留原系统与原 EXE ABI，仅使用合成音频和测试脚本；`verify-system-audio.py` 验证 BGV 双槽、普通语音抑制、MV 切换、分类静音、四变量银行、重置与快速存档冷恢复。默认不跳过冷恢复。
- `build-system-command-fixture.py` 保留原 `date` 处理器，另注册纯测试输入处理器；`verify-system-commands.py` 验证原计时返回、确认退出、两次独立输入等待、返回后的系统音效和剧情恢复。
- 两组设备脚本只读取技术元数据，不读取截图、正文或回想内容。运行前对显式指定的独立目录逐文件核对 SHA256；不会自动改写用户游戏目录或安装 APK。
- 夹具输出在 `.local/system-integration/`，含商业原系统资源，不加入 Git。设备部署、编译版本和每轮结果分别记录，不能用中间 APK 的通过结果代表后续源码已验证。

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


## 空恋原脚本排版与 NEXT / BACK

`verify-sorakoi-layout-navigation.py` 使用真实标题和原快捷菜单。先核验前三页的默认布局，再执行 NEXT 到首个选择、选择第一项、BACK 回到同一选择检查点、再次选择及额外 30 页。选择文本与游戏图不传回；正文只在设备上计算 SHA-256，报告包含字号、行距、锚点、裁切和选择检查点。

测试设备需已授权并导入空恋目录，使用 `pk02-validation` 游戏库标识。目录包含原 DAT、可选 PK02，以及原 EXE；EXE 仅供静态识别原生转场时间单位，绝不启动。原版入口排除 PK02；`--translated` 入口必须实际识别到汉化脚本。

```powershell
python platforms/android/regression/verify-sorakoi-layout-navigation.py --serial 127.0.0.1:5577 --pages 30 --output .local/sorakoi-import/navigation-android-original-final.json
python platforms/android/regression/verify-sorakoi-layout-navigation.py --serial 127.0.0.1:5577 --translated --pages 30 --output .local/sorakoi-import/navigation-android-translated-final.json
```

两条命令在同一设备顺序执行，不能并行操控 UI。菜单命中使用按钮中心，不点缩放后的边界；校验实际分发事件对应当前剧情页。长段 NEXT 的等待上限为 900 秒，不自动重试或跳过引擎错误。原版消息处理器仍可能有未覆盖的动画，WMV 常规播放也没有因 NEXT 越过影片而自动获得支持。

最终两版各 35 页、BACK 取消/确认与检查点恢复的同包记录见 `research/sorakoi-layout-navigation-verification.json`，APK SHA-256 为 `df7b9a5ba593245055e50ec9a12583c593a24904b99c41f5a1ca0a1e1d970811`。

## 共用 WMV 与剧情恢复

后续常规 WMV 接入见 `docs/WMV_Runtime_Playback.md`。三个脚本都只读取技术元数据，不抓取游戏截图、正文、回想或原始视频帧：

- `verify-movie-metadata.py --serial SERIAL --media LOCAL_MEDIA --output REPORT [--pause]`：直接验证同一 SAF 目录中同名媒体的真实播放，核对源文件 SHA、解码 EOF、非零音频、播放头、A/V 时间差与完整耗时。此结果不表示剧情脚本已经调用过电影。
- `verify-sorakoi-movie-chain.py --serial SERIAL --save NKSV --media LOCAL_MEDIA --output REPORT [--translated]`：导入运行时生成的电影前存档，执行实际脚本电影请求，等待自然结束，再确认进入 `CS01_C_07`。使用隔离的测试存档目录。
- `verify-sorakoi-resume.py --serial SERIAL --save NKSV --output REPORT [--translated] [--pages 100]`：恢复此前进度，继续普通对话，调用快捷存档并冷启动读回。使用实际版本存档目录的空闲第 79 槽，若已存在不同内容则拒绝覆盖；快捷存档保留给后续继续。

`crates/nekopack-runtime/examples/movie_checkpoint.rs` 通过正常脚本 ACK 生成检查点；它的桌面电影 ACK 仅推进状态，不算视频播放验证。原版/汉化需使用各自的存档和媒体。同一设备上的这些脚本必须串行执行。旧 `platforms/android/verify_movie.py` 已改为读取指定帧摘要和音频计数，不再依赖常驻 RGBA/PCM 文件或截图。

## 共用原音乐房

`verify-original-room-music.py` 仅允许明确指定 `127.0.0.1:5577`。它不安装 APK、不导入游戏、不复制数据：运行前准备统一 APK、可访问的原 DAT 入口，以及 BGM 名称到原音频 SHA256 的 JSON 字典。脚本先核对已安装 APK 与 `--apk`，再按 metadata bounds 点击第二曲、PLAY、NEXT、BACK、STOP，重播后退出。验证 `room_music_count` / op74 通路、原音频 SHA、非零播放头、实际播放器循环标记、增益、停止后播放器与缓存释放；只传技术元数据，不传截图、UI 原始 XML、剧情或回想正文。

```powershell
python platforms/android/regression/verify-original-room-music.py --serial 127.0.0.1:5577 --apk APK --tree AUTHORIZED_TREE_URI --directory GAME_DOCUMENT_ID --title-game 0 --room-action 8 --audio-manifest ORIGINAL_AUDIO_HASHES_JSON --output .local/room-music-verification.json
```

也可传 `--library-entry`；该模式须已启用应用诊断以生成 host/gain 元数据，或直接以 `--tree/--directory` 启动隔离诊断。`--plan-only` 仅输出输入配置，不访问设备。`--master-percent` 对应宿主音量（默认 100）。原标题不存在已启用音乐按钮时脚本明确失败，不添加菜单；预先构建的隔离引擎命令入口应使用 `--isolated-command-fixture`，结果单独标明，不作为原标题入口验收。

准备时只读检查 5577 的 Download 和游戏库注册元数据，当前仅见 NekoPack-Sorakoi，没有 TD4。TD4 本地原档位于 `D:/codex/td-port-workspace/游戏/[120727][Rosebleu]Tiny Dungeon ～BRAVE or SLAVE～ [汉化硬盘版+RR5]/[120727][Rosebleu]Tiny Dungeon ～BRAVE or SLAVE～`。避免复制 image.dat / voice.dat：原 system.dat 为 25,559,659 字节，已含原系统脚本、`x_music.txt`（325 字节）和 `x_music.mng`（895,104 字节）。第二曲是 BGM022（4,597,278 字节，SHA256 `f0a213a04fe33decb0c59ea6a09b99a3a36462ce0c035ef6055bcd6b662aadeb`），第三曲是 BGM001（3,699,378 字节，SHA256 `815b3f23e190eee7cc0b5e07e3127b2bce1bd7be4f8aeb29eaaa8c87adbf6d8f`）；连同原 se_PAGE / se_CLICK / se_OK / se_SELECT 可构建约 34 MB 的有限夹具，保留完整原音乐表与原处理器，用隔离脚本提供调用入口。此段仅列准备清单，未因此复制或安装资源。

`build-original-room-music-fixture.py` 已把上述准备过程固化：保留原 system.dat，另将前三曲（包含原 PLAY 空选中时需要的首曲）和四条 SE 打入小 DAT，仅合成 start/title 调用入口，不改原 `x_rooms.nut` / `x_music.txt` / `x_music.mng`。产物 DAT 共 36,236,415 字节。构建目标和设备目标均拒绝覆盖；`--deploy` 只推文件并核对 DAT SHA，不启动应用。

```powershell
python platforms/android/regression/build-original-room-music-fixture.py --output .local/system-integration/music-fixture
# 需要明确部署时使用新目标目录运行，并加 --deploy；已有目标拒绝覆盖。
```

本轮小夹具已在 `127.0.0.1:5577` 的 `/sdcard/Download/NekoPack-Sorakoi/NekoPack-Music-Probe` 部署。桌面 `original_music_room_command_plays_and_switches` 对这两个 DAT 验证 1 项通过；此记录不等于 Android 播放通过，Android 脚本仍待统一 APK 后串行运行。对应运行参数为 `--tree content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi --directory raw:/storage/emulated/0/Download/NekoPack-Sorakoi/NekoPack-Music-Probe --title-game 0 --room-action 1 --isolated-command-fixture --audio-manifest .local/system-integration/music-fixture/audio-hashes.json`。`--apk` 必须指向本轮统一 APK。

### 原分支与嵌套确认框增量验证（2026-11-13）

`build-system-command-fixture.py` 现额外生成合成 PNG mask 和 dialog MNG，保留挂载包原 `ViewDialog`；合成双参数处理器等待该原确认框返回后选第二 case。`verify-system-commands.py` 使用实际安卓触摸坐标确认，再验证延迟快速存档及冷恢复。夹具不读取剧情或原图片；不要把该验证称为原 choice 控件或完整设置页已全覆盖。结果见 `research/shared-system-branches-verification.json`。

### 真机覆盖升级

`verify-device-upgrade.py --serial <真机序列号> --apk <最终APK> --output <新的本地私有目录>` 先停止应用并备份 `files/saves` 与 `shared_prefs`，再执行 `adb install -r`，核对安装包及升级前后全部存档、配置文件摘要。输出目录须不存在，建议放 `.local`；备份包含用户数据，不提交仓库。脚本拒绝本地模拟器和同 APK 重装，不使用卸载、降级或清除数据来绕过签名／版本限制。旧包必须支持 run-as，否则在安装前停止，不能跳过备份。

安装验证通过仅证明数据在升级时保留；还必须用 `verify-existing-save.py` 在该真机实际读取旧档，随后分别验证原版、汉化版连续推进。2026-11-14 未发现 USB 真机，用户选择先跳过真机；脚本仅完成语法与参数检查，不能记为覆盖升级通过。

### 真实旧档连续推进与新档冷读回

`verify-existing-save-continuity.py --serial <设备> --entry <游戏库ID> --scope <存档目录名> --apk <安装包> --pages 200 --output <报告>`；原版另加 `--original`，汉化版 scope 包含 `_zh`。脚本只读取旧档五字节版本头和摘要，将最新快捷档复制到新的 `save_probe` 目录，用原场景运行、按页面完成推进、按元数据选择第一项。末尾通过原快捷存档动作保存并冷启动读取，对比脚本位置，核验原存档目录前后全部摘要一致。报告不包含剧情、选项正文或图片；脚本不接入回想入口。

`verify-existing-save.py` 增加 `--save-probe`，可只读复测此前隔离长流程生成的档。`saved_title_resume` 桌面示例用于复现标题读档错误，要求 `NEKO_TEST_FONT`，可指定与 Android 相同的挂载文件顺序；仅输出准备／恢复状态，不打印页面内容。

### 原 sysload / continue / room 真实设备回归

以下脚本使用同一个已安装 APK，真实操作原系统页面；只读取按钮、状态、存档及音频哈希，不获取截图或原剧情正文。先用对应 `build-*-fixture.py` 构建并部署独立夹具，不能将夹具覆盖到用户游戏目录。

- `verify-native-load.py --serial <设备> --output <报告>`：实际生成 NKSV，构造普通槽与快捷槽不同的修改时间；原 LOAD 取消/确认、真实恢复、continue 选最新普通槽、冷启动重测、空目录分支。所有写入限定在本次独立 save_probe。
- `verify-system-room.py --serial <设备> --apk <APK> --output <报告>`：原音乐页选曲、NEXT/BACK、滑块、STOP、退出恢复标题曲，验证实际播放器进度、循环标志、增益和 Ogg 哈希。
- `verify-system-room-boundaries.py --serial <设备> --apk <APK> --output <报告>`：原 CG 的 ImageAction/Commit/Render 使用生成的纯色素材；原回想条目只验证 ScriptEval 请求，不执行原正文。
- `verify-system-room-replay.py --serial <设备> --apk <APK> --fixture <夹具目录> --output <报告>`：夹具须以 `build-system-room-fixture.py --boundaries --synthetic-replay` 生成。仅打包合成绘制/等待指令，走生产 op82、EOF、原父协程恢复、窗口重绘及 CLOSE 返回。核验平台回想模式进入/退出，禁止夹带原剧本档。

夹具保留原 system.dat 和原技术处理器；合成入口及固色素材仅隔离调用路径，不替换原页面逻辑。这些测试不等于逐款游戏全部内容通关，也不证明原 Windows `.sav` 二进制互通。加载字段顺序由当前游戏原 `SaveInfo.Save` 写出，宿主不按游戏名称硬编码。

### 原 config 同协程设备回归

先运行 `build-system-config-fixture.py`，将独立config-fixture四个文件挂载到 `NekoPack-Config-Probe`（与用户游戏/存档分开）。`verify-system-config.py --serial <设备> --output <报告>` 通过实际触摸执行原主页面、三个子页、快捷键弹出选择、SaveConfig ACK、冷启动、DEFAULT确认与初始化；只读取按钮坐标和NKCF配置，不读取画面。新NKCF13解码明确校验十二槽动作，避免只比对旧F3–F12而漏检F1/F2。

开发定位可使用 `cargo run -p nekopack-runtime --example system_config_probe`；输出仅技术元数据，输入 event [x y]，4为完成配置写入确认，5为无游戏事件的消息唤醒；不导出图片。
