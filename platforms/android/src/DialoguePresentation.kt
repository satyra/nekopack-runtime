package org.nekopack.probe

import android.widget.TextView
import org.json.JSONObject
import java.io.File

/** Device presentation only: Rust decides visibility, completion and advance permission. */
class DialoguePresentation(private val view:TextView,private val timeline:SessionTimeline,private val root:File,
    private val request:(Int,Long,Long,(JSONObject)->Unit)->Unit,
    private val ready:(Boolean)->Unit,private val advance:(Boolean)->Unit) {
    private var epoch=0L
    private var active=false
    private var busy=false
    private var pendingClick=false
    private var sequence=0L
    private var started=0L
    private var speaker=""
    fun stop(){epoch++;active=false;pendingClick=false}
    fun show(token:Long,name:String){
        stop();active=true;busy=false;sequence=token;speaker=name;started=timeline.now()
        view.text=speaker+"\n";ready(false);send(false,0)
    }
    fun click(){if(!active)return;ready(false);if(busy){pendingClick=true;return};send(true,timeline.now()-started)}
    private fun send(click:Boolean,elapsed:Long){
        busy=true;val generation=epoch;val token=sequence
        request(if(click)12 else 11,token,elapsed){state->
            if(active&&epoch==generation){
                busy=false;check(state.getLong("sequence")==token)
                view.text=speaker+"\n"+state.getString("text")
                val snapshot=File(root,"text-$token.tmp");snapshot.writeText(state.toString())
                check(snapshot.renameTo(File(root,"text-$token.json"))){"cannot publish text state"}
                val event=JSONObject(state.toString());event.remove("text");event.put("click",click)
                File(root,"text-events.jsonl").appendText(event.toString()+"\n")
                if(state.getBoolean("advance")){stop();ready(false);advance(state.getBoolean("stop_voice"))}
                else {
                    ready(true)
                    if(pendingClick){pendingClick=false;this.click()}
                    else if(!state.getBoolean("complete"))timeline.post(33){if(active&&epoch==generation&&!busy)send(false,timeline.now()-started)}
                }
            }
        }
    }
}
