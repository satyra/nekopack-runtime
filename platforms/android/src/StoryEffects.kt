package org.nekopack.probe

import android.content.Context
import android.media.MediaPlayer
import android.media.AudioManager
import android.media.AudioAttributes
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.animation.ValueAnimator
import android.animation.Animator
import android.animation.AnimatorListenerAdapter
import android.view.animation.LinearInterpolator
import android.widget.ImageView
import android.graphics.Bitmap
import org.json.JSONObject
import java.io.File

/** Main-looper adapter; Rust owns sequencing, this class owns devices and completion. */
class StoryEffects(private val context:Context,private val image:ImageView,private val overlay:ImageView,private val root:File,private val failure:(Throwable)->Unit) {
    private val handler=Handler(Looper.getMainLooper())
    private val players=mutableMapOf<String,Pair<MediaPlayer,File>>()
    private var animator:ValueAnimator?=null
    private var closed=false
    private val audio=context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var focused=false
    private val focus=AudioManager.OnAudioFocusChangeListener { change->if(change<0 && !closed)failure(IllegalStateException("音频焦点已丢失")) }
    private fun log(event:String,kind:String,token:Long,extra:Long=0) {
        File(root,"host-events.jsonl").appendText(JSONObject().put("event",event).put("kind",kind).put("token",token).put("time_ms",SystemClock.elapsedRealtime()).put("value",extra).toString()+"\n")
    }
    fun stopVoice(){ release("voice") }
    private fun release(kind:String){players.remove(kind)?.let{(player,file)->player.release();check(file.delete()){"cannot delete audio cache"}}}
    fun close(){
        if(closed)return
        closed=true;handler.removeCallbacksAndMessages(null);animator?.cancel();animator=null
        val owned=players.values.toList();players.clear()
        for((player,_) in owned)player.release()
        if(focused)audio.abandonAudioFocus(focus)
        val undeleted=owned.map{it.second}.filter{!it.delete()}
        log("released","all",0,players.size.toLong())
        if(undeleted.isNotEmpty())failure(IllegalStateException("audio cache cleanup failed"))
    }
    fun run(meta:JSONObject,bitmap:Bitmap?,file:File?,complete:()->Unit){
        if(closed){file?.delete();return}
        val kind=meta.getString("host_effect");val token=meta.getLong("token");val duration=meta.getLong("duration_ms")
        val started=SystemClock.elapsedRealtime();log("begin",kind,token,duration)
        fun done(){if(!closed){log("ack",kind,token,SystemClock.elapsedRealtime()-started);complete()}}
        try {
            when(kind){
                "wait"->handler.postDelayed({done()},duration)
                "fade"->{
                    check(bitmap!=null);overlay.setImageBitmap(bitmap);overlay.alpha=0f
                    val animation=ValueAnimator.ofFloat(0f,1f);animator=animation
                    animation.duration=duration;animation.interpolator=LinearInterpolator()
                    var sampled=false
                    animation.addUpdateListener{val alpha=it.animatedValue as Float;overlay.alpha=alpha
                        if(!sampled && alpha>=0.25f && alpha<=0.75f){sampled=true;log("frame",kind,token,(alpha*1000).toLong())}
                    }
                    animation.addListener(object:AnimatorListenerAdapter(){override fun onAnimationEnd(animation:Animator){
                        if(closed)return
                        image.setImageBitmap(bitmap);overlay.setImageDrawable(null);overlay.alpha=0f;animator=null
                        val remaining=(duration-(SystemClock.elapsedRealtime()-started)).coerceAtLeast(0)
                        handler.postDelayed({done()},remaining)
                    }})
                    animation.start()
                }
                "bgm","se","voice"->{
                    check(file!=null)
                    if(!focused){check(audio.requestAudioFocus(focus,AudioManager.STREAM_MUSIC,AudioManager.AUDIOFOCUS_GAIN)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED){"audio focus denied"};focused=true}
                    release(kind)
                    val player=MediaPlayer();players[kind]=player to file
                    player.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).setContentType(if(kind=="voice")AudioAttributes.CONTENT_TYPE_SPEECH else AudioAttributes.CONTENT_TYPE_MUSIC).build())
                    player.setDataSource(file.absolutePath)
                    player.setOnPreparedListener{
                        if(!closed && players[kind]?.first===it){
                            try{it.isLooping=kind=="bgm";it.start();check(it.isPlaying){"audio did not start"};log("playing",kind,token,it.duration.toLong());done()
                                handler.postDelayed({if(!closed&&players[kind]?.first===player&&player.isPlaying)log("progress",kind,token,player.currentPosition.toLong())},200)}
                            catch(error:Throwable){failure(error)}
                        }
                    }
                    player.setOnCompletionListener{if(!closed&&players[kind]?.first===it){log("completed",kind,token,it.currentPosition.toLong());release(kind)}}
                    player.setOnErrorListener{_,what,extra->if(!closed)failure(IllegalStateException("audio error $what/$extra"));true}
                    player.prepareAsync()
                }
                else->error("unknown host effect $kind")
            }
        }catch(error:Throwable){file?.let{if(players.values.none{p->p.second==it})it.delete()};failure(error)}
    }
}
