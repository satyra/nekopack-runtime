import android.media.AudioAttributes
import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import org.nekopack.probe.ChannelPlayer
import org.nekopack.probe.ShortClip
import java.io.File

/** Standalone Android framework probe; no VM or game-state fixture injection. */
fun main(args:Array<String>){
    Looper.prepareMainLooper()
    val clip=ShortClip.read(File(args[0]))!!
    val handler=Handler(Looper.getMainLooper());val result=JSONArray()
    fun runCase(index:Int){
        if(index==4){println(JSONObject().put("rate",clip.rate).put("channels",clip.channels).put("frames",clip.frames).put("cases",result));System.exit(0)}
        val player=ChannelPlayer(clip);val heads=JSONArray();var finished=false;var paused=false;var callbacks=0
        val row=JSONObject().put("case",listOf("complete","pause_resume","cancel_playing","cancel_prepared")[index])
        player.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_GAME).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
        player.setDataSource(args[0])
        fun next(){row.put("heads",heads).put("completion_callbacks",callbacks);result.put(row);handler.postDelayed({runCase(index+1)},50)}
        player.setOnCompletionListener{
            check(index<2);callbacks++;check(callbacks==1 && it.playbackHead==clip.frames.toLong())
            heads.put(it.playbackHead);finished=true;it.release();next()
        }
        player.setOnPreparedListener{
            check(index!=3);it.setVolume(.5f,.5f);it.start()
            fun sample(){
                if(finished)return
                val head=it.playbackHead!!;heads.put(head)
                if(index==2 && head>0){finished=true;it.release();handler.postDelayed({check(callbacks==0);next()},500);return}
                if(index==1 && !paused && head>0 && head<clip.frames/2){
                    paused=true;it.pause()
                    // Allow already-submitted device buffers to settle, then require a frozen head.
                    handler.postDelayed({val frozen=it.playbackHead!!
                        handler.postDelayed({check(it.playbackHead==frozen);check(callbacks==0);row.put("paused_head",frozen);it.start();sample()},200)
                    },80)
                }else handler.postDelayed({sample()},10)
            }
            sample()
        }
        player.prepareAsync()
        if(index==3){player.release();handler.postDelayed({check(callbacks==0);next()},500)}
    }
    handler.post{runCase(0)}
    handler.postDelayed({error("short audio probe timed out")},10000)
    Looper.loop()
}
