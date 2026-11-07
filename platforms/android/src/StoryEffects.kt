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
class StoryEffects(private val context:Context,private val image:ImageView,private val overlay:ImageView,private val root:File,private val wipeFrame:(String,Long,Long,(Bitmap,Boolean)->Unit)->Unit,private val failure:(Throwable)->Unit) {
    private val handler=Handler(Looper.getMainLooper())
    private val players=mutableMapOf<String,Pair<MediaPlayer,File>>()
    private var animator:ValueAnimator?=null
    private var bgmAnimator:ValueAnimator?=null
    private var closed=false
    @Volatile private var motionSequence=0L
    private var motionFinished=false
    fun isMotionActive(sequence:Long)=!closed && motionSequence==sequence
    private fun motionLog(event:String,sequence:Long,elapsed:Long,finished:Boolean=false) {
        File(root,"motion-events.jsonl").appendText(JSONObject().put("event",event).put("sequence",sequence).put("elapsed_ms",elapsed).put("finished",finished).put("time_ms",SystemClock.elapsedRealtime()).toString()+"\n")
    }
    fun stopMotion(){
        val old=motionSequence;motionSequence=0
        if(old!=0L&&!motionFinished)motionLog("cancelled",old,0)
    }
    // Runs alongside the visible dialogue and voice; no host-effect ACK or script advancement.
    fun runMotion(sequence:Long){
        stopMotion();motionSequence=sequence;motionFinished=false
        wipeFrame("motion",sequence,0){initial,_->
            if(isMotionActive(sequence)){
                image.setImageBitmap(initial)
                val started=SystemClock.elapsedRealtime()
                motionLog("begin",sequence,0);motionLog("frame",sequence,0)
                fun frame(){
                    if(!isMotionActive(sequence))return
                    val elapsed=SystemClock.elapsedRealtime()-started
                    wipeFrame("motion",sequence,elapsed){value,finished->
                        if(isMotionActive(sequence)){
                            image.setImageBitmap(value);motionLog("frame",sequence,elapsed,finished)
                            if(finished){motionFinished=true;motionLog("finished",sequence,elapsed,true)}
                            else handler.postDelayed({frame()},16)
                        }
                    }
                }
                handler.postDelayed({frame()},16)
            }
        }
    }

    private val audio=context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var focused=false
    private val focus=AudioManager.OnAudioFocusChangeListener { change->if(change<0 && !closed)failure(IllegalStateException("音频焦点已丢失")) }
    private fun log(event:String,kind:String,token:Long,extra:Long=0) {
        File(root,"host-events.jsonl").appendText(JSONObject().put("event",event).put("kind",kind).put("token",token).put("time_ms",SystemClock.elapsedRealtime()).put("value",extra).toString()+"\n")
    }
    fun beginMovie(token:Long){
        for(kind in players.keys.toList())release(kind)
        if(focused){audio.abandonAudioFocus(focus);focused=false}
        log("stopped_all","movie",token,players.size.toLong())
    }
    fun stopVoice(){ release("voice") }
    private fun release(kind:String){if(kind=="bgm"){bgmAnimator?.cancel();bgmAnimator=null};players.remove(kind)?.let{(player,file)->player.release();check(file.delete()){"cannot delete audio cache"}}}
    fun close(){
        if(closed)return
        stopMotion();closed=true;handler.removeCallbacksAndMessages(null);animator?.cancel();animator=null;bgmAnimator?.cancel();bgmAnimator=null
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
                "bgm_stop"->{
                    release("bgm")
                    log("stopped",kind,token,if(players.containsKey("bgm"))1 else 0)
                    done()
                }
                "bgm_fade"->{
                    bgmAnimator?.cancel();bgmAnimator=null
                    val player=players["bgm"]?.first
                    if(player==null){log("stopped","bgm",token);done()} else {
                        val animation=ValueAnimator.ofFloat(1f,0f);bgmAnimator=animation
                        animation.duration=duration;animation.interpolator=LinearInterpolator()
                        var cancelled=false;var sampled=false
                        animation.addUpdateListener{
                            val volume=it.animatedValue as Float
                            player.setVolume(volume,volume)
                            if(!sampled&&volume>=0.25f&&volume<=0.75f){sampled=true;log("frame",kind,token,(volume*1000).toLong())}
                        }
                        animation.addListener(object:AnimatorListenerAdapter(){
                            override fun onAnimationCancel(animation:Animator){cancelled=true;log("cancelled",kind,token)}
                            override fun onAnimationEnd(animation:Animator){
                                if(closed||cancelled)return
                                bgmAnimator=null;release("bgm");log("stopped","bgm",token)
                                log("finished",kind,token,SystemClock.elapsedRealtime()-started)
                            }
                        })
                        animation.start();done()
                    }
                }
                "flash"->{
                    check(bitmap!=null)
                    val phases=duration/30;var phase=0L
                    fun frame(){
                        if(closed)return
                        wipeFrame(kind,token,phase){value,_->
                            if(!closed){image.setImageBitmap(value);log("frame",kind,token,phase);phase++
                                if(phase==phases)done() else handler.postDelayed({frame()},30)
                            }
                        }
                    }
                    handler.postDelayed({frame()},30)
                }
                "wipe","shake"->{
                    check(bitmap!=null)
                    fun frame(){
                        if(closed)return
                        val elapsed=(SystemClock.elapsedRealtime()-started).coerceAtMost(duration)
                        wipeFrame(kind,token,elapsed){value,_->
                            if(!closed){image.setImageBitmap(value);log("frame",kind,token,elapsed)
                                if(elapsed==duration){if(kind=="wipe")image.setImageBitmap(bitmap);done()}
                                else handler.postDelayed({frame()},16)
                            }
                        }
                    }
                    image.postOnAnimation{frame()}
                }
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
                                handler.postDelayed(object:Runnable {
                                    private var samples=0
                                    override fun run(){
                                        if(closed||players[kind]?.first!==player||!player.isPlaying)return
                                        val position=player.currentPosition.toLong();samples++
                                        log("progress",kind,token,position)
                                        if(position==0L&&samples<25)handler.postDelayed(this,200)
                                    }
                                },200)}
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
