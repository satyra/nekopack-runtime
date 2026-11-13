package org.nekopack.probe

fun main(){
    val gain=VoiceMusicGain()
    check(gain.value(0)==1f)
    check(gain.setActive(true,10));check(gain.value(10)==.5f)
    check(!gain.setActive(true,50))
    check(gain.setActive(false,100));check(gain.value(100)==.5f)
    check(gain.value(600)==.75f);check(gain.pending(1099));check(!gain.pending(1100));check(gain.value(1100)==1f)
    gain.setActive(true,1200);gain.setActive(false,1300)
    check(gain.value(1800)==.75f);gain.setActive(true,1800);check(gain.value(1800)==.5f)
    val output=AudioGain(.5f);output.category=.8f;output.voice=gain.value(1800);output.duck=.2f
    check(kotlin.math.abs(output.output-.04f)<.000001f)
    output.command=1f;check(kotlin.math.abs(output.output-.08f)<.000001f)
    println("VoiceMusicGain: original 50% duck, 1000ms restore, interruption and independent factors passed")
}
