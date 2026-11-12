package org.nekopack.probe

import android.app.Activity
import android.graphics.BitmapFactory
import android.widget.FrameLayout
import org.json.JSONObject
import java.util.concurrent.Executor

internal class TitleRooms(
    private val activity: Activity,
    private val stage: FrameLayout,
    private val title: TitleView,
    private val worker: Executor,
    private val request: (Int, IntArray, Long) -> ByteArray,
    private val audio: (JSONObject) -> Unit,
    private val presented: (JSONObject) -> Unit,
    private val failed: (Throwable) -> Unit,
    private val replay: () -> Unit
) {
    private var active = false
    private var pending = false
    private var backQueued = false
    private val view = TitleView(activity) { kind, x, y ->
        if (active && !pending) {
            if (kind == 2) pending = true
            submit(60, intArrayOf(kind, x, y), 0, kind == 2)
        }
    }

    fun open(room: Int) {
        if (active) return
        active = true
        pending = true
        title.accepting = false
        view.accepting = false
        stage.addView(view, FrameLayout.LayoutParams(-1, -1))
        submit(59, intArrayOf(), room.toLong(), true)
    }

    fun back(): Boolean {
        if (!active) return false
        if (pending) backQueued = true
        else {
            view.cancel()
            view.accepting = false
            pending = true
            submit(60, intArrayOf(4, 0, 0), 0, true)
        }
        return true
    }

    private fun close() {
        active = false
        pending = false
        backQueued = false
        stage.removeView(view)
        title.accepting = true
    }

    private fun unlockPointer() {
        pending = false
        view.accepting = true
        if (backQueued) {
            backQueued = false
            back()
        }
    }

    private fun submit(op: Int, input: IntArray, id: Long, unlock: Boolean) {
        worker.execute {
            try {
                val packet = request(op, input, id)
                val split = packet.indexOf(10.toByte())
                val meta = JSONObject(String(packet, 0, if (split < 0) packet.size else split, Charsets.UTF_8))
                check(!meta.has("error")) { meta.getString("error") }
                audio(meta)
                val bitmap = if (split >= 0) BitmapFactory.decodeByteArray(packet, split + 1, packet.size - split - 1)
                    ?: error("invalid room PNG") else null
                activity.runOnUiThread {
                    if (!active || activity.isDestroyed || activity.isFinishing) {
                        bitmap?.recycle()
                        return@runOnUiThread
                    }
                    val kind = meta.getString("kind")
                    if (kind != "room_unchanged") presented(meta)
                    when (kind) {
                        "room_unchanged" -> if (unlock) unlockPointer()
                        "room_closed" -> close()
                        "room_replay" -> {
                            close()
                            replay()
                        }
                        else -> {
                            view.contentDescription = when (meta.getString("room")) {
                                "cg" -> "原版 CG 鉴赏"
                                "scene" -> "原版回想鉴赏"
                                "actor" -> "原版立绘鉴赏"
                                else -> "原版鉴赏"
                            }
                            view.present(checkNotNull(bitmap))
                            if (unlock) unlockPointer()
                        }
                    }
                }
            } catch (error: Throwable) {
                failed(error)
            }
        }
    }
}
