package org.nekopack.probe

fun main() {
    val bgm=AudioGain(.5f)
    check(bgm.output==.5f)
    bgm.fade=.4f
    check(bgm.output==.2f)
    // A second fade starts at the current envelope, not at full volume.
    bgm.duck=.2f
    check(kotlin.math.abs(bgm.output-.04f)<.000001f)
    bgm.duck=1f
    check(bgm.output==.2f)
    val secondStart=bgm.fade
    for(step in 0..10) {
        bgm.fade=secondStart*(1-step/10f)
        check(bgm.output<=.2f)
    }
    check(bgm.output==0f)
    check(AudioGain(.5f).output==.5f) // A new track resets the envelope.
    check(AudioGain(1f).output==1f) // SE gain stays independent.
    val configured=AudioGain(.5f)
    configured.category=.5f
    check(configured.output==.25f)
    configured.fade=.4f;configured.duck=.2f
    check(kotlin.math.abs(configured.output-.02f)<.000001f)
    configured.category=0f;check(configured.output==0f)
    configured.category=1f
    check(configured.fade==.4f && configured.duck==.2f)
    check(kotlin.math.abs(configured.output-.04f)<.000001f)
    val voice=AudioGain(1f);voice.category=.75f;check(voice.output==.75f)
    println("AudioGain: command gain, repeated fade, replacement and SE passed")
}
