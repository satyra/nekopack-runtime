package org.nekopack.probe

import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.view.InputDevice
import android.view.MotionEvent
import kotlin.math.abs

internal enum class StoryAction(val label: String) {
    NONE("未绑定"), NEXT("继续对白"), MENU("剧情菜单"), HIDE("隐藏对白"),
    AUTO("自动播放"), SKIP("快进"), LOG("对白记录"), CONFIG("原版设置")
}

internal enum class StoryInput(val label: String) {
    PRIMARY("鼠标左键"), SECONDARY("鼠标右键"), MIDDLE("鼠标中键"),
    BACK("鼠标后退键"), FORWARD("鼠标前进键"),
    LEFT("向左滑"), RIGHT("向右滑"), UP("向上滑"), DOWN("向下滑")
}

internal class InputBindings(context: Context, game: String? = null) {
    private val prefs = context.getSharedPreferences("story_input", Context.MODE_PRIVATE)
    private val prefix = game?.let { "$it/" }.orEmpty()

    fun overrideAction(input: StoryInput): StoryAction? =
        prefs.getString(prefix + input.name, prefs.getString(input.name, null))?.let(StoryAction::valueOf)

    fun action(input: StoryInput) = overrideAction(input) ?: StoryAction.NONE

    fun show(activity: Activity) {
        val inputs = StoryInput.values()
        AlertDialog.Builder(activity).setTitle("鼠标与手势")
            .setItems(inputs.map { "${it.label} · ${overrideAction(it)?.label ?: if(it==StoryInput.SECONDARY)"原版设置" else StoryAction.NONE.label}" }.toTypedArray()) { _, index ->
                val input = inputs[index]
                val actions = StoryAction.values()
                AlertDialog.Builder(activity).setTitle(input.label)
                    .setSingleChoiceItems(actions.map { it.label }.toTypedArray(), action(input).ordinal) { dialog, selected ->
                        prefs.edit().putString(prefix + input.name, actions[selected].name).apply()
                        dialog.dismiss()
                        show(activity)
                    }.setNegativeButton("取消", null)
                    .setNeutralButton("恢复默认") { _, _ ->
                        prefs.edit().remove(prefix + input.name).apply()
                        show(activity)
                    }.show()
            }.setNegativeButton("关闭", null).show()
    }
}

internal class StoryPointer(private val threshold: Float) {
    private var tracking = false
    private var x = 0f
    private var y = 0f
    private var primary = StoryAction.NONE

    fun touch(event: MotionEvent, active: Boolean, bindings: InputBindings, deliver: (StoryAction) -> Unit): Boolean {
        val mouse = event.isFromSource(InputDevice.SOURCE_MOUSE)
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                tracking = active && !mouse && event.isFromSource(InputDevice.SOURCE_TOUCHSCREEN)
                x = event.x
                y = event.y
                primary = if (active && mouse && event.buttonState and MotionEvent.BUTTON_PRIMARY != 0)
                    bindings.action(StoryInput.PRIMARY) else StoryAction.NONE
                return primary != StoryAction.NONE
            }
            MotionEvent.ACTION_POINTER_DOWN -> tracking = false
            MotionEvent.ACTION_CANCEL -> {
                tracking = false
                val consumed = primary != StoryAction.NONE
                primary = StoryAction.NONE
                return consumed
            }
            MotionEvent.ACTION_UP -> {
                val action = primary
                primary = StoryAction.NONE
                if (action != StoryAction.NONE) {
                    if (active) deliver(action)
                    return true
                }
                val swiping = tracking
                tracking = false
                if (!swiping || !active) return false
                val dx = event.x - x
                val dy = event.y - y
                val ax = abs(dx)
                val ay = abs(dy)
                val input = when {
                    ax >= threshold && ax > ay * 1.4f -> if (dx > 0) StoryInput.RIGHT else StoryInput.LEFT
                    ay >= threshold && ay > ax * 1.4f -> if (dy > 0) StoryInput.DOWN else StoryInput.UP
                    else -> return false
                }
                val binding = bindings.action(input)
                if (binding == StoryAction.NONE) return false
                deliver(binding)
                return true
            }
        }
        return primary != StoryAction.NONE
    }

    fun mouse(event: MotionEvent, active: Boolean, bindings: InputBindings, secondary: StoryAction, deliver: (StoryAction) -> Unit): Boolean {
        if (!active || !event.isFromSource(InputDevice.SOURCE_MOUSE)) return false
        if (event.actionMasked != MotionEvent.ACTION_BUTTON_PRESS &&
            event.actionMasked != MotionEvent.ACTION_BUTTON_RELEASE) return false
        val input = when (event.actionButton) {
            MotionEvent.BUTTON_SECONDARY -> StoryInput.SECONDARY
            MotionEvent.BUTTON_TERTIARY -> StoryInput.MIDDLE
            MotionEvent.BUTTON_BACK -> StoryInput.BACK
            MotionEvent.BUTTON_FORWARD -> StoryInput.FORWARD
            else -> return false
        }
        val action = bindings.overrideAction(input) ?: if (input == StoryInput.SECONDARY) secondary else StoryAction.NONE
        if (action == StoryAction.NONE) return input == StoryInput.SECONDARY
        if (event.actionMasked == MotionEvent.ACTION_BUTTON_RELEASE) deliver(action)
        return true
    }
}
