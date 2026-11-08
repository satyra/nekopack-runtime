package org.nekopack.probe

/** Caller owns synchronization. Paused wall time never advances the runtime clock. */
class ActiveClock(private val wall:()->Long) {
    private var stoppedAt:Long?=null
    private var excluded=0L
    val paused get()=stoppedAt!=null
    fun now():Long=(stoppedAt ?: wall())-excluded
    fun pause(){if(!paused)stoppedAt=wall()}
    fun resume(){stoppedAt?.let{excluded+=wall()-it;stoppedAt=null}}
}
