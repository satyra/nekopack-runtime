# 共用 ASF / WMV 播放链

本轮同包设备结果见 [验证记录](../research/sorakoi-wmv-playback-verification.json)。APK SHA-256 为 `ae898b3adc8ad9792627d14758a97eab9920b38d59e54d3884509a98585dfd61`。

## 接入边界

`nekopack-video::Decoder` 按媒体魔数选择 MPEG Program Stream 或 ASF，不按游戏名、文件名或汉化版本分支。MPEG 保留 pl_mpeg；ASF 使用固定 FFmpeg 8.0.1 的最小共享库构建，包含 WMV1/2/3、VC1、WMA1/2/Pro/Lossless。资源仍通过同一 seekable reader/SAF 文件描述符读取，不执行游戏 EXE/DLL，也不预转码媒体。

ASF 解复用使用总计不超过 8 MiB / 4096 包的压缩包队列，提前解出与视频同步的音频，避免文件中视频包领先数秒时饿死音频。队列不积累解码后的 RGBA。

两条路径输出统一的 RGBA、stereo S16 PCM、视频 PTS 和真实 EOF。ASF 的 EOF 要求解复用、两个解码器及重采样器全部排空；不能用容器声明时长乘帧率推算完整性。首个解码音频帧的时间戳用于补齐开头静音或裁掉负时间预滚，使音频和视频共享时间轴。

## Android 宿主

解码和 AudioTrack 写入由唯一 decoder worker 执行；独立 renderer 使用最多三帧的队列以 AudioTrack 实际播放头为主时钟按 PTS 绘制。队列已满时保留未来帧、继续泵送音频，只淘汰已过期的待显示帧。分别记录 decoded、presented、dropped，正常 EOF 要求计数一致，并等待队列、音频播放头和最后媒体时间全部结束。

暂停、失焦和 Surface 销毁冻结活动时钟。绘制不持有音频/生命周期 gate；关闭时唤醒等待并结束渲染线程后释放资源。

默认不写 RGBA、PCM 或视频截图。诊断模式只保存元数据、解码/绘制耗时、指定帧摘要、PCM 摘要及非零样本/峰值计数。`verify-movie-metadata.py` 验证独立播放器；`verify-sorakoi-movie-chain.py` 从真实运行时存档验证原脚本电影请求及正常返回。两种证据不可混称。

## 构建与资源计数

使用 `crates/nekopack-video/tools/build_ffmpeg.py` 构建依赖，Android 构建参数 `--ffmpeg-prefix` 默认 `.local/ffmpeg/android`。APK 打包五个共享库，并逐个检查 AArch64 与 16 KiB ELF LOAD 对齐。ARM NEON/汇编优化开启；GPL/nonfree/version3 组件关闭。对应源码固定 SHA256、构建参数和许可证见该 crate README、构建清单和 APK 的 `assets/licenses`。发布二进制时还需附带匹配的 FFmpeg 源码与构建脚本。

MPEG 的内存 arena 计数保持原含义。ASF 的 `peak_bytes` 仅统计桥接层缓冲区，不包括 FFmpeg 内部堆；`allocation_scope` 明确标识这一点，不能用 `decoder_live_bytes == 0` 声称已经证明 FFmpeg 无泄漏。

## 同轮修复的存读档音效

共用运行时原先把存读档完成音效固定为 `se_STAR`。六作原脚本核对发现，空恋对应函数使用 `sound("OK")`，旧五作使用 `sound("STAR")`。现在分别从当前挂载的 `saveload.nut` 的 QuickSave、QuickLoad、NormalSave、NormalLoad 读取对应字面事件，由宿主传实际操作类型；不要求四个事件相同，也不按游戏 ID 分支。


剧情推进发现的缺失语音处理及原引擎分支依据见 [Sorakoi_Missing_Voice_Semantics.md](Sorakoi_Missing_Voice_Semantics.md)：查无语音资源时无声继续，已存在资源的读取/解码错误仍暴露。

## 存档页与检查点验证

实际读档链暴露了旧存档页对十槽布局及 `= CLOSE` 命名的固定假设。共用存档页现在从 `saveload.nut` 和 MNG 资源提取槽位数、分页、模式绑定、Glyph 标签坐标/字号/颜色和缩略图锚点，并规范化状态标记后的空白。宿主使用每页返回的 `slot_ids` 刷新和命中，不再自行计算 `page * 10`。空恋为八槽、十页；旧五作保持原十槽页。

从标题进入读档时停用标题输入及动画调度；失败路径保留首个错误，避免后续回调用次生的“no title session”覆盖根因。

NKSV 仍验证挂载布局身份。桌面夹具必须按设备的实际挂载顺序创建，不能用排序不同的挂载导出后直接套到 Android，也不能改存档头绕过校验。诊断模式的 `mount-inputs.json` 只记录文件名顺序；`movie_checkpoint` 的 `--mount-order` 接受该清单。电影前和恢复进度的原/汉夹具都分别生成、验证，不混用版本存档。

## 音频验证边界

本轮验证实际 PCM 输出、非零样本、峰值、总样本数、AudioTrack 播放头排空及时间轴。汉化 WMV 的独立播放与剧情播放 PCM SHA 相同；原版 WMA2 的两次 SHA 不同，但总样本数、非零样本数和峰值一致。尚未取得这两次 Android PCM 的逐样本差异，不能宣称原版音频逐样本一致，也不能仅凭 SHA 不同断定丢音或失真。

源码检查确认 WMA 噪声种子固定、SWR 默认不加抖动。独立桌面 FFmpeg 重复解码一致；改变 CPU 优化路径可产生 SHA 差异，抽样仅有少量 1 LSB 误差。这只证明数值路径差异是一种可能，不能作为 Android 两次差异的已确认根因。没有因此盲目加入 BITEXACT 或修改解码算法。后续如需逐样本音质验收，应按需收集同一短区间的两次 PCM，比较最大误差、RMS 和时间偏移；不得恢复常驻媒体转储。
