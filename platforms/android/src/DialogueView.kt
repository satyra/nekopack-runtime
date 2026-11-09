package org.nekopack.probe

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.view.View
import org.json.JSONObject

/** Maps a Rust-composited overlay into the same fit-center viewport as the scene. */
class DialogueView(context:Context):View(context) {
    private var bitmap:Bitmap?=null
    private var geometry:JSONObject?=null
    private val paint=Paint(Paint.FILTER_BITMAP_FLAG)
    fun show(image:Bitmap,layout:JSONObject,text:String){bitmap=image;geometry=layout;contentDescription=text;invalidate()}
    fun clear(){bitmap=null;geometry=null;contentDescription="";invalidate()}
    override fun onDraw(canvas:Canvas){
        super.onDraw(canvas)
        val image=bitmap ?: return;val layout=geometry!!;val size=layout.getJSONArray("size");val rect=layout.getJSONArray("bounds")
        val w=size.getDouble(0).toFloat();val h=size.getDouble(1).toFloat();val scale=minOf(width/w,height/h)
        canvas.save();canvas.translate((width-w*scale)/2,(height-h*scale)/2);canvas.scale(scale,scale)
        val x=rect.getDouble(0).toFloat();val y=rect.getDouble(1).toFloat()
        canvas.drawBitmap(image,null,RectF(x,y,x+rect.getDouble(2).toFloat(),y+rect.getDouble(3).toFloat()),paint)
        canvas.restore()
    }
}
