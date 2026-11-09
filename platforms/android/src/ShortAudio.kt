package org.nekopack.probe

import android.media.*
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import java.io.ByteArrayOutputStream
import java.io.File

/** Bounded, non-looping clips only. Decode on the story worker, never the UI thread. */
data class ShortClip(val pcm:ByteArray,val rate:Int,val channels:Int) {
    val frames get()=pcm.size/(channels*2)
    companion object {
        fun read(file:File):ShortClip? {
            val extractor=MediaExtractor()
            try {
                extractor.setDataSource(file.absolutePath)
                check(extractor.trackCount==1){"short audio must contain one track"}
                val input=extractor.getTrackFormat(0)
                val duration=input.getLong(MediaFormat.KEY_DURATION)
                if(duration>1_000_000)return null
                check(duration>0){"invalid audio duration"}
                extractor.selectTrack(0)
                val codec=MediaCodec.createDecoderByType(input.getString(MediaFormat.KEY_MIME)!!)
                try {
                    codec.configure(input,null,null,0);codec.start()
                    val bytes=ByteArrayOutputStream();val info=MediaCodec.BufferInfo()
                    var inputEnded=false;var rate=0;var channels=0
                    val deadline=SystemClock.elapsedRealtime()+5000
                    while(true) {
                        check(SystemClock.elapsedRealtime()<deadline){"short audio decode timed out"}
                        if(!inputEnded) {
                            val index=codec.dequeueInputBuffer(0)
                            if(index>=0) {
                                val buffer=codec.getInputBuffer(index)!!;buffer.clear()
                                val size=extractor.readSampleData(buffer,0)
                                if(size<0){codec.queueInputBuffer(index,0,0,0,MediaCodec.BUFFER_FLAG_END_OF_STREAM);inputEnded=true}
                                else {codec.queueInputBuffer(index,0,size,extractor.sampleTime,0);extractor.advance()}
                            }
                        }
                        val index=codec.dequeueOutputBuffer(info,10_000)
                        if(index==MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                            val output=codec.outputFormat
                            rate=output.getInteger(MediaFormat.KEY_SAMPLE_RATE);channels=output.getInteger(MediaFormat.KEY_CHANNEL_COUNT)
                            check(rate in 8000..192000 && channels in 1..2){"unsupported short audio format"}
                            check(!output.containsKey("pcm-encoding") || output.getInteger("pcm-encoding")==AudioFormat.ENCODING_PCM_16BIT){"short audio requires PCM16"}
                        } else if(index>=0) {
                            try {
                                check(info.size<=1_048_576-bytes.size()){ "short audio PCM budget exceeded" }
                                if(info.size>0) {
                                    val buffer=codec.getOutputBuffer(index)!!
                                    buffer.position(info.offset);buffer.limit(info.offset+info.size)
                                    val chunk=ByteArray(info.size);buffer.get(chunk);bytes.write(chunk)
                                }
                            } finally {codec.releaseOutputBuffer(index,false)}
                            if(info.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM!=0)break
                        }
                    }
                    check(rate>0 && channels>0 && bytes.size()>0 && bytes.size()%(channels*2)==0){"invalid decoded PCM"}
                    return ShortClip(bytes.toByteArray(),rate,channels)
                } finally {codec.release()}
            } finally {extractor.release()}
        }
    }
}

/** Same channel lifecycle for streaming MediaPlayer and bounded static PCM playback. */
class ChannelPlayer(private val clip:ShortClip?=null) {
    private val media=if(clip==null)MediaPlayer() else null
    private var track:AudioTrack?=null
    private val handler=Handler(Looper.getMainLooper())
    private var attributes:AudioAttributes?=null
    private var generation=0L
    private var released=false
    private var completed=false
    private var onPrepared:(ChannelPlayer)->Unit={}
    private var onComplete:(ChannelPlayer)->Unit={}
    val pcmFrames get()=clip?.frames
    val pcmRate get()=clip?.rate
    val playbackHead get()=track?.playbackHeadPosition?.toLong()?.and(0xffffffffL)
    val duration get()=media?.duration ?: (clip!!.frames*1000L/clip.rate).toInt()
    val currentPosition get()=media?.currentPosition ?: (playbackHead!!*1000/clip!!.rate).toInt()
    val isPlaying get()=media?.isPlaying ?: (!completed && track!!.playState==AudioTrack.PLAYSTATE_PLAYING)
    var isLooping:Boolean
        get()=media?.isLooping ?: false
        set(value){if(media!=null)media.isLooping=value else check(!value){"static short audio cannot loop"}}
    fun setAudioAttributes(value:AudioAttributes){attributes=value;media?.setAudioAttributes(value)}
    fun setDataSource(path:String){media?.setDataSource(path)}
    fun setOnPreparedListener(value:(ChannelPlayer)->Unit){onPrepared=value;media?.setOnPreparedListener{value(this)}}
    fun setOnCompletionListener(value:(ChannelPlayer)->Unit){onComplete=value;media?.setOnCompletionListener{value(this)}}
    fun setOnErrorListener(value:(ChannelPlayer,Int,Int)->Boolean){media?.setOnErrorListener{_,what,extra->value(this,what,extra)}}
    fun prepareAsync(){
        if(media!=null){media.prepareAsync();return}
        val pcm=clip!!
        val output=AudioTrack.Builder().setAudioAttributes(attributes!!)
            .setAudioFormat(AudioFormat.Builder().setSampleRate(pcm.rate).setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                .setChannelMask(if(pcm.channels==1)AudioFormat.CHANNEL_OUT_MONO else AudioFormat.CHANNEL_OUT_STEREO).build())
            .setTransferMode(AudioTrack.MODE_STATIC).setBufferSizeInBytes(pcm.pcm.size).build()
        track=output
        check(output.write(pcm.pcm,0,pcm.pcm.size)==pcm.pcm.size){"incomplete static PCM write"}
        handler.post{if(!released)onPrepared(this)}
    }
    fun start(){
        if(media!=null){media.start();return}
        track!!.play();val epoch=++generation
        fun poll(){
            if(released || generation!=epoch)return
            if(playbackHead!!>=clip!!.frames){completed=true;onComplete(this)}
            else handler.postDelayed({poll()},20)
        }
        handler.postDelayed({poll()},20)
    }
    fun pause(){if(media!=null)media.pause() else {generation++;track!!.pause()}}
    fun setVolume(left:Float,right:Float){if(media!=null)media.setVolume(left,right) else check(track!!.setStereoVolume(left,right)==AudioTrack.SUCCESS)}
    fun release(){released=true;generation++;media?.release();track?.release();track=null}
}
