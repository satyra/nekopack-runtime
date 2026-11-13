# 共用鉴赏房音乐宿主接口

本轮只执行挂载数据中的原 `com["room"]` / `ViewRoomMusic`，不复制曲目表、不按作品分派。技术证据为本地只读导出的 `inspection/pc-utf8/TD4/x_rooms.nut`：40–107 行是有效音乐房代码；47 行调用 `PlayMusic(name,true,100)`，65–69 行设置 Spot 的 `is_enable` 并加入集合，97 行按数字 ID 取得 `popup`。TD1–TD3 的同名音乐房主体处于注释中，不能据此替它们启用音乐房。

`RoomSpots` 保留名字检索，同时提供按 ID 数字检索、重复 Add 同对象不重复登记、`is_enable` 参与绘制和命中。`-` 前缀图层作为禁用状态。原脚本继续决定播放、上下曲、停止和曲目说明。

`PlayMusic(name, looped=true, volume=100)`、`VolumeMusic(volume)`、`StopMusic()` 按调用顺序生成 `Outcome.music`：`Play { name, looped, volume }`、`Volume(volume)`、`Stop`。音量范围为整数 0–100；播放名称必须是单一资源名称。Rust 在协议边界验证名称、循环值和音量。平台读取这组命令，播放请求的音频资源来自当前挂载的 `sound/bgm`。后续命令不得越过先前播放请求。

房间关闭和跳转回放时统一追加停止命令，覆盖原脚本的直接取消 `return` 分支。旧 `stop_music` 标记暂保留；消费有序队列的平台应避免再次应用旧标记。

单元测试执行原调用形态的 Squirrel 协程，验证数字索引、重复 Add、禁用命中、播放和音量顺序、取消停止及宿主绑定还原，并检查非法资源路径/音量。它不宣称原机声学一致性。未观看图片、剧情或回想正文，未操作 ADB。

房间宿主临时将 `ID_OK` 映射为 2000000001、`ID_HIDE/ID_NEXT/ID_BACK` 映射为 2000000002/3/4，Rust 输入事件使用同一映射，退出后恢复原值。这是移植宿主的虚拟命名空间，不是原引擎 ABI 数值。原音乐房以 `1..曲目数` 作为条目 ID，原 fixture 的 `ID_OK=1` 会把 PLAY 错当成首曲；协程测试验证选择第二曲后 PLAY 不换曲，NEXT/BACK/STOP 不进入条目分支。

真实 TD4 的 `image.dat/sound.dat/system.dat/voice.dat` 隔离调用原 `Room::open("music")` 验证通过：选择第二曲、PLAY、NEXT、BACK、STOP、取消及音频资源 `OggS` 头。该验证不依赖添加或修改原标题菜单。导出 `title.txt` 第 44–46 行存在 `.case 8` / `.room music` 分支，但这不单独证明标题可见按钮。额外原 cg/scene/actor 输入回归也已通过：发现 scene 的 `f["INFO0"]` 空图像检索在协程中依赖构造器明确从根表取得，`RoomImageSet._get` 改为显式分支并调用 `::RoomSprite()`，沿用空 Sprite 语义，未增加作品条件或补图。
