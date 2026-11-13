# Common video decoder

`Decoder<R: Read + Seek>` detects MPEG program streams and ASF by header bytes,
independently of resource name, game, archive type, or localization. MPEG retains
pl_mpeg; ASF uses the optional FFmpeg bridge. Both currently require one video
and one audio stream and emit RGBA plus interleaved stereo signed 16-bit PCM.

Set `NEKO_FFMPEG_DIR` to an FFmpeg install prefix containing `include/` and
`lib/` when compiling. Without this environment variable MPEG remains available
and ASF returns an explicit unsupported-build error. Android distribution builds
must enable the dependency and package all five shared libraries.

The reproducible source build is `tools/build_ffmpeg.py`. It pins the official
FFmpeg 8.0.1 archive and verifies SHA256 before extracting. It uses LGPL-only
native decoders with ARM NEON/assembly enabled: ASF; WMV1/2/3, VC1; WMA1/2/Pro/Lossless; libswscale and
libswresample. No network protocols, GPL, nonfree, encoders, or game-specific
rules are enabled. Generated libraries, exact configure command, source archive,
and build manifest are under `.local/ffmpeg/` (not committed).

FFmpeg is LGPL-2.1-or-later under this configuration. The application must ship
its notice and license, make the exact corresponding source and build commands
available, and allow replacement/relinking of these shared libraries. See
https://ffmpeg.org/legal.html and the pinned source `LICENSE.md` and
`COPYING.LGPLv2.1`. FFmpeg source is unmodified.

## Timing and EOF

`Info.duration` is container metadata, not a decoded-frame-count promise.
`Frame.video_time` is the most recently emitted video frame's presentation time
in seconds relative to container start. `video_end_time` adds its declared frame
duration (or 1/fps when absent). Empty RGBA/PCM blocks are valid. `audio_end_time`
includes leading silence up to the first decoded audio PTS (or trims negative
preroll), so audio and video share the container start. It is the cumulative
number of actual stereo PCM sample frames divided by output
sample rate; `decoded_video_frames` counts actual emitted RGBA frames.

ASF demux reads ahead into separate compressed packet queues, bounded to 8 MiB
and 4096 packets in total. Before decoding the next video frame, it supplies
audio up to the previous video interval plus 100 ms. This handles ASF files whose
physical video packet order leads audio by seconds without buffering RGBA frames.
Each decoder flushes only after its own queued packets are consumed.

ASF `ended` becomes true only after demux EOF, both decoder flushes have reached
EOF, and the audio resampler has drained. Presentation should wait until the
last video interval and queued audio complete. Do not derive expected ASF frame
counts from its duration. Decode errors and input I/O errors return an error.

## Resource accounting

MPEG allocation budget and `peak_bytes` retain the pl_mpeg hard arena accounting.
ASF enforces dimension (1920x1080), duration (600 s metadata / 603 s decoded),
sample-rate, stream-count, single-thread decoder, packet, and per-step I/O limits.
ASF accepts budgets of at least 32 MiB, but **FFmpeg's internal heap is not charged
to the arena and this is not a hard total-memory cap**. ASF `peak_bytes` reports
only bridge-owned output/input-buffer payload, excluding FFmpeg internal heap;
`live_bytes()` currently counts only pl_mpeg allocations. Callers must not use
these counters as total-process-memory proof for ASF.
