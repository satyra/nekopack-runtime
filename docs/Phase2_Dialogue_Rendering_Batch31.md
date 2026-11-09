# Batch 31：Rust 普通对白覆盖层

本批将 Android 独立 TextView 白话显示替换为 Rust 绘制的游戏坐标覆盖层。BLESS 原始 DAT 提供消息框素材与锚点，宿主提供字体字节；逐字进度、点击补全文字和下一页许可仍由 Rust 决定。本批不包含标题菜单、存档菜单、回想或完整原版 GUI。

## 实现边界

- `text_render.rs` 使用 `ab_glyph 0.2.32`，绘制普通 Unicode 正文、姓名、白字与一像素黑色描边，按字形 advance 换行。采用 `message.nut` 的 22px 字号、8px 行间隔；姓名位置采用原脚本 CP932 字节长度乘 11 的居中公式。
- 消息框直接来自 `system/msgbox.mng`：TEXT 为 `(100,605,1080,107)`，NAME 为 `(15,573,344,37)`，正文锚点为 `(136,638,1005,82)`，姓名锚点为 `(81,578,217,33)`。画布尺寸取自 MNG；以上是本次 BLESS 资源的验证值，不是 Android 视图硬编码。
- 匿名对白不绘制姓名框。字形按素材边界裁切，元数据记录行数与正文是否溢出。字体输入限制 24 MiB，单页最多 4096 个 Unicode scalar，字形缓存限制 16 MiB。
- Android 通过借用 FD 提供字体，Rust 复制 FD 所有权并读取；调用仍在单个故事 worker 上执行。核心不读取 Android 字体路径、不调用 Android 字形 API。
- Android `DialogueView` 只将裁剪后的覆盖层映射到与场景一致的 fit-center 视口。每帧传输消息框联合区域，BLESS 为 `1165×139`；没有改为每帧传输整个 `1280×720` 场景。
- `DialoguePresentation` 继续使用活动时间轴。过期或取消的异步位图不显示；关闭后停止请求帧。辅助功能文本保留在 content description 中，实际画面由位图显示。

## 原版透明度证据

`config.nut` 默认 `BoxOpacity = OPACITY_MAX * 3/4`，实际 VM 返回 191，Border 为 1、Bold 为 0。`message.nut` 在透明度小于 255 时执行 `Clear(); Blt(...)`，不是简单修改图层 Alpha。

已追踪 BLESS 原生路径：Squirrel Blt 包装 `0x40d550` → `0x4bd8ac` → `0x4bdaf4` → 像素循环 `0x4be0b0`。MMX 双像素循环先计算 `(source_alpha * opacity) >> 8`，再对包含 Alpha 的四个通道插值。清空目标上的结果是 `source_channel * factor >> 8`。本批仅在消息框局部复现这条路径，未修改普通场景的 straight-alpha 合成器。255 分支按原脚本直接使用原素材。

原版静止第一页截图中，排除文字的 TEXT 底部区域（局部 y≥70 且原素材 Alpha>0），RGB 平均绝对误差由仅缩放 Alpha 的 **30.85** 降到 **2.45**，95 分位误差由 **51** 降到 **6**。这是截图误差对照，不是逐像素原版一致性证明：原版捕获为 JPEG；当前 Windows ANSI 环境使日文文本乱码，不能用于正确日文字形对照。未将截图或游戏资源提交到仓库。

原生循环尾部标量路径取完整 BGRA 乘法高位，可能比双像素循环多一级舍入；后续原版 Draw 的定点舍入也未逐位复制。本实现采用双像素循环公式，保留这一限制，未使用经验透明度替代原版配置。

## 字体与许可

当前宿主显式提供模拟器 `/system/fonts/NotoSansCJK-Regular.ttc` 的第 0 个 face。`dialogue-font.json` 同时记录原版请求的 `ＭＳ ゴシック`、宿主替代标识和实际字体来源。**这不是 MS Gothic，也不宣称复现 GDI 的字形与抗锯齿。** APK 不包含字体或游戏素材。

`ab_glyph`、`ab_glyph_rasterizer`、`owned_ttf_parser`、`ttf-parser` 的 Apache-2.0 许可随已有许可证打包流程进入 APK。Android 字体路径目前只验证了本机 Android 9 模拟器；其他设备的字体供应需要独立验证，文件缺失会明确报错，不静默换字体。

## 验证

设备为 `emulator-5568`，x86_64 Android，通过 **libnb 翻译执行 ARM64**，不是 ARM64 真机结果。证据报告记录 APK SHA-256、字体 SHA-256、原始资源 SHA-256、原版指令位置与运行 ID。

- `research/dialogue-render-batch31-verification.json`：前三页分别覆盖具名、匿名和两行对白。相同 DAT 素材及字体输入下，桌面 Rust 与 Android 输出的空正文/完整正文覆盖层像素完全相同。另用实际 `screencap` 检查界面中的绘制区域，而非只检查导出 PNG 或辅助功能文字。关闭后 VM live bytes 为 0。
- 同一验证器修改本地 MNG 的 DEFI 坐标并重算 CRC，分别触发锚点相减溢出和面板边界相加溢出；桌面对照入口均返回明确错误，退出码 1，而非 panic。
- `research/dialogue-render-batch31-controls.json`：KeepVoice 0/1 两组各五页，检查逐字显示、Home 后文字计时冻结、恢复、第一次点击补全而不推进、第二次点击推进、语音保留/停止，以及文字尚未显示完时关闭并释放 VM。
- `research/dialogue-render-batch31-lifecycle.json`：BLESS 前 61 页，覆盖淡变、视频、对白/BGM、角色动作期间的五次后台/恢复，核对既有页面元数据及场景 PNG，并检查 VM 释放。
- Rust workspace：131 项测试通过；Clippy `--workspace --all-targets -- -D warnings` 通过。研究验证器的 15 项 Python 测试通过。NDK ARM64 构建通过，仍检查 ELF 16 KiB LOAD 对齐及 C++ 共享运行库依赖。

首次生命周期测试在第 32 页读取整张 PNG 时触发 ADB 45 秒超时，未计为通过。应用未写出 error.txt，设备上的该页 SHA-256 与桌面参考一致。验证器随后改为设备端计算 PNG SHA-256，与桌面完整文件哈希比较，避免反复传输大图；最终结果来自重新开始的完整回归，未跳过该页。

运行画面对照：

```powershell
python research/verify_dialogue_rendering.py --adb <adb> --serial emulator-5568 `
  --games <本地游戏目录> --portkit <portkit目录> `
  --reference .local/batch25-reference/0 `
  --desktop target/debug/examples/dialogue_overlay.exe `
  --work .local/dialogue-batch31/verification `
  --output <新报告路径> --original-capture <原版窗口捕获>
```

该验证器依赖已有 SAF 授权，不自行授予权限；原版截图参数可省略。`dialogue_overlay` 示例是宿主侧对照入口，字体、MNG、姓名与正文均由本地文件输入。

## 后续边界

普通对白接入完成，不代表完整 Glyph 兼容。尚未实现日文禁则、ruby、富文本、可变字号/粗体、字形渐显曲线与原版 GDI 栅格化；超出两行的文本可能触发素材裁切标志。原版字体匹配、边缘精确舍入和长对白布局需继续单独对照。已有底部原生按钮仍是运行器验证宿主，不是原版游戏按钮。

下一步应先补长对白与明确出现的文字控制语法，再接入原版输入/菜单控件；继续复用同一套 Rust 状态和排版，不为各平台建立独立文本解释器。本轮不重写渲染后端，不扩展到其他三部或桌面宿主。
