package org.nekopack.probe

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.view.MotionEvent
import android.view.View
import org.json.JSONObject

/** Touch adaptation: tap the original right-side region to reveal, then tap an item. */
class SideMenuView(context:Context,private val available:()->Boolean,private val reveal:()->Unit,private val pointer:(Int,Int,Int)->Unit):View(context){
    private var bitmap:Bitmap?=null
    private var geometry:JSONObject?=null
    private val paint=Paint(Paint.FILTER_BITMAP_FLAG)
    var expanded=false
        private set
    private var revealing=false
    private var tracking=false
    var accepting=true
    init {contentDescription="原版右侧悬浮菜单"}
    fun present(image:Bitmap,meta:JSONObject){bitmap?.recycle();bitmap=image;geometry=meta;accepting=true;invalidate()}
    fun collapse(){expanded=false;tracking=false;revealing=false;invalidate()}
    private fun target():RectF {
        val image=bitmap ?: return RectF()
        val scale=minOf(width.toFloat()/image.width,height.toFloat()/image.height)
        val w=image.width*scale;val h=image.height*scale
        return RectF((width-w)/2,(height-h)/2,(width+w)/2,(height+h)/2)
    }
    override fun onDraw(canvas:Canvas){super.onDraw(canvas);if(expanded&&available())bitmap?.let{canvas.drawBitmap(it,null,target(),paint)}}
    override fun onTouchEvent(event:MotionEvent):Boolean {
        val image=bitmap ?: return false
        if(!available()){collapse();return false}
        if(!accepting)return expanded
        val target=target()
        val x=kotlin.math.floor((event.x-target.left)*image.width/target.width()).toInt()
        val y=kotlin.math.floor((event.y-target.top)*image.height/target.height()).toInt()
        val b=geometry!!.getJSONArray("bounds")
        val inside=x>=b.getInt(0)&&y>=b.getInt(1)&&x<b.getInt(0)+b.getInt(2)&&y<b.getInt(1)+b.getInt(3)
        when(event.actionMasked){
            MotionEvent.ACTION_DOWN->{
                if(!expanded){if(!inside)return false;revealing=true;return true}
                if(!inside){collapse();return true}
                tracking=true;pointer(0,x,y)
            }
            MotionEvent.ACTION_MOVE->if(tracking)pointer(1,x,y)
            MotionEvent.ACTION_UP->{
                if(revealing){revealing=false;if(inside){expanded=true;accepting=false;reveal()}}
                else if(tracking){tracking=false;accepting=false;pointer(2,x,y)}
                performClick()
            }
            MotionEvent.ACTION_CANCEL,MotionEvent.ACTION_POINTER_DOWN->{if(tracking)pointer(3,-1,-1);collapse()}
        }
        return true
    }
    override fun onWindowFocusChanged(focused:Boolean){super.onWindowFocusChanged(focused);if(!focused){if(tracking)pointer(3,-1,-1);collapse()}}
    override fun performClick():Boolean{super.performClick();return true}
    override fun onDetachedFromWindow(){bitmap?.recycle();bitmap=null;super.onDetachedFromWindow()}
}
