# Batch 15：BLESS 的 route 状态与电影后章节调用

本批按用户要求只用 BLESS 验证，共用运行器实现，不为其他三部建立不同的 VM 或指令分支。没有修改视频解码器、JNI 或 Android 宿主。

## 原版语义及实现

依据是原始 DAT 内的 `system/system.nut`、`system/saveload.nut` 和 `script/story1.txt`。`research/verify_route_batch15.py` 使用独立 DAT 读取器核对原始 CP932 资源与本地 UTF-8 fixture；仅消除历史 fixture 写入时增加的一层 CR，记录两侧哈希。未提交游戏脚本。

- `system.nut:17` 的 `com["route"]` 取首参数设置 `StoryRoute`，无参设置空字符串，然后执行 `UpdateCaption()`；额外参数不参与计算。`.title` 同样更新 `StoryTitle`。本批实际调用已从游戏资源注册的函数，并以 root table 作为接收者，没有用 Rust 重新猜测其行为。
- `InRoute()` 判断字符串是否不等于空串，不能 trim：BLESS 序章的全角空格 `　` 是非空路线，因此返回 true。`ResetRoute()` 只清空路线，不清空章节标题，也不立即刷新 caption。
- `.route` 本身不跳转、不清空变量、不写文件、不调用 `SaveState`。后续 `.call 012401` 才进入章节，原有调用栈和变量继续使用。
- 原版 `SaveInfo.Build` 收集标题、路线、时间戳和缩略图；写入顺序是缩略图、路线、时间戳、标题、消息日志，之后 `SaveData` 才调用原生 `SaveState`。
- `LoadData` 先读回元信息、恢复 `StoryTitle`/`StoryRoute` 并刷新 caption，再调用原生 `LoadState`；后者成功时等待并抛出 null，退出旧执行流程。普通/快速存档入口在 `InRoute()` 为 false 时直接返回。

Rust 新增 `StoryLabels`，作为原版 Squirrel 状态的只读观测快照，附在页面和错误 checkpoint 中；它不是另一份可修改的权威状态，也不是完整存档。初始化及每次 `.route`/`.title` 后刷新，使测试能够确认指令真的改变了状态。后续若增加 reset/load/其他可修改标签的系统入口，需要在对应状态修改边界刷新快照；原版 LoadData 在 LoadState 返回 false 时也已经恢复标签，不能只在返回 true 后刷新。

## 存档验证的准确范围

`nekopack-script/examples/route_semantics.rs` 真正加载并执行用户自有的完整 `system.nut` 和 `saveload.nut`。文件、缩略图、时钟及原生 `SaveState`/`LoadState` 使用明确的记录型测试替身，以断言原版写入顺序、禁用存档、元数据往返、恢复 caption 以及成功加载的控制流退出。测试调用命令时使用 root 接收者，和运行器调度一致。

这验证的是 `.route` 对存档元信息和权限的语义；没有实现完整的 VM/图像/音频持久化存档，也没有验证原版 `.sav` 的二进制兼容性。Android 仍不提供存档按钮，不能把该测试解释成“现在可以保存和读档”。

## 执行链路与范围

BLESS 序章 12 页保持原来的文本、场景、变量、步数和 PNG；新增字段只有标签证据。movie 后现在执行：`story1:12 .route 第１章` → `story1:13 .call 012401` → 子脚本 `.title ２４日目：水の曜日`。实际停机位置为 `012401:6`，调用深度 2、步数 205，错误为尚未实现的 `.filter 夜`。

此时 route 为 `第１章`，title 为 `２４日目：水の曜日`，caption 包含两者，`InRoute()` 为 true。这是已进入章节调用、尚未显示该章节第一句对白的边界。没有绕过 filter，也没有把这次单作验证称为四部曲回归。

## 验证与复现

- `cargo test --workspace`：包含 route 跨 movie 等待、拒绝非法 ACK、首参数/无参数、title 保留和后续未知指令停机。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`。
- `cargo run -p nekopack-script --example route_semantics -- <system.nut> <saveload.nut>`：原版脚本存档元数据语义验证。
- `research/verify_route_batch15.py`：BLESS 原始 DAT/fixture 一致性、原版语义执行、桌面 12 页和新增章节边界。
- Android `verify_story.py --games 0 --until-boundary --movie-completion natural` 及 `secondary`：分别验证电影自然结束和显式跳过后进入同一章节状态；使用原始 SAF 资源。

下一批应确认并实现 `.filter 夜` 的原版图像处理语义，之后继续 BLESS 章节对白。完整存档需要在 VM/场景可恢复状态及安全保存点明确后单独接入，不能用标签 JSON 代替。

## 最终结果

BLESS 在 PGKM10（Android 15，原生 ARM64）完成两次 12 页真实 SAF 回归。电影自然结束完整解码 2497 帧；主动跳过在 189 帧退出，两次均进入相同的 `012401:6` 状态，标签一致，受管 VM 内存归零。本批没有重跑模拟器或其他三部。

APK SHA-256：`6256831c28f40c07a70d0a5e7a69a5e3fcd072c65efd9f2f82124edb3ac09d29`。验证结果见 `research/route-batch15-{desktop,arm64-natural,arm64-skip,final,audio-evidence}-verification.json`。workspace 测试通过（runtime 26 项），workspace/all-targets Clippy 和格式检查通过。
