package org.nekopack.probe

/** Command gain and fade are independent factors; replacing a fade preserves its envelope. */
class AudioGain(private val command:Float) {
    var fade=1f
    var duck=1f
    val output:Float get()=command*fade*duck
}
