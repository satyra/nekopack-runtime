package org.nekopack.probe

import android.app.Activity
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Rect
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.net.Uri
import android.os.SystemClock
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.widget.FrameLayout
import org.json.JSONObject
import java.io.File
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger

/** One worker owns native decoding and PCM; a bounded renderer owns Surface draws. */
class MovieHost(private val activity:Activity,private val stage:FrameLayout,private val root:File,private val diagnostics:Boolean=false) {
    private external fun nativeMovie(fd:Int,operation:Int):ByteArray
    private val worker=Executors.newSingleThreadExecutor()
    private val reason=AtomicInteger(-1)
    private var launched=false
    @Volatile private var closed=false
    val isActive get() = !closed
    private val gate=Object()
    private val drawLock=Object()
    private val clock=ActiveClock{SystemClock.elapsedRealtime()}
    private var background=false
    private var surfaceReady=false
    private var focusSuspended=false
    private var focused=false
    private var activeTrack:AudioTrack?=null
    private var trackPlaying=false
    private var volume=1f
    private var eventLog:(String,JSONObject)->Unit={_,_->}
    init {clock.pause()}
    private fun updateGate(){
        if(background||!surfaceReady||focusSuspended){
            clock.pause()
            if(trackPlaying){activeTrack!!.pause();trackPlaying=false}
        }else clock.resume()
        gate.notifyAll()
    }
    fun pause(){synchronized(gate){
        if(closed||reason.get()>=0)return
        background=true;updateGate()
        if(focused){audio.abandonAudioFocus(focus);focused=false}
        eventLog("paused",JSONObject().put("head",activeTrack?.playbackHeadPosition ?: 0).put("active_ms",clock.now()))
    }}
    fun resume(){synchronized(gate){
        if(closed||reason.get()>=0)return
        background=false
        if(!focused){
            check(audio.requestAudioFocus(focus,AudioManager.STREAM_MUSIC,AudioManager.AUDIOFOCUS_GAIN)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED){"movie audio focus denied on resume"}
            focused=true;focusSuspended=false
        }
        updateGate()
        eventLog("resumed",JSONObject().put("head",activeTrack?.playbackHeadPosition ?: 0).put("active_ms",clock.now()))
    }}
    private fun playable():Boolean=synchronized(gate){
        while(reason.get()<0&&(background||!surfaceReady||focusSuspended))gate.wait()
        if(reason.get()>=0)return false
        activeTrack?.let{if(!trackPlaying){it.play();trackPlaying=true}}
        true
    }
    private fun activeNow():Long=synchronized(gate){clock.now()}
    private val surface=SurfaceView(activity).apply{keepScreenOn=true}
    private val audio=activity.getSystemService(Activity.AUDIO_SERVICE) as AudioManager
    private val focus=AudioManager.OnAudioFocusChangeListener {change->synchronized(gate){
        if(!closed){
            when(change){
                AudioManager.AUDIOFOCUS_GAIN->{focusSuspended=false;volume=1f;focused=!background}
                AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK->volume=.2f
                AudioManager.AUDIOFOCUS_LOSS_TRANSIENT,AudioManager.AUDIOFOCUS_LOSS->{focusSuspended=true;if(change==AudioManager.AUDIOFOCUS_LOSS)focused=false}
            }
            activeTrack?.setVolume(volume);updateGate()
        }
    }}
    private fun packet(bytes:ByteArray):Pair<JSONObject,Int>{
        val split=bytes.indexOf(10.toByte());check(split>=0)
        val meta=JSONObject(String(bytes,0,split,Charsets.UTF_8));check(!meta.has("error")){meta.optString("error")}
        return meta to split+1
    }
    fun start(uri:Uri,token:Long,leftSkip:Boolean,complete:(Int)->Unit,failure:(Throwable)->Unit){
        check(!closed&&!launched)
        val events=File(root,"movie-$token-events.jsonl")
        fun log(event:String,meta:JSONObject=JSONObject()) {synchronized(gate){events.appendText(meta.put("event",event).put("time_ms",SystemClock.elapsedRealtime()).toString()+"\n")}}
        eventLog={event,meta->log(event,meta)}
        resume()
        surface.setOnClickListener{if(leftSkip)synchronized(gate){reason.compareAndSet(-1,1);gate.notifyAll()}}
        surface.holder.addCallback(object:SurfaceHolder.Callback{
            override fun surfaceChanged(holder:SurfaceHolder,format:Int,width:Int,height:Int){}
            override fun surfaceDestroyed(holder:SurfaceHolder){synchronized(drawLock){synchronized(gate){surfaceReady=false;updateGate();if(reason.get()<0)log("surface_destroyed")}}}
            override fun surfaceCreated(holder:SurfaceHolder){
                synchronized(gate){surfaceReady=true;updateGate();if(reason.get()<0)log("surface_created")}
                if(launched||closed)return
                launched=true
                worker.execute {
                    var track:AudioTrack?=null
                    var renderer:Thread?=null
                    var renderInputEnded=false;var renderDone=false
                    val renderFailure=java.util.concurrent.atomic.AtomicReference<Throwable>()
                    val presented=java.util.concurrent.atomic.AtomicLong()
                    val dropped=java.util.concurrent.atomic.AtomicLong()
                    val renderMillis=java.util.concurrent.atomic.AtomicLong()
                    val maxVideoLateness=java.util.concurrent.atomic.AtomicLong()
                    val mediaPositionMillis=java.util.concurrent.atomic.AtomicLong()
                    val presentedVideoMillis=java.util.concurrent.atomic.AtomicLong(-1)
                    val avDeltaMillis=java.util.concurrent.atomic.AtomicLong()
                    val maxAbsAvDeltaMillis=java.util.concurrent.atomic.AtomicLong()
                    var error:Throwable?=null
                    var frames=0L;var samples=0L;var peak=0L;var head=0L
                    var lastVideoTime=-1.0
                    var decodeMillis=0L;var digestMillis=0L
                    val pcmDigest=if(diagnostics)java.security.MessageDigest.getInstance("SHA-256")else null
                    var pcmNonzeroSamples=0L;var pcmPeakAmplitude=0
                    val started=SystemClock.elapsedRealtime();val activeStarted=activeNow()
                    try {
                        val digest=java.security.MessageDigest.getInstance("SHA-256")
                        var length=0L
                        activity.contentResolver.openInputStream(uri)!!.use{input->val buffer=ByteArray(65536);while(playable()){val n=input.read(buffer);if(n<0)break;length+=n;check(length<=512L*1024*1024);digest.update(buffer,0,n);if(reason.get()>=0)break}}
                        if(reason.get()<0){
                            val info=activity.contentResolver.openFileDescriptor(uri,"r")!!.use{packet(nativeMovie(it.fd,0)).first}
                            info.put("source_sha256",digest.digest().joinToString(""){"%02x".format(it.toInt() and 255)}).put("source_bytes",length).put("left_skip",leftSkip)
                            File(root,"movie-$token-info.json").writeText(info.toString())
                            val width=info.getInt("width");val height=info.getInt("height");val rate=info.getInt("sample_rate")
                            // A due frame remains useful. Drop only after a visible backlog
                            // exceeds 100 ms or two nominal frame intervals, whichever is larger.
                            val lateFrameDropMillis=maxOf(100L,kotlin.math.ceil(2000.0/info.getDouble("fps")).toLong())
                            info.put("late_frame_drop_ms",lateFrameDropMillis)
                            val minimum=AudioTrack.getMinBufferSize(rate,AudioFormat.CHANNEL_OUT_STEREO,AudioFormat.ENCODING_PCM_16BIT);check(minimum>0)
                            track=AudioTrack.Builder().setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build())
                                .setAudioFormat(AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(rate).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build())
                                .setTransferMode(AudioTrack.MODE_STREAM).setBufferSizeInBytes(maxOf(minimum,rate*4/10)).build()
                            check(track.state==AudioTrack.STATE_INITIALIZED)
                            synchronized(gate){activeTrack=track;track.setVolume(volume)}
                            if(playable())log("started",info)
                            // The decoder may emit independent audio and video blocks. Keep the
                            // unwritten PCM while presenting video against its own timestamp.
                            data class PendingPcm(val bytes:ByteArray,var offset:Int=0)
                            val pending=java.util.ArrayDeque<PendingPcm>()
                            var pendingBytes=0L
                            val playbackStarted=activeNow()
                            fun checkDeadline(){check(activeNow()-activeStarted<(info.getDouble("duration")*4+60)*1000){"movie playback deadline exceeded"}}
                            var finalAudioSamples=-1L
                            var audioTailStarted=-1L
                            // AudioTrack's consumed sample frames are the master, including
                            // underruns. Only the real audio EOF permits an active-clock tail.
                            fun mediaNow():Long=synchronized(gate){
                                val position=track.playbackHeadPosition.toLong() and 0xffffffffL
                                val time=if(finalAudioSamples>=0&&position>=finalAudioSamples){
                                    if(audioTailStarted<0)audioTailStarted=clock.now()
                                    finalAudioSamples*1000/rate+clock.now()-audioTailStarted
                                }else position*1000/rate
                                mediaPositionMillis.set(time)
                                time
                            }
                            fun pumpAudio(){
                                while(pending.isNotEmpty()){
                                    val block=pending.first
                                    val n=synchronized(gate){if(reason.get()>=0||background||!surfaceReady||focusSuspended)0 else track.write(block.bytes,block.offset,block.bytes.size-block.offset,AudioTrack.WRITE_NON_BLOCKING)}
                                    check(n>=0&&n%4==0){"movie PCM write failed: $n"}
                                    if(n==0)break
                                    block.offset+=n;pendingBytes-=n;samples+=n/4
                                    if(block.offset==block.bytes.size)pending.removeFirst()
                                }
                                head=track.playbackHeadPosition.toLong() and 0xffffffffL
                            }
                            data class VideoFrame(val rgba:ByteArray,val time:Double)
                            val videoQueue=java.util.ArrayDeque<VideoFrame>()
                            renderer=Thread({
                                var ownedBitmap:Bitmap?=null
                                try {
                                    val bitmap=Bitmap.createBitmap(width,height,Bitmap.Config.ARGB_8888);ownedBitmap=bitmap
                                    val pixels=ByteBuffer.allocateDirect(width*height*4)
                                    while(playable()){
                                        var frame=synchronized(gate){
                                            while(reason.get()<0&&videoQueue.isEmpty()&&!renderInputEnded)gate.wait()
                                            if(reason.get()>=0||videoQueue.isEmpty())null else videoQueue.removeFirst().also{gate.notifyAll()}
                                        } ?: break
                                        while(playable()){
                                            val now=mediaNow()
                                            synchronized(gate){
                                                while(videoQueue.isNotEmpty()&&now-(frame.time*1000).toLong()>lateFrameDropMillis&&(videoQueue.first.time*1000).toLong()<=now){
                                                    frame=videoQueue.removeFirst();dropped.incrementAndGet();gate.notifyAll()
                                                }
                                            }
                                            val remaining=(frame.time*1000).toLong()-mediaNow()
                                            if(remaining<=0)break
                                            synchronized(gate){if(reason.get()<0)gate.wait(minOf(remaining,5L))}
                                        }
                                        if(!playable())break
                                        val copyStart=SystemClock.elapsedRealtime()
                                        pixels.clear();pixels.put(frame.rgba);pixels.flip();bitmap.copyPixelsFromBuffer(pixels)
                                        renderMillis.addAndGet(SystemClock.elapsedRealtime()-copyStart)
                                        var drawn=false
                                        while(!drawn&&playable()){synchronized(drawLock){
                                            if(synchronized(gate){reason.get()<0&&surfaceReady&&!background&&!focusSuspended}){
                                                val drawStart=SystemClock.elapsedRealtime()
                                                val canvas=if(android.os.Build.VERSION.SDK_INT>=26)holder.surface.lockHardwareCanvas() else holder.lockCanvas() ?: error("movie surface unavailable")
                                                try {canvas.drawColor(Color.BLACK);val scale=minOf(canvas.width.toFloat()/width,canvas.height.toFloat()/height);val w=(width*scale).toInt();val h=(height*scale).toInt();val x=(canvas.width-w)/2;val y=(canvas.height-h)/2;canvas.drawBitmap(bitmap,null,Rect(x,y,x+w,y+h),null)}finally{if(android.os.Build.VERSION.SDK_INT>=26)holder.surface.unlockCanvasAndPost(canvas) else holder.unlockCanvasAndPost(canvas)}
                                                renderMillis.addAndGet(SystemClock.elapsedRealtime()-drawStart)
                                                presented.incrementAndGet();drawn=true
                                                val target=(frame.time*1000).toLong()
                                                val delta=target-mediaNow()
                                                presentedVideoMillis.set(target);avDeltaMillis.set(delta)
                                                maxAbsAvDeltaMillis.set(maxOf(maxAbsAvDeltaMillis.get(),kotlin.math.abs(delta)))
                                                maxVideoLateness.set(maxOf(maxVideoLateness.get(),-delta))
                                            }
                                        }}
                                    }
                                }catch(error:Throwable){renderFailure.set(error);synchronized(gate){reason.compareAndSet(-1,4);gate.notifyAll()}}
                                finally {ownedBitmap?.recycle();synchronized(gate){renderDone=true;gate.notifyAll()}}
                            },"movie-render-$token").also{it.start()}
                            while(playable()){
                                checkDeadline()
                                val decodeStart=SystemClock.elapsedRealtime()
                                val bytes=nativeMovie(-1,1);decodeMillis+=SystemClock.elapsedRealtime()-decodeStart;val (meta,offset)=packet(bytes)
                                val pcm=meta.getInt("pcm_bytes");val rgba=meta.getInt("rgba_bytes");check(pcm>=0&&rgba>=0&&bytes.size==offset+pcm+rgba&&pcm%4==0)
                                if(pcm>0){
                                    if(diagnostics){
                                        val digestStart=SystemClock.elapsedRealtime()
                                        pcmDigest!!.update(bytes,offset,pcm)
                                        var index=offset
                                        while(index<offset+pcm){
                                            val sample=((bytes[index].toInt() and 255) or (bytes[index+1].toInt() shl 8)).toShort().toInt()
                                            if(sample!=0)pcmNonzeroSamples++
                                            pcmPeakAmplitude=maxOf(pcmPeakAmplitude,kotlin.math.abs(sample));index+=2
                                        }
                                        digestMillis+=SystemClock.elapsedRealtime()-digestStart
                                    }
                                    pending.add(PendingPcm(bytes.copyOfRange(offset,offset+pcm)));pendingBytes+=pcm
                                }
                                pumpAudio()
                                if(rgba>0&&playable()){
                                    check(rgba==width*height*4)
                                    val videoTime=meta.getDouble("video_time")
                                    check(videoTime.isFinite()&&videoTime>=0&&videoTime>=lastVideoTime){"invalid movie video timestamp: $videoTime after $lastVideoTime"}
                                    lastVideoTime=videoTime
                                    if(diagnostics&&(frames==0L||frames==24L||frames==240L)){
                                        val digestStart=SystemClock.elapsedRealtime()
                                        val hash=java.security.MessageDigest.getInstance("SHA-256").apply{update(bytes,offset+pcm,rgba)}.digest().joinToString(""){"%02x".format(it.toInt() and 255)}
                                        digestMillis+=SystemClock.elapsedRealtime()-digestStart
                                        log("frame_digest",JSONObject().put("decoded_frame",frames).put("video_time",videoTime).put("rgba_sha256",hash))
                                    }
                                    val frame=VideoFrame(bytes.copyOfRange(offset+pcm,bytes.size),videoTime)
                                    var queued=false
                                    while(!queued&&playable()){
                                        checkDeadline();pumpAudio()
                                        synchronized(gate){
                                            if(videoQueue.size>=3&&mediaNow()-(videoQueue.first.time*1000).toLong()>lateFrameDropMillis){videoQueue.removeFirst();dropped.incrementAndGet()}
                                            if(videoQueue.size<3){videoQueue.addLast(frame);queued=true;gate.notifyAll()}else if(reason.get()<0)gate.wait(5)
                                        }
                                    }
                                    if(!queued)break
                                    frames++
                                    if(frames%120L==0L)log("progress",JSONObject().put("frames",frames).put("decoded_frames",frames).put("presented_frames",presented.get()).put("dropped_frames",dropped.get()).put("samples",samples).put("head",head).put("video_time",videoTime).put("decode_ms",decodeMillis).put("render_ms",renderMillis.get()).put("digest_ms",digestMillis).put("pending_pcm_bytes",pendingBytes).put("active_ms",activeNow()-playbackStarted).put("media_ms",mediaNow()).put("presented_video_time",presentedVideoMillis.get()/1000.0).put("av_delta_ms",avDeltaMillis.get()).put("max_abs_av_delta_ms",maxAbsAvDeltaMillis.get()))
                                }
                                peak=maxOf(peak,meta.getLong("peak_bytes"))
                                // Bound host-side queued PCM even when demuxing audio-only blocks.
                                while(pendingBytes>rate*4L*2&&playable()){checkDeadline();pumpAudio();if(pendingBytes>rate*4L*2)Thread.sleep(5)}
                                if(meta.getBoolean("ended")&&reason.get()<0){
                                    val audioEnd=meta.getDouble("audio_end_time");val videoEnd=meta.getDouble("video_end_time")
                                    val queuedSamples=samples+pendingBytes/4
                                    check(audioEnd.isFinite()&&videoEnd.isFinite()&&audioEnd>0&&videoEnd>=lastVideoTime&&frames>0){"invalid movie end timestamps"}
                                    check(kotlin.math.abs(audioEnd*rate-queuedSamples)<=1.0){"movie PCM count disagrees with decoder end time"}
                                    if(info.getString("container")=="mpeg"){
                                        check(queuedSamples.toDouble()/rate>=info.getDouble("duration")-.2 && frames>=kotlin.math.floor(info.getDouble("duration")*info.getDouble("fps")).toLong()){"movie ended before declared media duration"}
                                    }
                                    log("decoder_ended",JSONObject().put("frames",frames).put("decoded_frames",frames).put("presented_frames",presented.get()).put("dropped_frames",dropped.get()).put("queued_samples",queuedSamples).put("video_time",lastVideoTime).put("audio_end_time",audioEnd).put("video_end_time",videoEnd).put("head",head))
                                    synchronized(gate){finalAudioSamples=queuedSamples;renderInputEnded=true;gate.notifyAll()}
                                    val mediaEnd=(maxOf(audioEnd,videoEnd)*1000).toLong()
                                    val drain=activeNow()
                                    val drainDeadline=maxOf(pendingBytes*1000/(rate*4L),mediaEnd-mediaNow())+5000
                                    while((pending.isNotEmpty()||head<samples||!synchronized(gate){renderDone}||mediaNow()<mediaEnd)&&playable()){
                                        check(activeNow()-drain<drainDeadline){"movie audio/video drain timeout"}
                                        pumpAudio();Thread.sleep(5)
                                    }
                                    renderFailure.get()?.let{throw it}
                                    if(reason.get()<0){
                                        check(frames==presented.get()+dropped.get()){"movie render queue did not drain"}
                                        reason.compareAndSet(-1,0)
                                    }
                                    break
                                }
                            }
                        }
                        renderFailure.get()?.let{throw it}
                    }catch(e:Throwable){error=e}
                    finally {
                        synchronized(gate){if(error!=null)reason.compareAndSet(-1,4);renderInputEnded=true;gate.notifyAll()}
                        renderer?.join()
                        if(error==null)error=renderFailure.get()
                        synchronized(gate){activeTrack=null;trackPlaying=false;track?.release()}
                        try {val nativeClose=packet(nativeMovie(-1,2)).first;check(nativeClose.getBoolean("closed")&&nativeClose.getLong("decoder_live_bytes")==0L)}catch(e:Throwable){if(error==null)error=e else error.addSuppressed(e)}
                        audio.abandonAudioFocus(focus)
                        log("closed",JSONObject().put("reason",reason.get()).put("frames",frames).put("decoded_frames",frames).put("presented_frames",presented.get()).put("dropped_frames",dropped.get()).put("samples",samples).put("head",head).put("peak_bytes",peak).put("decode_ms",decodeMillis).put("render_ms",renderMillis.get()).put("digest_ms",digestMillis).put("hashes_enabled",diagnostics).put("pcm_sha256",pcmDigest?.digest()?.joinToString(""){"%02x".format(it.toInt() and 255)} ?: JSONObject.NULL).put("pcm_nonzero_samples",if(diagnostics)pcmNonzeroSamples else JSONObject.NULL).put("pcm_peak_amplitude",if(diagnostics)pcmPeakAmplitude else JSONObject.NULL).put("rgba_hash_scope","decoded_frames_0_24_240").put("media_ms",mediaPositionMillis.get()).put("presented_video_time",presentedVideoMillis.get()/1000.0).put("av_delta_ms",avDeltaMillis.get()).put("max_abs_av_delta_ms",maxAbsAvDeltaMillis.get()).put("last_video_time",lastVideoTime).put("max_video_lateness_ms",maxVideoLateness.get()).put("elapsed_ms",SystemClock.elapsedRealtime()-started).put("error",error?.toString() ?: JSONObject.NULL))
                        worker.shutdown()
                    }
                    activity.runOnUiThread {
                        stage.removeView(surface)
                        if(!closed){closed=true;if(error!=null)failure(error)else if(reason.get() in 0..2)complete(reason.get())else failure(IllegalStateException("movie surface lost before completion"))}
                    }
                }
            }
        })
        stage.addView(surface,FrameLayout.LayoutParams(-1,-1))
    }
    fun close(){
        synchronized(gate){if(closed)return;closed=true;reason.compareAndSet(-1,3);if(trackPlaying){activeTrack!!.pause();trackPlaying=false};gate.notifyAll()}
        audio.abandonAudioFocus(focus)
        if(!launched){worker.shutdown();stage.removeView(surface)}
    }
}
