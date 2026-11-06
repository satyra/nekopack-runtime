# 变量、系统注册与标题执行链路

2026-11-06，跨平台方案通过后的 M1 增量。保留已有 DAT/MNG/像素算法，不建设桌面图形宿主。

## 实现范围

`runtime::Machine` 实现已观察到的变量声明、常量、赋值、调用返回、加载替换、系统请求及 map/case 选择。效果确认前不推进 PC；未知语句与系统 API 显式失败。新增支持样本中的 `?名称 = 整数`，仍不把复杂表达式当成已实现。

`SystemVm` 实际执行 Squirrel 2.2.5，注册 `com` 闭包并调用。`def(address, alias)` 解析 G/L 数值槽和样本 GF 标记别名；GF 名称以 `@` 访问，使用独立逻辑存储。相同地址别名共享值，重复相同声明不清零，冲突声明失败。GF 的原版位打包和自动解锁行为尚未证实，不作为本次兼容承诺。

Rust `Variables` 是唯一状态来源。共享系统桥的 var 代理通过 C ABI 同步读写它，没有第二份可漂移的 Squirrel 变量表。绑定仅在一次同步调用期间有效，成功/错误后均解除；VM 保持线程亲和，不引入平台 API。常量和未声明变量不能赋值，值限制为 i32 整数。

CLI 无窗口宿主加载实际系统模块，记录图层、填充、转场和声音请求；输入使用指定菜单项、时钟使用虚拟时间。BRAVE 标题需要的 Fill/转场入口已接通，但没有设备插值或音频输出。菜单之后执行 route、剧情入口赋值和 call，到首个剧情子脚本请求时停止并报告边界；没有跳过该请求或宣称后续对白已执行。

## 四部曲验证

新的 [title-verification.json](../research/title-verification.json) 记录二进制、fixture 与最终变量状态 SHA-256，不包含游戏脚本或图片。四款选择 1 均到达 story1，并进一步进入深度为 2 的剧情调用：BLESS/BLACK 为 P01，BIRTH 为 012401，BRAVE 为 010901。BIRTH/BRAVE 的 G800 剧情入口标记为 1。各款非法选择 999 必须失败，不能以退出码 0 掩盖输入错误。

Rust 测试覆盖实际系统注册、常量拒写、别名共享、双向即时可见、GF/G 隔离、重复定义、冲突定义、无绑定访问拒绝、错误后恢复和剧情继续读取。历史资源/像素报告保留；本批未改图像算法，未重复全量像素扫描。

本批验收：46 项 Rust/CLI 测试、8 项 Python 测试通过；fmt、Clippy（warnings deny）和 release 构建通过；新生成四部曲 fixture 的 4 条正常流程与 4 条非法选择检查全部通过。

## 构建与本地复现

当前已测环境为 Windows GNU Rust + Zig 0.14.1。先配置 Rust 环境（见 README），再把下列路径换成本机 Zig：

```powershell
$env:CXX='C:/tools/zig/zig.exe'
$env:AR=$env:CXX
$env:NEKO_CXX_ARGS='c++ -D__MSVCRT_VERSION__=0x700'
$env:NEKO_AR_ARGS='ar'
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
python -m unittest discover -s research -p 'test_*.py' -v
python research/prepare_title.py --games 'D:/owned-games' --reference-root 'D:/reference-project' --output .local/title
python research/verify_title.py --fixtures .local/title --cli target/release/nekopack-cli.exe --output research/title-verification.json
```

`prepare_title.py` 依赖外部参考工程的 `portkit.neko.Archive` 与已有 metadata 报告。它按来源优先级提取，遇同优先级重名拒绝，严格 CP932 转 UTF-8，产生实际素材几何与测试符号 ID；ID 并非原版数值 ABI。输出包含商业脚本，仅放在忽略的本地目录，不提交。此步骤是研究适配器，不是未来生产资源层；直接 DAT 输入属于 M3。

## 限制与下一步

仍未实现完整 VM、剧情显示、音频、存档、Android JNI/设备宿主。Squirrel 2.2.5 与原版 2.2.1 的完整兼容未建立，C++ 构建尚未完成 NDK/MSVC 适配；Rust 指令预算不能打断任意 Squirrel 死循环，样本验证由外部 30 秒超时约束。应用接入前必须解决这些边界，不把当前桥直接当成 Android 可发布组件。

下一批按 M2 隔离原生构建、验证 Android ARM64 ABI 与最小设备调用；继续共享运行器主线，不并行开发多个平台产品。
