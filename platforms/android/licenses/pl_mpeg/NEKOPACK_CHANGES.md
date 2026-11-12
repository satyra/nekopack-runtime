# Local pl_mpeg changes

Base: phoboslab/pl_mpeg `c871f2be022ece7ef4f64230b4fb8e1fb9eb6023` (MIT).
This is a **modified version**, not the pristine upstream header. `UPSTREAM.json`
records both digests. Author and license declarations remain intact.

1. **Drain a pending reference frame after a final B picture.** At EOF,
   `has_reference_frame` describes the still-pending backward reference; the most
   recently decoded picture can be B. Restricting the drain to the last picture
   being I/P dropped the final display frame in BIRTH (4195 versus FFmpeg 4196).
   The drain still clears the flag, so it happens once; the no-B-frame fast path
   is unchanged. Other three original videos retain their original counts.
2. **MP2 quantizer level normalization.** A quantizer has `levels` reconstruction
   values centered at `(levels-1)/2`; its scale divisor is that level count.
   Multiplication precedes division to retain fixed-point precision.
   Dividing by `levels+1` attenuated different subbands differently, especially
   low-bit-rate audio. The correction is derived from that mapping; no FFmpeg
   implementation source was copied.
3. **MP2 synthesis normalization.** Normalize the fixed-point synthesis output by
   Q30 (`2^30`). Upstream divides by `65*2^24`, introducing additional `64/65`
   gain. The PCM adapter retains explicit saturation; this does not remove
   clipping checks or multiply output based on a per-game guessed volume.

Independent FFmpeg raw decodes measure frame counts and PCM differences before
and after the changes. Initial mismatches are preserved in
`research/movie-batch14-initial-{counts,decoder-differences}.json`; final evidence
is under the same batch prefix. This is not a claim of original DirectShow
bit-exact output, complete codec conformance, or fuzz-tested malformed input.

No other upstream logic is changed. Bounded allocation, I/O callbacks, exception
containment and C ABI ownership are in `crates/nekopack-video/src/decoder.cpp`.
