package org.nekopack.probe

/** Category, command, fade and focus factors remain independent. */
class AudioGain(var command:Float) {
    var category=1f
    var fade=1f
    var duck=1f
    var voice=1f
    val output:Float get()=command*category*fade*duck*voice
}
