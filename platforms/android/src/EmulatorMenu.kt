package org.nekopack.probe

import android.app.Activity
import android.app.Dialog
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.view.WindowManager

internal class EmulatorMenu(activity: Activity, onClose: (() -> Unit)?) : Dialog(activity) {
    private val content = LinearLayout(activity).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(dp(16), dp(4), dp(16), dp(16))
    }
    private var selected: (() -> Unit)? = null
    private val shell = LinearLayout(activity)

    init {
        shell.apply {
            orientation = LinearLayout.VERTICAL
            background = GradientDrawable().apply {
                setColor(0xff201929.toInt())
                val r = dp(22).toFloat()
                cornerRadii = floatArrayOf(r, r, 0f, 0f, 0f, 0f, r, r)
            }
        }
        val header = LinearLayout(activity).apply {
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(18), dp(12), dp(14), dp(8))
        }
        header.addView(TextView(activity).apply {
            text = "模拟器菜单"
            textSize = 21f
            setTextColor(Color.WHITE)
            setTypeface(typeface, android.graphics.Typeface.BOLD)
        }, LinearLayout.LayoutParams(0, -2, 1f))
        header.addView(actionButton("关闭", true, false, false) {})
        shell.addView(header)
        shell.addView(ScrollView(activity).apply {
            clipToPadding = false
            addView(content)
        }, LinearLayout.LayoutParams(-1, 0, 1f))
        setContentView(shell)
        setCanceledOnTouchOutside(true)
        setOnDismissListener {
            onClose?.invoke()
            selected?.invoke()
        }
    }

    fun section(title: String) {
        content.addView(TextView(context).apply {
            text = title
            textSize = 13f
            setTextColor(0xffcdb4ef.toInt())
            setPadding(dp(4), dp(12), 0, dp(5))
        })
    }

    fun panel(view: android.view.View) {
        content.addView(view, LinearLayout.LayoutParams(-1, -2))
    }

    data class Action(val label: String, val enabled: Boolean, val run: () -> Unit)

    fun pair(first: Action, second: Action) {
        val row = LinearLayout(context)
        for ((index, action) in listOf(first, second).withIndex()) {
            row.addView(actionButton(action.label, action.enabled, false, false, action.run),
                LinearLayout.LayoutParams(0, dp(46), 1f).apply {
                    if (index == 0) marginEnd = dp(7)
                })
        }
        content.addView(row, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(6) })
    }

    fun item(label: String, enabled: Boolean = true, keepOpen: Boolean = false, primary: Boolean = false, action: () -> Unit) {
        content.addView(actionButton(label, enabled, keepOpen, primary, action),
            LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(5) })
    }

    private fun actionButton(label: String, enabled: Boolean, keepOpen: Boolean, primary: Boolean, action: () -> Unit) =
        Button(context).apply {
            text = label
            isAllCaps = false
            isEnabled = enabled
            gravity = Gravity.CENTER_VERTICAL or Gravity.START
            minimumHeight = dp(46)
            minHeight = dp(46)
            setPadding(dp(12), dp(4), dp(12), dp(4))
            setTextColor(when {
                !enabled -> 0xff9b91a8.toInt()
                primary -> 0xff251735.toInt()
                else -> 0xfff4effa.toInt()
            })
            background = android.graphics.drawable.RippleDrawable(
                android.content.res.ColorStateList.valueOf(0x33ffffff),
                GradientDrawable().apply {
                    cornerRadius = dp(12).toFloat()
                    setColor(when {
                        !enabled -> 0xff2a2134.toInt()
                        primary -> 0xffd5baff.toInt()
                        else -> 0xff342743.toInt()
                    })
                }, null
            )
            setOnClickListener {
                if (keepOpen) action()
                else {
                    selected = action
                    dismiss()
                }
            }
        }

    override fun show() {
        val window = requireNotNull(window)
        window.setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
        window.addFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND)
        window.addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN)
        window.setDimAmount(0.45f)
        window.setGravity(Gravity.RIGHT or Gravity.TOP)
        window.setWindowAnimations(0)
        super.show()
        window.setLayout(minOf(dp(340), context.resources.displayMetrics.widthPixels), -1)
        shell.translationX = dp(340).toFloat()
        shell.animate().translationX(0f).setDuration(180).start()
    }

    private fun dp(value: Int) = (value * context.resources.displayMetrics.density + 0.5f).toInt()
}
