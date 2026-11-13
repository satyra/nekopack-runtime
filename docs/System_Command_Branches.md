# 原 com 参数与场景分支契约

2026-11-13。静态读取原 Sora PE，交叉检查 ED 的注册 ABI；没有执行原 EXE、查看图像、播放音频或输出剧情正文。复核片段：`.local/sorakoi-import/analysis/choice-native-evidence.txt`，含 Sora EXE SHA-256；完整技术反汇编在同目录 `choice-com-dis.txt`、`choice-compiler-dis.txt`、`choice-state-dis.txt`。

## 分支由注册闭包的参数个数决定

Sora `com._set` 入口 `0x5953F8`（绑定 `0x55C3A9`），注册 Squirrel closure 时调用 `sq_getclosureinfo`：`0x596882 → 0x69BC88`。`0x596890–0x59689E` 比较 parameter count 与 2，将 `count > 2` 写入 command 的 `+0x80`。参数数包含隐式 `this`，所以 `function(args)` 不开分支，`function(args,list)` 开分支。不是检测名字 `choice/select/map`，也不是检测后文有没有 `.case`。

ED 对应 `0x58B586 → 0x443E58`，`0x58B594–0x58B5A2` 同样比较 2 并写 `+0x80`。两个原系统的 `choice(args,list)` 均属于此机制。参数有默认值或更多参数时，原判断仍只是声明参数数量；本次没有为原函数调用失败添加兼容处理。

Sora 通用 command 的方法表 `0x765290`：parse `0x55F84C`、execute `0x56B900`。这不是 `.system` 专用 `SystemCommand`（parse `0x5AB5EC`、execute `0x5AD838`）；两者不要混淆。

## args、rows 与返回值

普通命令名后的参数构成 `args` 数组。声明 `list` 的命令另外消费后续的行数组，即使没有任何行，也必须传空数组并开启分支。`0x567A44–0x567A62` 按注册标志调用 compiler 虚表 `+0x18`，实际 `0x6563D4`；`0x656415–0x656430` 跳过空行与首字符为 `;` 的行，到首字符 `.` 的行立即停止且不消费该命令。每个其余行作为一个 list 元素，不再按空格拆成多个元素。此处没有要求这些行必须是选项文本。

`0x56B6AF–0x56B6C2` 仍按同一个标志调用 compiler `+0x10`，实际 `0x66B994`，建立 type 4 块，并发出 opcode `0x18` 的待回填跳转。因此零 rows 的 `choice(args,list)` 仍是合法分支入口。

执行分为两条路径：

- 带 list：`0x573B77 → 0x573F18`，`0x57409A–0x5740A3` 调 `sq_call`，参数数 3（this、args、list），请求返回值。`0x57412C` 读 numeric 结果；失败在 `0x57413D` 置 0。`0x573B9B` 把结果交给执行上下文 `+0x0C`，实现 `0x633E00` 写入结果寄存器 0。
- 不带 list：`0x573BB6 → 0x574168`，`0x57424E–0x574257` 调 `sq_call`，参数数 2，**不请求返回值**。原函数可以 return integer/bool/string/null，这条路径都忽略它，而不是错误或隐式打开分支。

numeric 转换函数 `0x69CD14` 以类型位 `0x04000000` 判断；integer 原值，float 类型 `0x05000004` 经 `0x6F4248` 转整数。bool、string、null 不是 numeric，分支结果按 0。当前宿主在分支完成时先按原规则转 integer，再交 Machine；普通命令完成只交 `None`。不把字串数字或 bool 强转成选择序号。

本次实现沿用已有普通参数 token 子集；没有据这份证据扩张转义、插值或宏参数语法。

## case 编译与执行

`0x65C367–0x65C380` 为 `.case` 查找最近 type 4 块。块查找 `0x659740` 从栈尾向前按类型位匹配（核心 `0x6597A3–0x6597D7`），因此嵌套的带 list 命令与 `.switch` 都建立自己的分支上下文。

`0x65A388` 在前一个分支体尾部发出跳到块末端的待回填跳转，并把前一失败路径指向当前 case 的判断。每个 `.case` 后的空白分隔 token 是一个候选常量（分词 `0x65CC70 → 0x67C3F0`，分隔符 `0x772DC3 = " \\t\\n\\r"`）：

- 数值或负数走 `0x65D306 → 0x62588C`；其他名字查常量表，失败的原错误为“不是常量”（`0x65D31B–0x65D358`）。不能用可变槽的当前值替代常量。
- 每个候选发 opcode `0x14`（`0x65D360–0x65D395`），比较结果寄存器与常量，相等则越过剩余测试进入同一个分支体。执行器 `0x637D28–0x637D4A` 验证是相等条件跳转。
- 所有测试失败后发 opcode `0x18` 到下一个 case/块末尾（`0x65D43F–0x65D49C`）。`0x63782F → 0x633FE4` 是无条件设置 PC。
- **空 `.case` 是默认分支**：token 数为 0 时 `0x65CD38` 直接跳过上述所有测试和失败跳转，落入分支体。
- **第一个匹配分支生效**，重复值不报错，也不落入后续 case。默认分支若在前面，会遮住后面的 case。
- 没有匹配并且没有默认分支时，直接到 `.end` 之后，不因结果超出某个 rows 数组而报错。UI selection 的选项有效性检查是宿主输入边界，不能推广成原通用 com 的返回值限制。

Machine 本次支持正常嵌套的分支/loop、case 的十进制整数和已声明常量、多个候选、默认分支、无匹配及 goto 分支作用域。没有据此声明已实现原编译器所有常量表达式或跨未闭合其他块的异常结构。

## 实现接口与验证

`Machine::register_system_command(name: &[u8], takes_rows: bool)` 接收当前 VM 的原注册元数据，覆盖旧 map/select/title 兼容识别。`pending_system_command_takes_rows() -> Option<bool>` 供宿主区分应传 `Some(rows)`（包括空数组）还是不传 list，以及完成时是否回传整数。原 VM 注册是权威来源；新建、读档恢复和 `.system` 改变注册后重新同步，不把游戏 ID 写进解析器。

Machine 保存的物理行位置与分支结束栈格式不变；读档后重新同步元数据，再继续文本/分支。嵌套命令、loop 和 goto 的块识别共用注册判定。

`cargo test -p nekopack-script --lib`：42 passed / 0 failed。新增四项合成测试覆盖 args/rows、零 rows 分支、嵌套分支存档恢复、非分支返回忽略、显式 arity 覆盖 title 形状、常量列表/负数/默认/重复值顺序/无匹配、可变 case 拒绝及 loop/goto。旧 map 测试的“无匹配必报错”假设按原证据移除，缺失分支结果仍保持 pending 且报错。此为静态证据与桌面合成验证，不是原剧情调用或 Android 设备验收。
