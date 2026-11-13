package org.nekopack.probe

/** Original legacy MuteBGM envelope, independent of Android audio focus. */
class VoiceMusicGain {
    private var active=false
    private var start=0L
    private var from=1f
    private var restoring=false
    fun value(now:Long):Float = if(active).5f else if(restoring) {
        from+(1f-from)*((now-start).coerceIn(0,1000)/1000f)
    } else 1f
    fun setActive(next:Boolean,now:Long):Boolean {
        if(next==active)return false
        from=value(now);start=now;active=next;restoring=!next
        return true
    }
    fun pending(now:Long)=restoring && now-start<1000
}
