package org.nekopack.probe

import android.content.Context
import android.media.AudioManager
import android.media.AudioAttributes
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
class StoryEffects(private val context:Context,private val image:ImageView,private val overlay:ImageView,private val root:File,private val timeline:SessionTimeline,private val wipeFrame:(String,Long,Long,(Bitmap,Boolean)->Unit)->Unit,private val audioTimeline:SessionTimeline=timeline,private val failure:(Throwable)->Unit) {
    private val players=mutableMapOf<String,Pair<ChannelPlayer,File>>()
    private var animator:ValueAnimator?=null
    private var bgmAnimator:ValueAnimator?=null
    private val gains=mutableMapOf<String,AudioGain>()
    private val categoryVolumes=mutableMapOf<String,Int>()
    fun configureAudio(config:JSONObject) {
        for(kind in listOf("bgm","voice","se","env","system")){
            val percent=config.getInt(kind);check(percent in 0..100)
            categoryVolumes[kind]=percent
        }
        for(kind in gains.keys)applyGain(kind)
    }
    private val focusPaused=mutableSetOf<String>()
    private val pendingStart=mutableMapOf<String,()->Unit>()
    private val audioTokens=mutableMapOf<String,Long>()
    private var audioPaused=false
    private var focusSuspended=false
    private var ducked=false
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
                val started=timeline.now()
                motionLog("begin",sequence,0);motionLog("frame",sequence,0)
                fun frame(){
                    if(!isMotionActive(sequence))return
                    val elapsed=timeline.now()-started
                    wipeFrame("motion",sequence,elapsed){value,finished->
                        if(isMotionActive(sequence)){
                            image.setImageBitmap(value);motionLog("frame",sequence,elapsed,finished)
                            if(finished){motionFinished=true;motionLog("finished",sequence,elapsed,true)}
                            else timeline.post(16){frame()}
                        }
                    }
                }
                timeline.post(16){frame()}
            }
        }
    }

    private val audio=context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var focused=false
    private fun applyGain(kind:String) {
        val gain=gains.getValue(kind)
        gain.category=categoryVolumes.getValue(kind)/100f
        gain.duck=if(ducked).2f else 1f
        players.getValue(kind).first.setVolume(gain.output,gain.output)
        File(root,"audio-gain-events.jsonl").appendText(JSONObject().put("kind",kind).put("token",audioTokens.getValue(kind)).put("category_percent",categoryVolumes.getValue(kind)).put("output",gain.output).put("fade",gain.fade).put("duck",gain.duck).put("player",System.identityHashCode(players.getValue(kind).first)).put("time_ms",SystemClock.elapsedRealtime()).toString()+"\n")
    }
    private fun regainFocus() {
        if(audioPaused){focused=false;return}
        focused=true;focusSuspended=false;ducked=false
        for(kind in gains.keys)applyGain(kind)
        for(kind in focusPaused) {
            val player=players.getValue(kind).first
            player.start()
            val playbackStarted=SystemClock.elapsedRealtime()
            val prepared=pendingStart.remove(kind)
            log(if(prepared!=null)"playing" else "resumed",kind,audioTokens.getValue(kind),player.duration.toLong(),playbackStarted)
            prepared?.invoke()
        }
        focusPaused.clear();bgmAnimator?.resume()
    }
    private val focus=AudioManager.OnAudioFocusChangeListener { change->
        if(!closed) {
            log("focus","audio",0,change.toLong())
            when(change) {
                AudioManager.AUDIOFOCUS_GAIN->regainFocus()
                AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK->{
                    ducked=true;for(kind in gains.keys)applyGain(kind)
                }
                AudioManager.AUDIOFOCUS_LOSS_TRANSIENT,AudioManager.AUDIOFOCUS_LOSS->{
                    focusSuspended=true
                    if(change==AudioManager.AUDIOFOCUS_LOSS)focused=false
                    for(kind in gains.keys) {
                        val player=players.getValue(kind).first
                        if(player.isPlaying){player.pause();focusPaused.add(kind);log("paused",kind,audioTokens.getValue(kind),player.currentPosition.toLong())}
                    }
                    bgmAnimator?.pause()
                }
            }
        }
    }
    private fun log(event:String,kind:String,token:Long,extra:Long=0,observedAt:Long=SystemClock.elapsedRealtime()) {
        File(root,"host-events.jsonl").appendText(JSONObject().put("event",event).put("kind",kind).put("token",token).put("time_ms",observedAt).put("value",extra).toString()+"\n")
    }
    internal fun lifecycleAudio(event:String){
        val channels=JSONObject()
        for(kind in gains.keys){
            val player=players.getValue(kind).first
            channels.put(kind,JSONObject().put("token",audioTokens.getValue(kind)).put("player_id",System.identityHashCode(player)).put("playing",player.isPlaying).put("position",player.currentPosition).put("gain",gains.getValue(kind).output))
        }
        File(root,"audio-lifecycle.jsonl").appendText(JSONObject().put("event",event).put("time_ms",SystemClock.elapsedRealtime()).put("channels",channels).toString()+"\n")
    }
    fun pause(){
        if(closed)return
        audioPaused=true
        for(kind in gains.keys){
            val player=players.getValue(kind).first
            if(player.isPlaying){player.pause();focusPaused.add(kind);log("paused",kind,audioTokens.getValue(kind),player.currentPosition.toLong())}
        }
        animator?.pause();bgmAnimator?.pause()
        audio.abandonAudioFocus(focus);focused=false
        log("lifecycle_paused","all",0,timeline.now());lifecycleAudio("paused")
    }
    fun resume(){
        if(closed)return
        audioPaused=false
        lifecycleAudio("resuming")
        if(players.isNotEmpty()){
            check(audio.requestAudioFocus(focus,AudioManager.STREAM_MUSIC,AudioManager.AUDIOFOCUS_GAIN)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED){"audio focus denied on resume"}
            regainFocus()
        }
        animator?.resume()
        log("lifecycle_resumed","all",0,timeline.now());lifecycleAudio("resumed")
    }
    fun beginMovie(token:Long){
        for(kind in players.keys.toList())release(kind)
        if(focused){audio.abandonAudioFocus(focus);focused=false}
        log("stopped_all","movie",token,players.size.toLong())
    }
    internal fun audioSnapshot():JSONObject {
        val player=players.getValue("bgm").first
        return JSONObject().put("playing",player.isPlaying).put("position",player.currentPosition)
            .put("gain",gains.getValue("bgm").output).put("suspended",focusSuspended).put("token",audioTokens.getValue("bgm"))
    }
    // Completed players are released; a focus-paused player remains owned and unfinished.
    fun voiceFinished() = !players.containsKey("voice")
    fun stopVoice(){ release("voice") }
    fun finishDialogue(sequence:Long,stop:Boolean){
        val player=players["voice"]?.first
        val event=JSONObject().put("sequence",sequence).put("stop",stop).put("voice_token",audioTokens["voice"] ?: -1)
            .put("player",player?.let{System.identityHashCode(it)} ?: 0).put("playing_before",player?.isPlaying ?: false)
            .put("position_before",player?.currentPosition ?: -1)
        if(stop)stopVoice()
        event.put("retained",player!=null && players["voice"]?.first===player).put("playing_after",players["voice"]?.first?.isPlaying ?: false)
        File(root,"voice-next-events.jsonl").appendText(event.toString()+"\n")
    }
    private fun release(kind:String){if(kind=="bgm"){bgmAnimator?.cancel();bgmAnimator=null};focusPaused.remove(kind);pendingStart.remove(kind);audioTokens.remove(kind);gains.remove(kind);players.remove(kind)?.let{(player,file)->player.release();check(file.delete()){"cannot delete audio cache"}}}
    internal fun cleanupState():JSONObject = JSONObject().put("closed",closed).put("players",players.size)
        .put("pending_start",pendingStart.size).put("paused_players",focusPaused.size).put("gains",gains.size).put("tokens",audioTokens.size)
        .put("focused",focused).put("motion_active",motionSequence!=0L).put("animator",animator!=null).put("bgm_animator",bgmAnimator!=null)
        .put("audio_cache_files",root.listFiles()!!.count{it.name.startsWith("audio-")&&it.extension=="ogg"})
    fun resetForLoad(){
        check(!closed)
        stopMotion();animator?.cancel();animator=null;bgmAnimator?.cancel();bgmAnimator=null
        for(kind in players.keys.toList())if(kind!="system")release(kind)
        check(players.keys.all{it=="system"}&&pendingStart.keys.all{it=="system"}&&focusPaused.all{it=="system"})
        log("load_released","all",0,0)
        audioTokens["system"]?.let{log("load_system_retained","system",it)}
    }
    fun close(){
        if(closed)return
        stopMotion();closed=true;timeline.close();animator?.cancel();animator=null;bgmAnimator?.cancel();bgmAnimator=null
        val owned=players.values.toList();players.clear();gains.clear();focusPaused.clear();pendingStart.clear();audioTokens.clear()
        for((player,_) in owned)player.release()
        if(focused)audio.abandonAudioFocus(focus)
        focused=false
        val undeleted=owned.map{it.second}.filter{!it.delete()}
        log("released","all",0,players.size.toLong())
        if(undeleted.isNotEmpty())failure(IllegalStateException("audio cache cleanup failed"))
    }
    fun configureCategory(kind:String,percent:Int){check(percent in 0..100);categoryVolumes[kind]=percent}
    fun run(meta:JSONObject,bitmap:Bitmap?,file:File?,shortClip:ShortClip?=null,complete:()->Unit){
        if(closed){file?.delete();return}
        val kind=meta.getString("host_effect");val token=meta.getLong("token");val duration=meta.getLong("duration_ms")
        val started=timeline.now();log("begin",kind,token,duration)
        fun done(){(if(meta.optBoolean("ui_audio"))audioTimeline else timeline).post{if(!closed){log("ack",kind,token,timeline.now()-started);complete()}}}
        try {
            when(kind){
                "wait"->timeline.post(duration){done()}
                "env_stop","se_stop"->{
                    val channel=kind.removeSuffix("_stop")
                    release(channel)
                    log("stopped",kind,token,if(players.containsKey(channel))1 else 0)
                    players["bgm"]?.first?.let{if(it.isPlaying)log("peer_playing","bgm",token,it.currentPosition.toLong())}
                    done()
                }
                "bgm_stop"->{
                    release("bgm")
                    log("stopped",kind,token,if(players.containsKey("bgm"))1 else 0)
                    done()
                }
                "bgm_fade"->{
                    bgmAnimator?.cancel();bgmAnimator=null
                    val player=players["bgm"]?.first
                    if(player==null){log("stopped","bgm",token);done()} else {
                        val gain=gains.getValue("bgm")
                        val from=gain.fade
                        log("fade_start_gain","bgm",token,(gain.output*1000).toLong())
                        val animation=ValueAnimator.ofFloat(from,0f);bgmAnimator=animation
                        animation.duration=duration;animation.interpolator=LinearInterpolator()
                        var cancelled=false;var sampled=false
                        animation.addUpdateListener{
                            val volume=it.animatedValue as Float
                            gain.fade=volume
                            player.setVolume(gain.output,gain.output)
                            if(!sampled&&volume>=0.25f&&volume<=0.75f){sampled=true;log("frame",kind,token,(volume*1000).toLong())}
                        }
                        animation.addListener(object:AnimatorListenerAdapter(){
                            override fun onAnimationCancel(animation:Animator){cancelled=true;log("cancelled",kind,token)}
                            override fun onAnimationEnd(animation:Animator){
                                if(closed||cancelled)return
                                bgmAnimator=null;release("bgm");log("stopped","bgm",token)
                                log("finished",kind,token,timeline.now()-started)
                            }
                        })
                        animation.start();if(focusSuspended||audioPaused)animation.pause();done()
                    }
                }
                "flash"->{
                    check(bitmap!=null)
                    val phases=duration/30;var phase=0L
                    fun frame(){
                        if(closed)return
                        wipeFrame(kind,token,phase){value,_->
                            if(!closed){image.setImageBitmap(value);log("frame",kind,token,phase);phase++
                                if(phase==phases)done() else timeline.post(30){frame()}
                            }
                        }
                    }
                    timeline.post(30){frame()}
                }
                "wipe","shake"->{
                    check(bitmap!=null)
                    fun frame(){
                        if(closed)return
                        val elapsed=(timeline.now()-started).coerceAtMost(duration)
                        wipeFrame(kind,token,elapsed){value,_->
                            if(!closed){image.setImageBitmap(value);log("frame",kind,token,elapsed)
                                if(elapsed==duration){if(kind=="wipe")image.setImageBitmap(bitmap);done()}
                                else timeline.post(16){frame()}
                            }
                        }
                    }
                    image.postOnAnimation{timeline.post{frame()}}
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
                        val remaining=(duration-(timeline.now()-started)).coerceAtLeast(0)
                        timeline.post(remaining){done()}
                    }})
                    animation.start()
                }
                "bgm","env","se","voice","system"->{
                    check(file!=null)
                    if(!focused&&!audioPaused){check(audio.requestAudioFocus(focus,AudioManager.STREAM_MUSIC,AudioManager.AUDIOFOCUS_GAIN)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED){"audio focus denied"};regainFocus()}
                    release(kind)
                    val player=ChannelPlayer(shortClip);players[kind]=player to file
                    player.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).setContentType(if(kind=="voice")AudioAttributes.CONTENT_TYPE_SPEECH else AudioAttributes.CONTENT_TYPE_MUSIC).build())
                    player.setDataSource(file.absolutePath)
                    player.setOnPreparedListener{
                        if(!closed && players[kind]?.first===it){
                            try{
                                audioTokens[kind]=token
                                if(player.pcmFrames!=null){log("pcm_frames",kind,token,player.pcmFrames!!.toLong());log("pcm_rate",kind,token,player.pcmRate!!.toLong())}
                                if(kind=="bgm" || kind=="env" || kind=="se"){
                                    it.isLooping=meta.getBoolean("loop")
                                    val percent=meta.getInt("volume_percent")
                                    val gain=AudioGain(percent/100f);gains[kind]=gain
                                    it.setVolume(gain.output,gain.output);log("gain",kind,token,percent.toLong())
                                }else {it.isLooping=false;gains[kind]=AudioGain(1f)}
                                applyGain(kind)
                                if(focusSuspended||audioPaused){focusPaused.add(kind);pendingStart[kind]={done()};log("focus_pending",kind,token)}
                                else {it.start();val playbackStarted=SystemClock.elapsedRealtime();check(it.isPlaying){"audio did not start"};log("playing",kind,token,it.duration.toLong(),playbackStarted)}
                                if(kind=="env" || kind=="se"){
                                    log("looping",kind,token,if(it.isLooping)1 else 0)
                                    players["bgm"]?.first?.let{peer->if(peer.isPlaying)log("peer_playing","bgm",token,peer.currentPosition.toLong())}
                                }
                                if(!focusSuspended&&!audioPaused)done()
                                val sampleDelay=if(kind=="env" || (kind=="se"&&player.isLooping))200L else 50L
                                val progress=object:Runnable {
                                    private var samples=0
                                    private var previousPosition=-1L
                                    override fun run(){
                                        if(closed||players[kind]?.first!==player)return
                                        if(!player.isPlaying){if(focusPaused.contains(kind))audioTimeline.post(200){run()};return}
                                        val position=player.currentPosition.toLong();samples++
                                        player.playbackHead?.let{head->log("pcm_head",kind,token,head)}
                                        log("progress",kind,token,position)
                                        if((kind=="env" || kind=="se") && position<previousPosition)log("looped",kind,token,position)
                                        previousPosition=position
                                        if(kind=="env" || (kind=="se" && player.isLooping) || (position==0L&&samples*sampleDelay<5000))audioTimeline.post(sampleDelay){run()}
                                    }
                                };audioTimeline.post(sampleDelay){progress.run()}}
                            catch(error:Throwable){failure(error)}
                        }
                    }
                    player.setOnCompletionListener{if(!closed&&players[kind]?.first===it){it.playbackHead?.let{head->log("pcm_head",kind,token,head)};log("completed",kind,token,it.currentPosition.toLong());release(kind)}}
                    player.setOnErrorListener{failed,what,extra->if(!closed&&players[kind]?.first===failed)failure(IllegalStateException("audio error $what/$extra"));true}
                    player.prepareAsync()
                }
                else->error("unknown host effect $kind")
            }
        }catch(error:Throwable){file?.let{if(players.values.none{p->p.second==it})it.delete()};failure(error)}
    }
}
