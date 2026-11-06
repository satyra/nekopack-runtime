# 素材坐标与画布合成增量

这是首次画布增量的历史验收记录。后续已定位 alpha 丢弃、控件素材状态和 JPEG 截图问题，最新实现与验证见 [显示差异修正](Phase1_Display_Correction.md)。

## 实现边界

`Mng::sprites()` 提供素材序号、FRAM 原始名称字节、有效 DEFI 坐标。保留同名素材，不将名称直接塞入会覆盖重复项的字典。当前支持本机样本出现的可见、abstract、object 0、12 字节 DEFI，以及 mode 2、无参数变更的 FRAM；坐标缺省为零，后续图片继承最后一次完整 DEFI。其他定义形式与控制参数明确报错。这个接口提取素材，不执行 TERM 循环、FRAM 时钟或动画播放。

`RgbaImage::transparent()` 按预算创建画布，`draw()` 使用 straight RGBA source-over、带符号坐标和画布裁切。中间值使用整数，最终通道四舍五入；不转换 gamma/ICC。分配预算按每张图像计算，不是整个进程的总内存限制。

CLI：

```text
inspect-sprites FILE ENTRY
compose-mng FILE ENTRY:IMAGE [ENTRY:IMAGE ...]
```

前者输出序号、x、y、十六进制名称；后者按照参数顺序合成一个 DAT 内选定图层，输出标准 PNG。画布尺寸取第一个 MNG 的 MHDR，其他图层必须相同。允许重复绘制同一素材。商业游戏名称和界面逻辑没有写入 Rust 库。

## 四部曲验证

`research/verify_canvas.py` 检查固定样本封包与解密 MNG 的 SHA-256，再独立遍历原始块，核对全部 **84 MNG / 3,072 素材**的名称和坐标。每个 MNG 选第一、最后两张素材进行合成，共 **84 组**，与 Pillow 的裁切及 alpha_composite 对照，尺寸全部一致、最大通道差不超过 **1/255**。Pillow 使用中间定点舍入，Rust 对有理数结果做一次舍入，因此没有宣称全部逐字节一致。该测试验证选定图层，不代表所有图层组合或真实动画。

```powershell
python research/verify_canvas.py --games 'D:\codex\电脑移植安卓\游戏' --cli target/release/nekopack-cli.exe --output research/canvas-verification.json
```

合成器另有半透明颜色、alpha 累积、负坐标、极端坐标、预算和元数据状态测试。

本次 `cargo test --workspace` 共 34 项通过，Python 原有 6 项测试通过；fmt、clippy（warnings as errors）、release build 通过。

## 原版 BLESS 设置页对照：尚未通过像素一致验收

通过 computer-use 启动样本内 `Tiny_BoD.exe`，进入 Config 第一页。窗口截图 1282×747，推定客户端裁切为 `(1,26)-(1281,746)`。根据该样本 `system/config.nut` 的 ViewConfigPage（111–139 行）、RadioButton（`system/gui.nut` 114–130 行）及可见的选项状态选择素材；这里只重建一个静态页面，没有执行脚本。

资源来自 `system.dat` 条目 24（config）与 19（config1）。图层列表、封包/截图/CLI 哈希和差分统计见 `research/bless-config-comparison.json`；复现入口为 `research/compare_bless_config.py`。样本专用选择规则仅存在于研究脚本。

可见布局和选项状态能够对应，但原版与合成图的色彩、图案与文字边缘明显不同。在客户端 `(100,200)-(1170,650)` 区域比较，以排除右上角工具提示覆盖，481,500 像素中 480,695 个不同，最大通道差 132，平均绝对通道差约 25.509。未进行缩放、色彩拟合或以修改素材掩盖差异。截图与合成图片仅保留在忽略提交的 `.local/`，不上传游戏图像。

下一步应检查原版加载的实际资源版本、PNG 色彩信息、渲染采样/显示变换及 Sprite 绘制语义，逐项隔离差异。目前不能声称与原版显示一致，更不能声称游戏可运行。
