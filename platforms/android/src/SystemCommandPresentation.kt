package org.nekopack.probe

import android.graphics.Bitmap
import org.json.JSONObject

/** The mounted script owns waits and completion; Android supplies active time and input. */
class SystemCommandPresentation(
    private val timeline:SessionTimeline,
    private val present:(Bitmap)->Unit,
    private val request:(Long,Long,IntArray)->Unit
) {
    var active=false
        private set
    private var token=0L
    private var started=0L
    private var epoch=0L
    private var busy=false
    private var delay=0L
    private var hasFrame=false
    private var nativeReturnWake=false
    private var lastNativeReturnId=0L
    private val pendingInputs=java.util.ArrayDeque<IntArray>()

    fun finish(){epoch++;active=false;busy=false;nativeReturnWake=false;pendingInputs.clear()}
    fun accept(meta:JSONObject,bitmap:Bitmap?){
        val incoming=meta.getLong("token")
        if(!active||token!=incoming){active=true;token=incoming;started=timeline.now()-meta.getLong("elapsed_ms");nativeReturnWake=false;pendingInputs.clear()}
        if(meta.optBoolean("native_child_return")){
            val returned=meta.getLong("native_child_return_id")
            if(returned!=lastNativeReturnId){
                lastNativeReturnId=returned
                nativeReturnWake=incoming==meta.getLong("native_child_return_token")
            }
        }
        busy=false
        val generation=++epoch
        delay=meta.getLong("delay_ms");hasFrame=meta.getBoolean("has_frame")
        bitmap?.let(present)
        if(meta.optBoolean("native_ack") || meta.optBoolean("load_catalog")){pendingInputs.clear();send(intArrayOf(6));return}
        if(meta.optBoolean("save_config")){pendingInputs.clear();send(intArrayOf(4));return}
        if(nativeReturnWake){
            nativeReturnWake=false
            // A returned child window invalidates its uncovered parent. Native
            // wait(-1) also pumps paint/posted messages with game event zero.
            // Only this explicit return boundary receives one such message.
            if(!hasFrame && delay<0){send(intArrayOf(5));return}
        }
        if(pendingInputs.isNotEmpty()){sendPending();return}
        if(delay>=0)timeline.post(if(delay==0L)16L else delay){
            if(active&&!busy&&epoch==generation)send(intArrayOf(0))
        }
    }
    fun input(event:Int):Boolean {
        require(event in 1..2)
        if(!active)return false
        if(!timeline.paused){if(busy)pendingInputs.addLast(intArrayOf(event)) else send(intArrayOf(event));pendingInputs.addLast(intArrayOf(5))}
        return true
    }
    fun pointer(event:Int,x:Int,y:Int):Boolean {
        require(event in 0..3)
        if(!active)return false
        if(!timeline.paused){val input=intArrayOf(event,x,y);if(busy){
            if(event==3 && pendingInputs.peekLast()?.get(0)==3)pendingInputs.removeLast()
            pendingInputs.addLast(input)
        }else if(event==0 && delay==0L && !hasFrame){pendingInputs.addLast(input);send(intArrayOf(0))}else send(input)}
        return true
    }
    private fun sendPending(){
        // Native wait(0) does not block for a later release message. Finish a
        // no-render zero wait before delivering that release to the next wait.
        val event=pendingInputs.peekFirst()[0]
        if(delay==0L && !hasFrame && event in listOf(0,5))send(intArrayOf(0))
        else send(pendingInputs.removeFirst())
    }
    private fun send(event:IntArray){
        epoch++;busy=true
        request(token,timeline.now()-started,event)
    }
}
