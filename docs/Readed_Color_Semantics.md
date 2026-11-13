# 原消息颜色表达式与已读状态

2026-11-13。仅读取原系统代码和合成文本；未查看图像、未操作 ADB。

原 sorakoi `message.nut:365–368` 在局部变量中执行 `Config.ReadedColor && is_readed`，由其选择 `NORMAL_TEXT_COLOR` / `READED_TEXT_COLOR` 与对应 BORDER 常量。常量由原 config.nut:5–8 声明，宿主不另造同名规则，也不向不使用这些变量的旧脚本添加该分支。

`message_adapter::layout_adapter` 保留并执行原 readed_color、text_color、border_color 局部声明，取得原 Glyph.Color、姓名 Glyph.Print 的边框参数，以及正文 Glyph.Build2 的边框参数。声明所需的原常量或配置不存在时，继续报告原表达式错误，不猜默认值。原脚本直接写白色或黑边时，仍使用原字面量。

Story 在布局计算前查询当前消息的实际 read_history，并把该页进入时的已读状态传给原局部表达式。设置菜单关闭后重新计算当前页布局，用原页的已读状态和当前 Config，颜色或布局变化时重建栅格；不会因为本页显示期间写入阅读历史就把未读页立即染成已读色。

Glyph recorder 返回姓名/正文各自的 RGB 与可选边框 RGB；None 表示原脚本此次没有请求边框。栅格器在完成字形覆盖率和粗体计算后设置 RGB，不改变 alpha；姓名和正文分别采用捕获结果。渲染 metadata 增加 `name_color`、`name_border`、`text_color`、`text_border`，颜色为 0xRRGGBB 对应整数，无边框为 null。

验证包括：合成原表达式的未读、已读、关闭选项、关闭边框及缺失原常量；真实六作 message/config 系统代码的布局测试；使用 C:/Windows/Fonts/msyh.ttc 对纯合成 AB 字形检查实际 RGBA 值、满覆盖像素和 alpha 不变，以及原布局参数进入栅格器。全程只做数值断言，不生成或查看预览图。

原 Config.Climax 默认为整数 1（sorakoi config.nut:54），绑定 CheckButton（:356）；原 gui.nut:208–223 的控件更新在 0/1 间切换、Get 返回整数。NKCF12 将 Climax 作为同一可选命名标量保存，范围严格为 0/1。旧 NKCF12 未包含此字段时保留当前原默认值；测试覆盖保存关闭、全新 VM 恢复、旧缺字段及非法值 2 在配置改变前拒绝。
