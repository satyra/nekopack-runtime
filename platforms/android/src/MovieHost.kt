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

/** One worker owns the seekable SAF descriptor, native decoder, surface draws and PCM sink. */
class MovieHost(private val activity:Activity,private val stage:FrameLayout,private val root:File) {
    private external fun nativeMovie(fd:Int,operation:Int):ByteArray
    private val worker=Executors.newSingleThreadExecutor()
    private val reason=AtomicInteger(-1)
    private var launched=false
    @Volatile private var closed=false
    val isActive get() = !closed
    private val gate=Object()
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
            override fun surfaceDestroyed(holder:SurfaceHolder){synchronized(gate){surfaceReady=false;updateGate();if(reason.get()<0)log("surface_destroyed")}}
            override fun surfaceCreated(holder:SurfaceHolder){
                synchronized(gate){surfaceReady=true;updateGate();if(reason.get()<0)log("surface_created")}
                if(launched||closed)return
                launched=true
                worker.execute {
                    var track:AudioTrack?=null
                    var bitmap:Bitmap?=null
                    var error:Throwable?=null
                    var frames=0L;var samples=0L;var peak=0L;var head=0L
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
                            bitmap=Bitmap.createBitmap(width,height,Bitmap.Config.ARGB_8888)
                            val pixels=ByteBuffer.allocateDirect(width*height*4)
                            val minimum=AudioTrack.getMinBufferSize(rate,AudioFormat.CHANNEL_OUT_STEREO,AudioFormat.ENCODING_PCM_16BIT);check(minimum>0)
                            track=AudioTrack.Builder().setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build())
                                .setAudioFormat(AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(rate).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build())
                                .setTransferMode(AudioTrack.MODE_STREAM).setBufferSizeInBytes(maxOf(minimum,rate*4/10)).build()
                            check(track.state==AudioTrack.STATE_INITIALIZED)
                            synchronized(gate){activeTrack=track;track.setVolume(volume)}
                            if(playable())log("started",info)
                            val pcmSample=File(root,"movie-$token-sample.pcm").outputStream()
                            try {
                                while(playable()){
                                    check(activeNow()-activeStarted<(info.getDouble("duration")*4+60)*1000){"movie playback deadline exceeded"}
                                    val bytes=nativeMovie(-1,1);val (meta,offset)=packet(bytes)
                                    val pcm=meta.getInt("pcm_bytes");val rgba=meta.getInt("rgba_bytes");check(bytes.size==offset+pcm+rgba&&pcm%4==0)
                                    if(samples<rate*3){pcmSample.write(bytes,offset,minOf(pcm,((rate*3-samples)*4).toInt()))}
                                    var written=0
                                    while(written<pcm&&playable()){
                                        val n=synchronized(gate){if(background||!surfaceReady||focusSuspended)0 else track.write(bytes,offset+written,pcm-written,AudioTrack.WRITE_NON_BLOCKING)}
                                        check(n>=0){"movie PCM write failed: $n"};written+=n
                                        if(n==0)Thread.sleep(5)
                                    }
                                    samples+=written/4;head=track.playbackHeadPosition.toLong() and 0xffffffffL
                                    if(rgba>0&&playable()){
                                        check(rgba==width*height*4)
                                        pixels.clear();pixels.put(bytes,offset+pcm,rgba);pixels.flip();bitmap.copyPixelsFromBuffer(pixels)
                                        if(frames in listOf(0L,24L,240L))File(root,"movie-$token-frame-$frames.rgba").writeBytes(bytes.copyOfRange(offset+pcm,bytes.size))
                                        var drawn=false
                                        while(!drawn&&playable()){synchronized(gate){if(reason.get()<0&&surfaceReady&&!background&&!focusSuspended){
                                        val canvas=if(android.os.Build.VERSION.SDK_INT>=26)holder.surface.lockHardwareCanvas() else holder.lockCanvas() ?: error("movie surface unavailable")
                                        try {canvas.drawColor(Color.BLACK);val scale=minOf(canvas.width.toFloat()/width,canvas.height.toFloat()/height);val w=(width*scale).toInt();val h=(height*scale).toInt();val x=(canvas.width-w)/2;val y=(canvas.height-h)/2;canvas.drawBitmap(bitmap,null,Rect(x,y,x+w,y+h),null)}finally{if(android.os.Build.VERSION.SDK_INT>=26)holder.surface.unlockCanvasAndPost(canvas) else holder.unlockCanvasAndPost(canvas)}
                                        frames++;drawn=true
                                        }}}
                                    }
                                    peak=maxOf(peak,meta.getLong("peak_bytes"))
                                    if(frames%120L==0L)log("progress",JSONObject().put("frames",frames).put("samples",samples).put("head",head))
                                    if(meta.getBoolean("ended")&&reason.get()<0){
                                        check(samples.toDouble()/rate>=info.getDouble("duration")-.2 && frames>=kotlin.math.floor(info.getDouble("duration")*info.getDouble("fps")).toLong()){"movie ended before declared media duration"}
                                        val drain=activeNow()
                                        while(head<samples&&playable()){check(activeNow()-drain<5000){"movie audio drain timeout"};Thread.sleep(10);head=track.playbackHeadPosition.toLong() and 0xffffffffL}
                                        reason.compareAndSet(-1,0);break
                                    }
                                }
                            }finally{pcmSample.close()}
                        }
                    }catch(e:Throwable){error=e}
                    finally {
                        synchronized(gate){activeTrack=null;trackPlaying=false;track?.release()};bitmap?.recycle()
                        try {val nativeClose=packet(nativeMovie(-1,2)).first;check(nativeClose.getBoolean("closed")&&nativeClose.getLong("decoder_live_bytes")==0L)}catch(e:Throwable){if(error==null)error=e else error.addSuppressed(e)}
                        audio.abandonAudioFocus(focus)
                        log("closed",JSONObject().put("reason",reason.get()).put("frames",frames).put("samples",samples).put("head",head).put("peak_bytes",peak).put("elapsed_ms",SystemClock.elapsedRealtime()-started).put("error",error?.toString() ?: JSONObject.NULL))
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
