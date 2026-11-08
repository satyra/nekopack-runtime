package org.nekopack.probe

import android.content.Context
import android.media.AudioManager
import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import org.json.JSONArray
import java.io.File

/** Explicit probe mode only; interrupts real BGM through a separate OS focus client. */
fun probeAudioState(context:Context,effects:StoryEffects,root:File,failure:(Throwable)->Unit) {
    val handler=Handler(Looper.getMainLooper())
    val audio=context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    val other=AudioManager.OnAudioFocusChangeListener { }
    val evidence=JSONArray()
    fun sample(label:String):JSONObject=effects.audioSnapshot().also{it.put("label",label);evidence.put(it)}
    fun later(block:()->Unit){handler.postDelayed({try{block()}catch(error:Throwable){audio.abandonAudioFocus(other);failure(error)}},350)}
    try {
        val initial=sample("initial")
        check(initial.getBoolean("playing")&&initial.getDouble("gain")==.5)
        check(audio.requestAudioFocus(other,AudioManager.STREAM_MUSIC,AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED)
        later {
            val paused=sample("transient_loss")
            check(paused.getBoolean("suspended")&&!paused.getBoolean("playing"))
            later {
                val still=sample("paused_position")
                check(kotlin.math.abs(still.getInt("position")-paused.getInt("position"))<=30)
                val sourceToken=still.getLong("token")
                val copy=File(root,"audio-probe-replacement.ogg")
                File(root,"audio-$sourceToken.ogg").copyTo(copy)
                val replacement=JSONObject(File(root,"effect-$sourceToken.json").readText()).put("token",-3)
                var acknowledged=false
                effects.run(replacement,null,copy){acknowledged=true}
                later {
                    val prepared=sample("prepared_while_paused")
                    check(prepared.getLong("token")==-3L&&!prepared.getBoolean("playing")&&!acknowledged)
                    audio.abandonAudioFocus(other)
                    later {
                    val resumed=sample("regained")
                    check(!resumed.getBoolean("suspended")&&resumed.getBoolean("playing")&&resumed.getDouble("gain")==.5&&acknowledged)
                    fun fade(token:Long)=effects.run(JSONObject().put("host_effect","bgm_fade").put("token",token).put("duration_ms",2000),null,null){ }
                    fade(-1)
                    later {
                        val before=sample("first_fade")
                        check(before.getDouble("gain") in .01.. .49)
                        fade(-2)
                        val after=sample("repeat_fade_start")
                        check(kotlin.math.abs(after.getDouble("gain")-before.getDouble("gain"))<.001)
                        later {
                            val tail=sample("repeat_fade_progress")
                            check(tail.getDouble("gain")<after.getDouble("gain"))
                            File(root,"audio-state-probe.json").writeText(JSONObject().put("passed",true).put("samples",evidence).toString())
                        }
                    }
                    }
                }
            }
        }
    }catch(error:Throwable){audio.abandonAudioFocus(other);failure(error)}
}
