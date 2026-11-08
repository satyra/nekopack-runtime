package org.nekopack.probe

fun main(){
    var wall=100L
    val clock=ActiveClock{wall}
    val start=clock.now()
    wall+=80;clock.pause();clock.pause()
    wall+=5000;check(clock.now()-start==80L)
    clock.resume();clock.resume();wall+=20
    check(clock.now()-start==100L)
    clock.pause();wall+=9000;clock.resume();wall+=150
    check(clock.now()-start==250L)
    println("ActiveClock: repeated pause/resume excludes background time")
}
