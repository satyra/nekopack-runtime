# 显示色彩与边缘差异修正

## 已确认的原因

1. 先前对照脚本对透明合成图调用 `convert('RGB')`，这会丢弃 alpha，**不会**将图像合成到黑色背景。BLESS 设置页背景大面积 alpha 为 187，例如原始 RGBA `(183,64,54,187)` 应在黑底显示为 `(134,47,40)`，旧脚本却拿 `(183,64,54)` 比较。错误发生在显示/对照边界，不是 PNG 像素解码。
2. 原脚本把单选控件的选中态映射到了 `+ ` 素材（悬停外观）。观察到的选中态对应 `@ ` 素材，正常态为 `= `。修正后高亮颜色和字形边缘与原版明显接近。这个映射是四部曲研究配方，不作为所有 NekoPack 游戏的通用引擎规则。
3. `.local/bless-config-original.png` 实际文件格式是 **JPEG**。当前工具返回的 MIME 也是 `image/jpeg`，实测色度抽样为 4:2:0（Pillow sampling 2）。扩展名误导了先前分析；有损截图的色彩边缘不能作为原始 RGBA 的逐字节真值。新采集文件使用 `.jpg`，分析通过文件内容识别格式。

没有修改 PNG 解码器、添加 gamma 曲线或给输出做模糊处理；也没有凭截图拟合一个颜色变换。

## 实现

`RgbaImage::filled(width,height,rgba,budget)` 提供明确的初始画布样本。`transparent()` 保留透明输出语义。CLI 新命令：

```text
compose-mng-on FILE RRGGBB ENTRY:IMAGE [ENTRY:IMAGE ...]
```

它从指定不透明底色开始，按原有 source-over 规则逐层绘制。`compose-mng` 仍输出透明合成图；不能丢弃其 alpha 后当作屏幕图像。黑底是这次静态设置页对照的显式输入，不是已证明的所有 `Sprite.Base()` 调用的通用语义。

`research/config_scene.py` 通过名称哈希查找 config/config1，再按完整素材名解析图层序号，不再写死 BLESS 的封包条目或素材序号。同名歧义、缺失素材直接报错。原版脚本 `config.nut` 111–139 行和 `gui.nut` 114–130 行提供了页面构造/控件状态的静态证据，素材状态外观又与实际页面对照。本次未实现脚本 VM。

## 四部曲回归

全部 **84 MNG / 3,072 素材**名称与坐标继续通过。每个 MNG 分别验证透明、黑色及 `193B79` 底色，共 **252 组**首尾图层组合，与 Pillow 的最大通道差不超过 1；所有不透明输出 alpha 均为 255。

另外四部曲各重建一份 22 图层设置页，覆盖 17 个选项素材，其中 8 个选中态；按名称解析全部成功，与独立 Pillow 合成最大通道差不超过 1。这里是**四份资源配方测试**，不是声称采集了四个原版窗口。

36 项 Rust/CLI 测试、8 项 Python 测试通过；fmt、clippy（`-D warnings`）、release build 通过。新增回归覆盖透明像素、半透明边缘、黑/白/彩色底、错误底色输入、重复名称及选中/悬停素材区分。

## 原版 BLESS 的测量结果与边界

用户恢复原版 Config 第一页后重新采样。在原对照区域，新旧截图解码后逐字节相同，排除了页面状态漂移。窗口仍为 1282×747，客户端裁切 `(1,26)-(1281,746)`；测量客户端 `(100,200)-(1170,650)`，避开工具浮层和标题栏。

- 旧方法（丢弃 alpha、误用悬停态）：平均绝对通道差 **25.5088**，最大 **132**。
- 仅修正画布底色：平均 **5.6721**。
- 底色和选中态都修正：平均 **3.1624**，最大 **65**。
- 额外使用截图自身量化表、抽样方式和窗口块坐标编码一次 JPEG，仅作同编码诊断：平均 **1.4274**，99% 通道误差不超过 **7**，最大 **26**。

直接对照平均误差降低约 **87.6%**。JPEG 诊断没有使用拟合的亮度、gamma、缩放或模糊参数，也没有拿原版像素填充被测区域。截图自身量化表和各阶段指标全部记录在报告中。

**仍不能宣称与原版逐像素一致。** 主要显示错误已修正，但同编码诊断仍有残差，可能涉及原版整数混合/采样和编码器差异，不能仅凭有损截图判定或随意更改 Rust 混合公式。需要原版无损帧或受控渲染实验才能完成这一项验收。研究期间比较过不同舍入假设，但没有把截图拟合结果写入运行器。

报告：`research/canvas-verification.json`、`research/config-scene-verification.json`、`research/bless-config-corrected.json`。旧失败报告 `research/bless-config-comparison.json` 保留作为前测记录；截图及可视化图片仅在 `.local/`，没有提交商业素材。

## 复现

```powershell
python research/verify_canvas.py --games 'D:\codex\电脑移植安卓\游戏' --cli target/release/nekopack-cli.exe --output research/canvas-verification.json
python research/verify_config_scenes.py --games 'D:\codex\电脑移植安卓\游戏' --cli target/release/nekopack-cli.exe --output research/config-scene-verification.json
python research/compare_bless_config.py --archive '你的 BLESS system.dat' --screenshot '原版设置第一页截图.jpg' --cli target/release/nekopack-cli.exe --output research/bless-config-corrected.json
```

对照配方固定为本次观察的第一页选项状态和窗口尺寸。其他状态、缩放或截图尺寸不能直接套用；脚本不自动缩放或修正颜色。
