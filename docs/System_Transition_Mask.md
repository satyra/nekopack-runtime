# 原生 Trans mask 契约

2026-11-13。本文补充 [系统过场契约](System_Transition_Semantics.md) 中此前未实现的 mask 分支。分析仅涉及原 EXE 的 PE、注册表、指令与合成像素；未执行商业 EXE、查看原图片或读取剧情/回想正文。地址为当前证据 EXE 的 VA，不参与运行时分派。

## 注册与模式

`sorakoi.exe` SHA-256：`9a30908aedf0f10b41ebf1a22c16f48eee840f1d949d2f7b7d8394de20a86d2f`。

- `0x51EAE5` 注册 `TransSet`，最少参数计数为 -2（包括环境），类型串 `0x75D385` 为 `.nss`。duration 是必需 number；两个可选参数是 string。桥 `0x57C874` 经整数读取器获得 duration，浮点数截断。
- mask 空串或省略：mode 1。mask 非空，第三参数忽略大小写等于 `fade`：mode 3；否则 mode 2。空 mask 不清除旧 mask 表面；第三参数不是全局模式开关。
- `0x4055E0` 在非空 mask 时先加载，再检查 `Config.Effect`。关闭 Effect 只令 duration=0，仍执行 mask 加载。
- `RenderTrans` 注册 `0x51EACA` 的类型串为 `.i`，最少参数计数 -1。省略参数按计时器计算 p；整数参数直接作为 p。p<=0 精确复制 begin，p>=255 精确复制 end，两个端点均绕过 mask 核心。

旧族补充：`Tiny_BoS.exe` 的 TransSet 注册在 `0x401956`，桥 `0x4231AC` → `0x49141C`；`ed.exe` 注册在 `0x51E928`，桥 `0x5739D4` → `0x62C268`。二者同样为 `.nss`/-2 和 mode 1/2/3。它们的 RenderTrans 是只有计时分支的旧桥（`0x423198` / `0x5739C0`），类型串为 `.`，不能把新引擎显式 p 的扩展反向宣称为所有旧引擎的能力。检查到的 `Tiny_BoD.exe`、`Tiny_BfY.exe`、`Tiny_BaW.exe` 没有这两个脚本注册名字。

## mask 是持久四通道表面

`0x405410` 创建空临时图像，调用 `0x469A08(temp,"image/mask",name)`；通用加载器构造 PNG、JPEG 候选。随后调用 `0x69426C(maskSurface,0,0,temp)`，并不将 mask 转为灰度，也不缩放到屏幕尺寸。

`maskSurface=0x7427C4` 在 `0x405385–0x40538A` 首次分配四通道表面；当前证据尺寸为1600×900。`0x68C660` → `0x624FF0` → `0x74184C` → IAT `0x7D0390` 为 `VirtualAlloc(NULL,size,MEM_COMMIT,PAGE_READWRITE)`，初始字节为零。运行器使用实际逻辑画布尺寸，不硬编码本证据文件的尺寸。

`0x69426C` → `0x69454C` 的矩形检查 `0x693FC0` 裁剪源/目标范围。目标坐标明确为 (0,0)，因此 PNG 内嵌位置不作为 mask 的屏幕原点。实际逐行函数 `0x694774`：

```text
source alpha == 255: 精确复制四字节
source alpha < 255: 每通道 out = old + floor((source-old)*sourceAlpha/256)
```

这里第四字节也参加运算；不是标准 source-over 的 alpha 合成。较小图片只覆盖左上交集，较大图片被裁剪，未覆盖区域保留旧值。重复加载半透明 mask 会继续合成，不能仅保存“最后文件名”代表表面。

资源缺失：`0x46AF1E` 的失败分支不调用成功物化函数 `0x68D364`，临时图像保持空。后续 `0x693FF2` 检查源通道数不是4即返回，不写目标。于是首次缺失使用零表面，之后缺失保留已有表面。运行器只将真正不存在作为此分支；已存在但无法解码的文件直接报错。

WipeCommand 的 execute `0x422984` 内，`0x4262FD` 调 `0x42A43C`，后者 `0x42A48D` 调相同的 `0x405410`。因此 `.wipe` 与系统 Trans 共享一份 mask，不能维护两套互不相干的“上次 mask”。

## 像素内核

对于 1<=p<=254，以下规则逐存储字节独立执行。彩色 mask 的 R/G/B 阈值分别作用于对应通道，不能统一取 mask[0]。

```text
mode 2: out[i] = mask[i] + p >= 255 ? end[i] : begin[i]
mode 3: a = clamp(mask[i] + 2*p - 255, 0, 255)
        out[i] = begin[i] + floor((end[i]-begin[i])*a/256)
```

硬边 `0x401384` 的 `paddusb`/`pcmpeqb` 证明饱和加法后与255比较。柔边 `0x4013FC` 的 `psubusw`/`pcmpgtw` 实现范围限制，随后 `psubsw`/`pmullw`/`psrlw 8`/`paddb` 实现有符号差值的模运算；Rust `div_euclid(256)` 保持负差值向下取整。它与无 mask 模式 `(begin*(255-p)+end*p)>>8` 是不同公式。

两个核心分别为120、232字节，SHA-256 为：

- hard：`3d57419ceab91ae1e0a7ee47afdeed9f20c86ea49c449ec72bbaef41006ac86f`
- fade：`f2c52fa9cab387e7523972e249ae96e5597662502dbca54c6349ea183c5eea77`

与 `research/original-effects-batch10-evidence.json` 的四款原 EXE 已有记录完全相同。运行器将 native 屏幕最终呈现为不透明 RGB；mask 表面本身保留真实第四通道，不在加载时强行写255。

## 实现、生命周期和范围

`system_command/mask.rs` 管理持久 `MaskState` 和共用核心。`TransSet` 输出 M 加载记录，即使没有 Screen.Update 也会消费；每次加载获得不可变 Rc 快照。Y 绘制记录引用加载序号，后续 TransSet 不改变已捕获的中间画面。ScriptView 可在 X/Y 嵌套快照中继续解析和绘制。Story 在命令间移交同一 MaskState，`.wipe` 同样使用它。

原 mask 地址的直接代码引用只有构造、析构、首次分配、加载和读取五类位置，未发现 Save/Load 序列化引用。此次不新增 NKSV mask PNG 字段：warm load 保留 live mask，新的 Story 从零开始。既有 NKSV 的 `last_wipe_mask` 文件名字段保持格式兼容；仅在没有 live 表面的旧 Wipe 恢复路径继续按该文件名重建。这是既有运行器兼容行为，不能描述成原 EXE 持久保存完整 mask。半透明累积表面没有新增的冷恢复保证。

当前支持 PNG mask，包含彩色、透明、尺寸裁剪与缺失沿用；通用原加载器的 JPEG 候选已识别，但运行器 JPEG 解码尚未实现，存在 JPEG 候选时明确失败。负 duration 仍是已声明的不支持输入。没有声称覆盖所有 EXE 的 ABI，也没有按游戏名设置例外。

复核命令：`python research/verify_original_trans_mask.py --exe <原EXE> --output <报告.json>`，断言56条关键指令和两个内核哈希。当前报告 `.local/sorakoi-import/analysis/trans-mask-proof.json`。单元验证只使用合成像素，覆盖硬/柔边、各通道不同阈值、负差值取整、端点、透明合成、旧快照、缺失与损坏边界、无帧加载和跨 Trans/Wipe 共享。
