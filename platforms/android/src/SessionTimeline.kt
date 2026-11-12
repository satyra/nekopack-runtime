package org.nekopack.probe

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import java.util.PriorityQueue

/** Main-thread queue. Keeps delays, order and in-flight completions across backgrounding. */
class SessionTimeline(private val rate:Long=1) {
    private data class Task(val at:Long,val order:Long,val run:()->Unit,val cancel:()->Unit)
    private val handler=Handler(Looper.getMainLooper())
    private val clock=ActiveClock{SystemClock.elapsedRealtime()}
    private val tasks=PriorityQueue<Task>(compareBy<Task>{it.at}.thenBy{it.order})
    private var order=0L
    private var closed=false
    val paused get()=clock.paused
    fun now()=clock.now()*rate
    fun wallDelay(activeDelay:Long)=(activeDelay.coerceAtLeast(0)+rate-1)/rate
    init {clock.pause()}
    private val dispatch=Runnable{drain()}
    private fun schedule(){
        handler.removeCallbacks(dispatch)
        if(!closed&&!paused&&tasks.isNotEmpty())handler.postDelayed(dispatch,wallDelay(tasks.element().at-now()))
    }
    private fun drain(){
        while(!closed&&!paused&&tasks.isNotEmpty()&&tasks.element().at<=now())tasks.remove().run()
        schedule()
    }
    fun post(delay:Long=0,cancel:()->Unit={},action:()->Unit){
        if(closed){cancel();return}
        tasks.add(Task(now()+delay,order++,action,cancel));schedule()
    }
    fun pause(){clock.pause();handler.removeCallbacks(dispatch)}
    fun resume(){if(!closed){clock.resume();schedule()}}
    fun close(){closed=true;handler.removeCallbacks(dispatch);while(tasks.isNotEmpty())tasks.remove().cancel()}
}
