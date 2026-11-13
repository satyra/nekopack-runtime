# 变量 bank 语义

本实现依据原 sorakoi.exe 的静态反汇编；未执行游戏、读取图像或剧情正文。

原生地址解析 `0x47AED0` 区分 G/L 和可选 F 后缀，`0x47E87A–0x47E89E` 计算 `physical_index = (2*local_bank + flag_suffix)*1000 + declared_index`。G、GF、L、LF 分别对应原连续槽位 0..999、1000..1999、2000..2999、3000..3999。

`0x47E854–0x47E86D` 检查首索引加声明名称数不得超过 1000。因此每个 bank 的可声明索引为 0..999，多名称 `.var` 不得跨 bank。`.var` 与系统 `def(address, alias)` 使用同一个地址解析函数。

宿主保留已有存档内部标记：G=`G`、GF=`F`、L=`L`，新增 LF=`f`。这些字符是存档标签，源脚本不接受独立 `F` 或 `f` 地址。每个槽仍为 i32；F 后缀表示 flag bank，现有证据不支持将赋值强制限制为布尔值。

GF/LF 的名称在宿主表达式空间中使用 `@alias`。原生 `0x47E8A6–0x47E8D9` 对全局/局部 flag 名称分别选择 +0xB8/+0xE8 容器，与普通整数名称容器 +0x88 分离。宿主以 @ 前缀区分 flag 与整数名称，保留现有 GF 名称约定。

原 `.reset` 只清连续槽位 2000..3999，即 L/LF；宿主在效果确认后清这两个 bank 的值，保留 G/GF、声明名称、脚本位置和调用/分支栈。LF 不加入全局解锁持久化集合。机器 checkpoint 保存/恢复四个 bank 与别名；原 G/L/F checkpoint 标签保持兼容，恢复拒绝未知 bank 或越界索引。

证据文件：`.local/sorakoi-import/analysis/native-reset-evidence.md`、`native-var-bank-parser-start.txt`、`native-var-bank-parser.txt`。关键测试位于 `crates/nekopack-script/src/runtime.rs`：四 bank 相同索引独立及 checkpoint 往返、边界和跨 bank 声明拒绝、确认前后局部 reset。
