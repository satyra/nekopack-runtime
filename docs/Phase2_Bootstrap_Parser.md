# 启动脚本解析首个增量

本轮回到运行器主线：连接资源读取与脚本 AST。没有继续调整截图颜色，也没有创建假执行器。Phase 1 的动画/显示验证尚未全部完成，本增量也不代表 Phase 2 完成。

## 实现范围

新增无外部依赖的 `nekopack-script`，提供 `parse_bootstrap(bytes, max_statements)`。语句携带物理行号、原始字节区间、原始 token 和结构化 AST：

- `.def name decimal`：常量声明的语法节点。
- `.var G/L地址 name...`：变量组声明的语法节点。
- `.call target`、`.load target`、`.system file`：分别保留独立节点，不将调用与跳转混为一谈。
- `? name = decimal`：本机启动补丁脚本出现的赋值形式；不代表支持一般表达式或条件语句。

支持 LF/CRLF、空行、ASCII 空格/tab、整行分号注释。名称保留原始编码字节，不进行有损 Unicode 转换；CP932 双字节字符尾部的 `0x5c` 不会被误判为转义符。数字有明确范围（i32/u32）。未知语句、引号语法、行内注释、复杂表达式和超出预算的输入直接报错，携带行号、字节位置与 token hex。

这是明确的有限语法，不是完整文本脚本解析器，更不是 `.nut` 解释器。声明、赋值、资源调用的**运行时行为尚未执行验证**；AST 分类不能代替语义证据。

CLI `parse-bootstrap FILE ENTRY` 直接从 DAT 条目读取并解析，输出 TSV（行号、起止字节偏移、各 token hex）。完整解析成功后才输出，未知命令不会产生看似成功的部分 AST。CLI 的资源输入预算 64 MiB、语句上限 100,000；库的语句上限由调用方提供，源字节预算由调用方控制。

## 四部曲证据与验证

`research/verify_bootstrap.py` 校验原有固定样本的全部 DAT 哈希，通过现有名称哈希/最高 priority 策略查找 `script/start.txt`，独立逐行对照 token 和位置。报告不包含商业脚本全文或对白。

BLESS、BIRTH、BRAVE、BLACK 的 start 候选分别包含 **19、19、27、18** 条语句，共 **83** 条。它们直接引用的两份补丁脚本每份有一条本轮支持的赋值，四作合计 **8** 条。全部 **12 份脚本、91 条语句**通过解析与字节位置差分。

直接依赖合计 **53 次引用**：41 次系统 `.nut`、8 次补丁脚本、4 次标题目标；全部唯一解析到来源条目，记录封包相对路径、entry、priority、脚本 SHA-256、引用行与字节位置。这里验证的是当前资源解析策略，不等于再次证明原版所有动态挂载顺序。

三作的最终 `.load` 指向 title，BRAVE 指向 titlelogo。进入这些后续脚本时，本轮 parser 在不支持的命令处明确停止，CLI 非零退出且不输出部分 AST；没有以“全部跳过”制造启动成功。

41 份加载引用中的直接 `com["名称"] = function` 注册行也被静态记录：三作各观察到 11 个注册，BRAVE 为 13 个。包括 `map`、`select`、`config`、`sysload`、`room` 等。匹配只覆盖已经观察到的直接赋值形式，不是完整 Squirrel AST，也不宣称动态注册完整率。

这些证据表明，标题与选择界面依赖系统脚本注册机制，后续 VM 必须连接这层能力；不能把所有点命令都当作固定的原生 opcode。原版是否必然从 start.txt 开始以及原生解释器的准确版本仍需进一步证据。

## 验收与下一步

41 项 Rust/CLI 测试、8 项已有 Python 测试通过；fmt、clippy（`-D warnings`）、release build 通过。合成测试覆盖字节编码、位置、数字范围、错误语法、预算与无部分输出；真实样本回归报告为 `research/bootstrap-verification.json`。

下一步优先确认声明/赋值/调用的运行状态，以及 `.system` 注册表与引擎宿主 API 的边界，再接标题菜单的 `map` 调用。执行语义不明确的指令必须停止并报告。完整 VM、系统脚本解释、剧情调度、音频、Android 工程和 APK 都尚未交付。

```powershell
cargo run -p nekopack-cli -- parse-bootstrap path/to/system.dat ENTRY
python research/verify_bootstrap.py --games 'D:\codex\电脑移植安卓\游戏' --cli target/release/nekopack-cli.exe --output research/bootstrap-verification.json
```
