# BGV 共用运行时

实现依据 `docs/Audio_Command_Semantics.md` 与原 EXE 的静态 ABI；没有读取图像、剧情正文或操作 ADB。

`DatGame` 在挂载 EXE 时调用 `engine_abi::probe_bgv`，保留 `BgvProfile`。多个 EXE 给出互相冲突的契约时明确失败。角色标记、资源名前缀和普通语音抑制子串全部由 EXE 提取，不按游戏 ID 或固定角色名称判断。

`.bgv` 参数交给 profile 的原生顺序解析器。无参命令可在没有 profile 时停止两个 BGV 槽；带参命令缺 profile 则报未知契约。channel=0 停双槽；channel=2/3 选择 bgv1/bgv2，name 为空单槽停止，否则从 `sound/bgv/<resolved-name>.ogg` 读取并循环播放。读取失败及不带 Ogg 标头的资源明确报错。

角色命令音量直接读取 `Config.VoiceMute[tag]` 和 `Config.VoiceVolume[tag]`。角色 key 不存在时对应值为 0，未使用 XX 或其他角色兜底。运行配置须提供两张角色表；有效角色音量必须为 0..100 整数，mute 接受整数/布尔值。分类 `bgv` 音量与角色命令音量分别传给宿主，不能将二者合并后再重复相乘。mute 后命令音量为 0，仍正常加载循环播放。

MegaSkip 的新 BGV 命令生成该槽停止效果，不读取新资源，不保留待恢复 BGV；普通 skip 不抑制 BGV 循环播放。每个效果独立等待宿主 token 确认，确认后才更新持久音频与角色标签状态；全停、单停、reset、movie 按槽清理。BGV 命令不消耗排队的角色动作。

宿主协议：`host_effect=bgv1/bgv2`，`loop=true`，`volume_percent=角色命令音量`；`bgv1_stop/bgv2_stop` 停指定槽，`bgv_stop` 停双槽。`audio_config` 附加 `bgv_voice_tags` 对象，两个值来自 profile 的语音抑制子串，宿主据实际普通语音播放状态进行子串匹配。

NKSV/NKSC 新写版本 6：原音频记录允许 bgv1/bgv2，原版本 5 数据体末尾追加 `(kind, tag)` 集合。继续读取版本 1..5。版本 6 校验 BGV 记录与角色标签一一对应，标签匹配挂载 profile 的槽位。保存实际资源名和角色 tag；恢复前按当前 Config 重算角色命令音量，不保存易过期的计算结果。原 BGM/ENV 命令音量保持 50，不因新增字段变成 100。

另接通剧情执行期间的 `.system`：调用 bootstrap 共用 `load_system_module`，复用当前 VM、变量和资源层。模块成功执行后才确认命令；缺资源、模块错误或 message adapter 契约错误直接暴露。

测试采用 LEFT/RIGHT 与 speaker_a/speaker_b 等合成契约和 `Synthetic` 文本，不使用真实剧情。覆盖双槽/单槽停止、普通/MegaSkip、角色值缺失及静音、缺失/错误音频、保存恢复重算音量、版本 5 读取、无效保存标签拒绝、动态 `.system` 的共享 VM 和错误边界。

## 角色配置冷启动恢复修复

设备元数据 `.local/system-integration/audio-device-initial.json` 记录首次 BGV1 命令音量 73，QSAVE 冷重启后为 0。根因位于 NKCF 字典导入：旧实现要求目标字典已存在相同角色 key，导致原脚本动态新增、原默认配置尚无的 key 被忽略。现在对存在且类型正确的 VoiceMute/VoiceVolume/Dialog 表使用 rawset 恢复已校验记录；角色名来自快照，不使用固定角色名单。旧 NKCF11 数据同样受此修复覆盖。

NKCF12 在原快照外追加命名标量，保存 VolMAS/VolBGV/VolMV、其余分类 Mute 字段和 ReadedColor/ReadedFast；既有五分类音量与 MuteBGM 继续由基础记录保存。旧 NKCF 版本继续读取，未知命名字段、截断记录、重复记录和越界值在改变配置前拒绝。旧快照未曾记录的值不会凭空恢复。

合成冷启动测试从空角色表创建全新 VM，导入动态新增 NewRoleA/NewRoleB 的 73/61 音量与 mute，验证完整现代分类音量及阅读选项恢复、快照往返一致、坏记录不改变配置、原 NKCF11 字典也能恢复新 key。此测试不操作 ADB；设备最终复测由主代理执行。另共用 delay 后续扩展使当前 NKSV/NKSC 写版本为 7，BGV 的版本 6 标签扩展布局保持不变，读取继续兼容旧版本。

后续原 SaveInfo 的系统状态保存使用 NKSV/NKSC8：版本7仍只读取delay扩展，版本8另读system_state，避免改变既有v7布局。BGV读取测试现同时检查当前v8以及去掉新扩展后的v7/v6/v5，包含原音频资源/角色标签验证。
