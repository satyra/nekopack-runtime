# 原消息对象与绘制钩子

本轮让消息图像消费原 VM 中当前的 `MsgBox`，而不是根据命令名或游戏名选择固定 `msgbox.mng`。`message_adapter::message_box` 捕获对象实际 Sprite 资源句柄、位置、大小以及经典消息框的文字局部锚点；`DialogueRenderer::from_message_box/set_message_box` 使用这些对象生成面板。资源从当前挂载中解析，图片只在内存中解码。

经典消息框保留原 `name_x/name_y/name_w/text_x/text_y/text_w`。共享底板消息框读取实际 `base/frame/name/text`，frame 在姓名前还是后、是否仅有名时绘制，都来自原消息处理器的有效绘制语句。共享底板的 `Screen.Draw(..., opacity)` 使用透明度；经典 `Clear/Blt` 继续采用已验证的原 Blt 像素规则。原坐标不重排，超出画布的面板按 Screen.Draw 语义裁切。ED 原 BASE/FRAME 超出 1280×720 两像素，是这条通用裁切路径的真实样本。

`layer_adapter` 在排除 `//` 和 `/* */` 后，从原 `com[""]` 中保留支持的消息前钩子、装饰层绘制语句。页面开始执行原 `UpdateClimax()` 一次，恢复参数通过 `InRestore()` 传给原函数；后续帧只执行原绘制语句，不再次推进计数。原 `Config.Climax` 条件由脚本执行。Sprite 合成保留原位置和逐层偏移，生成透明覆盖层，最后叠在消息文字上；覆盖层不为空时输出几何扩展到场景画布。

TD1–TD4 和 ED 的 `DrawDate` 第一条有效语句均是 `return;`，即使原消息循环调用它，也应保持无绘制。Sorakoi 的该调用本身已注释，日期采用独立 `com["date"]` 协程。适配器不启用这些不可达或被注释的日期动画。五部旧作的 `com["box"]` 赋值也处于块注释中；覆盖清单已修正，不把它们当有效注册。其他实际模块若改变 MsgBox，对象消费者可在下一消息页读取新对象。

新的共用 API：

- `message_box(vm)`：当前消息框对象快照。
- `message_layers(vm, store, variables, begin, rest, elapsed_delta)`：执行原钩子，返回 Sprite 覆盖层计划及原 `DrawDate` 是否要求继续绘制。时间输入为增量毫秒。
- `raster_message_layers(store, snapshot, size)`：只在内存中生成透明 RGBA。
- `DialogueRenderer::set_overlay`：保留已准备字形，更新覆盖层与缓存；`frame_origin` 返回当前输出矩形的场景坐标。

已验证支持普通资源 Sprite/合成 Sprite。未验证的填充复合 Sprite、带场景快照或过场计划的 MsgBox Sprite 明确报错，不将其误当资源名或静默丢弃。

验收：消息相关单元测试 16 项通过、3 项原资源测试默认跳过；其中新测试覆盖对象资源/几何修改、共享 frame 策略、底板透明度、注释排除、前钩子仅一次、恢复不推进，以及旧 DrawDate 早返回。`six_original_layouts_preserve_their_own_parameters` 独立通过。另以 BLACK、BLESS、BIRTH、BRAVE、ED、Sorakoi 六作原 DAT 启动到标题边界，执行新对象快照、原 Glyph 布局和原消息钩子，渲染合成 probe 消息到内存：`six_original_msgbox_objects_drive_render_panels` 通过（六作循环，2.86 秒）。未查看、导出游戏图或正文，未操作 ADB；这些桌面结果不冒充 Android 同包呈现验收。
