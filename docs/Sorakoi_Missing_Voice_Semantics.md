# Sorakoi 缺失语音的原引擎语义

2026-11-13，仅检查资源索引、技术系统脚本及原 EXE，不读取正文、图片或回想。

## 可复核输入

- `sorakoi.exe`：4,235,776 bytes，SHA-256 `9a30908aedf0f10b41ebf1a22c16f48eee840f1d949d2f7b7d8394de20a86d2f`。
- `voice.dat`：1,531,127,141 bytes，SHA-256 `9b538f1b6b4aaa7178801b4dcf7a3c66110d7dbdb48f00520cf7a3227a08cdf4`。
- 本地提取的技术脚本 `analysis/message.nut`：12,638 bytes，SHA-256 `19109e461c15d62f73ef83b20e064e4a01d36502c8d8214cfbf47fdb8eccfb7d`。

## 资源证据

原 DAT name seed 为 `8C416067`；`voice` 目录 hash 为 `7471E515`；`CS01_C_10_AR_005.ogg` 文件名 hash 为 `19CDEDDA`。所有已检查原 DAT 索引均不含该文件名 hash；正确 voice 目录中相邻资源存在：AR004 ordinal 8243，AR006 ordinal 2799。AR001–004、006–015 均匹配。原 hash 表将 ASCII 大小写映射到同值，改变大小写无法补出 AR005。原、汉化两条真实 ACK 路线均在同一资源请求失败，不是翻译层重命名。

`message.nut` 的原消息逻辑计算 `VoiceVolume(m.mask)`，然后直接调用 `PlayVoice(m.voice, voice, pan/2 + pan/4)`。`SoundHook` 对 BGM/SE 宏的映射不参与此语音调用；没有把 AR005 改成其他声音的依据。

## 原 EXE 控制流

以下均为上述 EXE 的虚拟地址，ImageBase `00400000`：

1. `0051E89A` 注册 `PlayVoice` 的 Squirrel callback `0057C238`。17 bytes：`6838c257006862d275005653e8f97a0500`。
2. callback 在 `0057C2EE` 调用播放实现 `00447884`，随后 `0057C2F6` 清零返回值，正常返回给脚本。自 `0057C2ED` 起 13 bytes：`52e891b5ecff83c40c33c050ff`。
3. `004478E3` 压入原始语音名，`004478E4` 压入 `00745C84`（NUL 结束的字符串 `voice`），随后调用 `00465748`。返回后 `004478F2 test al, al`；`004478F4 je 00447EF7`，查找/打开未成功时直接跳到正常清理返回分支。自 `004478E3` 起 23 bytes：`5668845c740053e859de010083c40c84c00f84fd050000`。
4. `00447EF7` 开始正常清理，`00447F19 ret`，此分支不向脚本抛出异常。自 `00447EF7` 起 35 bytes：`66c745e80000eb0b66c745e81400e8e2312b008b45d864a3000000005f5e5b8be55dc3`。

因此：原语音资源查无时播放调用可以正常返回；共用运行时原先将查无资源传播为整场 Story fatal，造成了差异。

## 共用运行时修正与边界

- Speaker 消息先执行原脚本语音 predicate，再查询准确的 `voice/<name>.ogg`。查无时不创建 voice effect；与原 predicate 禁用语音一样，清空表示可播放语音的 `self.voice`，页面与新建日志不提供虚假的语音回放条目。
- 日志和快捷重播中的缺失旧资源返回显式 `audio_unavailable: true` 包，宿主无声返回。
- 只处理准确资源查询返回不存在；查找错误、存在资源的读取错误及非 Ogg 内容仍直接报错，不替换声音、不构造静音媒体、不关闭其他语音。
- 单测覆盖缺失后继续页面、下一条现有语音仍正常发出 effect、旧日志缺失结果、坏 Ogg 仍失败。真实原 `voice.dat` 的 ignored 测试用 `NEKO_SORAKOI_VOICE_DIR` 指定目录，验证 AR005 缺失及 AR004/006 有效。
