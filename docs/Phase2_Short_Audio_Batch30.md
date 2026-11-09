# Batch 30：短音频输出帧进度

## 问题与选择

Batch 29 在 BLESS 第 542 页的 `SE_024` 上缺少可靠播放证据：MediaPlayer 的位置始终为零，标称 372 ms 的片段完成回调只比 playing 日志晚 368 ms。旧报告仍保留失败状态，不修改历史验收结论。

公开 AOSP `MediaPlayer.getTimestamp()` 实现通过 `getCurrentPosition()` 构造时间戳，因此不能把该接口当成与位置查询独立的输出证据。也不能把短片段的标称时长与宿主墙钟差值当作精确设备进度。本批保留原严格音频判据，没有增加时间容差。

Android 宿主对声明时长不超过 1 秒的非循环语音和 SE 使用静态 PCM 后端；其余音频继续使用 MediaPlayer。选择依据是资源类型、循环标记和容器时长，不包含游戏名、脚本行号或特定素材名称。Rust VM、DAT 解析、脚本调度与 C ABI 均不变。

## 实现边界

- `ShortClip.read` 在既有故事工作线程使用 Android MediaExtractor/MediaCodec 解码原始 DAT 提取的 Ogg。只接受单音轨、单/双声道、8–192 kHz、PCM16；输出最多 1 MiB，解码循环最多 5 秒。容器时长超过 1 秒时直接选择原流式后端；非法输入、格式或预算失败直接报错，不静默换后端。Extractor 与 codec 在成功、返回和异常路径均释放，准备失败时删除尚未移交的缓存文件。
- `ChannelPlayer` 统一现有声道使用的准备、开始、暂停、音量和释放操作。短片段先写入 AudioTrack MODE_STATIC，再开始播放；以 playback head 达到解码帧数作为完成条件，不按定时器推算播放量。
- 20 ms 回调只查询设备帧计数。暂停和释放使旧轮询代次失效，恢复保留帧位置；完成回调只产生一次。准备完成前取消也不会触发准备回调。
- 既有音量、焦点、后台切换、movie 前清理、同声道替换与 KeepVoice 使用同一个声道适配对象。循环音频不进入静态 PCM 路径。
- `pcm_frames`、`pcm_rate`、`pcm_head` 事件提供可核验的输出进度。现有 `progress` 为播放头换算的毫秒值；不会写入根据墙钟虚构的位置。

1 MiB 是单片段解码 PCM 上限，不是 Android 整体进程或 codec 内部分配上限。静态缓冲写入仍有设备侧副本。此后端没有解决所有音频格式、混音、真实听感或原版声学一致性问题，也不是跨平台 Rust 音频解码器。

## 验证

当前四组应用专项通过：两组 KeepVoice 文字专项、61 页/5 次后台切换、13 页音频状态、10 次音量设置更新；各组正常关闭后 VM 计数归零。Python 既有 15 项验证器测试通过，NDK ARM64 APK 构建通过。Rust 源码未改动，本批未额外重跑整个 Rust workspace。

首次连续运行 `story_94ccde2a8f7c413da0637a7b4cfbce2e` 在第 3 页进入后台，系统前台切换到另一游戏包 `com.linekong.cjad.lk`。用户确认正在使用模拟器，故停止验证进程；运行器冻结了文字和音频。此中断运行不计作通过。

用户允许继续后，从标题重新执行的 `story_c8b7d7ce4e8a460294c9d1a504f6bc64` 完成了全部 793 个参考页，最终 APK SHA-256 为 `78f93c21202e5e1066351583f21936bdd51838adba28c69bc27581165242e75c`，与前述四组应用专项一致。仅使用 BLESS 与现有 x86_64/libnb ARM64 翻译模拟器，无真机验证。

- 793 页元数据、原始资源来源及 PNG 哈希通过，493 次音频请求通过原严格事件校验，全部观测到正播放位置（采样或完成回调），无仅靠替换或终止窗口豁免的请求。完整连续运行正常关闭后 VM live bytes=0，音频缓存已清理。
- 16 个短 PCM 片段均观测到末帧完成。此前 token 96 短语音完整播放 14,564 帧；token 352 的 `SE_024` 完整播放 16,384 帧，播放期间 BGM 继续运行。
- 11 个角色动作场景、59 张实际采集中间帧通过独立原始 DAT/Pillow 合成核验。该核验不等同于原版窗口像素一致性。
- 独立文字事件重放覆盖全部 793 页：默认速度公式、每页唯一推进、旧页回调顺序及最终快照通过；观察到 586 次换页保留仍在播放的语音。
- `research/verify_audio_batch30.py` 核对全部相关报告属于同一 APK，逐个验证 PCM 帧计数单调、有界、完成事件位于末帧之后，并输出 `continuous_regression_passed=true`。旧 batch 29 失败报告不改写。

报告保存于 `research/audio-batch30-{story,dialogue,lifecycle,state,config,short-probe,frames,evidence,summary}.json`，summary 包含依赖报告的 UTF-8/LF 哈希。此验收仅覆盖已建立的 793 页参考范围，不代表整部游戏通关；音频被同声道替换的证据也不能冒充该片段完整播放或实际听感证明。

`research/verify_short_audio_device.py` 独立从原始 DAT 提取 `SE_024`，核对既有资源哈希，将实际 `ShortAudio.kt` 和设备探针编译为 dex，以 `app_process` 运行。该诊断是 Android 框架测试，不含 Rust VM，也不冒充 ARM64 应用测试。覆盖完整播放、暂停/恢复、播放中取消、准备回调前取消。正常播放与恢复后均须到达 16,384 帧，取消不得产生完成回调。

初次临时诊断虽已看到完整帧计数，但尝试退出 Android 主 Looper 导致探针异常；它不作为通过证据。正式探针在释放播放器后正常结束进程，完整运行记录由上述脚本重新生成。

正式应用验证仍通过 SAF 由 Rust 直接读取原始 DAT，使用默认 TextSpeed=50、KeepVoice=1。动作帧、音频事件与逐页文字状态由独立脚本复核，不用独立诊断的预提取素材替代应用运行路径。

## 参考

- [AOSP MediaPlayer 实现](https://android.googlesource.com/platform/frameworks/base/+/master/media/java/android/media/MediaPlayer.java)
- [AudioTrack 播放帧计数与静态缓冲](https://developer.android.com/reference/android/media/AudioTrack#getPlaybackHeadPosition())
- [MediaFormat PCM 编码约定](https://developer.android.com/reference/android/media/MediaFormat#KEY_PCM_ENCODING)
