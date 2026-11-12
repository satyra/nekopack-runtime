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
    var screenMode = 0
        set(value) { field = value; invalidate() }
    private val paint=Paint(Paint.FILTER_BITMAP_FLAG)
    fun show(image:Bitmap,layout:JSONObject,text:String){bitmap=image;geometry=layout;contentDescription=text;invalidate()}
    fun clear(){bitmap=null;geometry=null;contentDescription="";invalidate()}
    companion object {
        fun decodeRgba(packet:ByteArray,offset:Int,geometry:JSONObject):Bitmap {
            val bounds=geometry.getJSONArray("bounds")
            val width=bounds.getInt(2)
            val height=bounds.getInt(3)
            check(width>0 && height>0 && width.toLong()*height*4==packet.size.toLong()-offset){"invalid dialogue pixel data"}
            val rgba=java.nio.ByteBuffer.wrap(packet,offset,packet.size-offset).asIntBuffer()
            val argb=IntArray(width*height)
            rgba.get(argb)
            for(i in argb.indices)argb[i]=Integer.rotateRight(argb[i],8)
            return Bitmap.createBitmap(argb,width,height,Bitmap.Config.ARGB_8888)
        }
    }
    override fun onDraw(canvas:Canvas){
        super.onDraw(canvas)
        val image=bitmap ?: return;val layout=geometry!!;val size=layout.getJSONArray("size");val rect=layout.getJSONArray("bounds")
        val w=size.getDouble(0).toFloat();val h=size.getDouble(1).toFloat();val fit = minOf(width/w,height/h)
        val crop = maxOf(width/w,height/h)
        val scaleX = when(screenMode){1 -> width/w; 2 -> crop; else -> fit}
        val scaleY = when(screenMode){1 -> height/h; 2 -> crop; else -> fit}
        canvas.save();canvas.translate((width-w*scaleX)/2,(height-h*scaleY)/2);canvas.scale(scaleX,scaleY)
        val x=rect.getDouble(0).toFloat();val y=rect.getDouble(1).toFloat()
        canvas.drawBitmap(image,null,RectF(x,y,x+rect.getDouble(2).toFloat(),y+rect.getDouble(3).toFloat()),paint)
        canvas.restore()
    }
}
