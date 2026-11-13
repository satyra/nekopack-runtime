package org.nekopack.probe

import android.graphics.Bitmap
import org.json.JSONObject
import java.io.File

/** Device presentation only: Rust decides visibility, completion and advance permission. */
class DialoguePresentation(private val view:DialogueView,private val timeline:SessionTimeline,private val root:File,private val trace:Boolean,
    private val request:(Int,Long,Long,Long,(JSONObject,Bitmap?,JSONObject)->Unit)->Unit,
    private val ready:(Boolean)->Unit,private val advance:(Boolean)->Unit) {
    private var epoch=0L
    private var active=false
    private var busy=false
    private var pendingClick=false
    private var sequence=0L
    private var started=0L
    private var speaker=""
    private var paintedCount=-1L
    var visibleCount=0L
        private set
    fun stop(){epoch++;active=false;pendingClick=false;view.clear()}
    fun suspend(){epoch++;active=false;pendingClick=false;ready(false)}
    fun resume(){check(!active);active=true;busy=false;paintedCount=-1;ready(false);send(false,timeline.now()-started)}
    fun show(token:Long,name:String){
        stop();active=true;busy=false;sequence=token;speaker=name;started=timeline.now();paintedCount=-1;visibleCount=0
        ready(false);send(false,0)
    }
    fun refresh(){if(active&&!busy){ready(false);send(false,timeline.now()-started)}}
    fun click(){if(!active)return;ready(false);if(busy){pendingClick=true;return};send(true,timeline.now()-started)}
    private fun send(click:Boolean,elapsed:Long){
        busy=true;val generation=++epoch;val token=sequence;val sentAt=timeline.now()
        request(if(click)12 else 11,token,elapsed,paintedCount){state,bitmap,geometry->
            if(active&&epoch==generation){
                busy=false;check(state.getLong("sequence")==token);visibleCount=state.getLong("visible")
                if(bitmap!=null){
                    view.show(bitmap,geometry,speaker+"\n"+state.getString("text"))
                    paintedCount=if(state.optBoolean("message_animating"))-1 else visibleCount
                }
                if(trace){
                    val snapshot=File(root,"text-$token.tmp");snapshot.writeText(state.toString())
                    check(snapshot.renameTo(File(root,"text-$token.json"))){"cannot publish text state"}
                    val event=JSONObject(state.toString());event.remove("text");event.put("click",click)
                    File(root,"text-events.jsonl").appendText(event.toString()+"\n")
                }
                if(state.getBoolean("advance")){stop();ready(false);advance(state.getBoolean("stop_voice"))}
                else {
                    ready(true)
                    if(pendingClick){pendingClick=false;this.click()}
                    else if(!state.getBoolean("complete") || state.getBoolean("auto") || state.getBoolean("skip") || state.optBoolean("message_animating") || state.optBoolean("delay_pending"))timeline.post((33-(timeline.now()-sentAt)).coerceAtLeast(0)){if(active&&epoch==generation&&!busy)send(false,timeline.now()-started)}
                }
            }else bitmap?.recycle()
        }
    }
}
