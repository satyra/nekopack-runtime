package org.nekopack.probe

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.view.MotionEvent
import android.view.View

/** Device coordinates only; menu state, hit testing and rendering belong to Rust. */
class TitleView(context:Context,private val pointer:(Int,Int,Int)->Unit):View(context){
    private var bitmap:Bitmap?=null
    private val paint=Paint(Paint.FILTER_BITMAP_FLAG)
    private val target=RectF()
    private var tracking=false
    var accepting=true
    init { contentDescription="游戏标题菜单"; isClickable=true }
    fun present(next:Bitmap){val previous=bitmap;bitmap=next;previous?.recycle();invalidate()}
    private fun bounds(){
        val image=bitmap ?: return
        val scale=minOf(width.toFloat()/image.width,height.toFloat()/image.height)
        val w=image.width*scale;val h=image.height*scale
        target.set((width-w)/2,(height-h)/2,(width+w)/2,(height+h)/2)
    }
    override fun onDraw(canvas:Canvas){super.onDraw(canvas);canvas.drawColor(android.graphics.Color.BLACK);bounds();bitmap?.let{canvas.drawBitmap(it,null,target,paint)}}
    override fun onTouchEvent(event:MotionEvent):Boolean{
        val image=bitmap ?: return false
        if(!accepting)return true
        bounds()
        val x=kotlin.math.floor((event.x-target.left)*image.width/target.width()).toInt()
        val y=kotlin.math.floor((event.y-target.top)*image.height/target.height()).toInt()
        when(event.actionMasked){
            MotionEvent.ACTION_DOWN->{tracking=true;pointer(0,x,y)}
            MotionEvent.ACTION_MOVE->if(tracking)pointer(1,x,y)
            MotionEvent.ACTION_UP->if(tracking){tracking=false;accepting=false;pointer(2,x,y);performClick()}
            MotionEvent.ACTION_CANCEL,MotionEvent.ACTION_POINTER_DOWN->cancel()
        }
        return true
    }
    fun cancel(){if(tracking){tracking=false;pointer(3,-1,-1)}}
    override fun onWindowFocusChanged(hasWindowFocus:Boolean){super.onWindowFocusChanged(hasWindowFocus);if(!hasWindowFocus)cancel()}
    override fun performClick():Boolean{super.performClick();return true}
    override fun onDetachedFromWindow(){bitmap?.recycle();bitmap=null;super.onDetachedFromWindow()}
}
