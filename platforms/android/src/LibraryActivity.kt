package org.nekopack.probe

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract as Docs
import android.text.Editable
import android.text.TextWatcher
import android.view.Gravity
import android.widget.*
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.Executors
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

internal object LibraryPaths {
    fun gameIndex(name: String) = listOf("BLESS", "BIRTH", "BRAVE", "BLACK").indexOfFirst { name.contains(it, true) }
    fun location(tree: Uri, id: String) = if (id == Docs.getTreeDocumentId(tree)) tree else Docs.buildDocumentUriUsingTree(tree, id)
    fun identity(tree: Uri, id: String) = Docs.buildDocumentUri(tree.authority!!, id).toString()
    fun saveDirectory(files: File, storage: Uri, index: Int, chinese: Boolean, probe: String? = null): File {
        val scope = MessageDigest.getInstance("SHA-256").digest("${storage}_$index".toByteArray())
            .joinToString("") { "%02x".format(it.toInt() and 255) }
        return File(files, "saves/$scope" + (if (chinese) "_zh" else "") + (probe?.let { "_probe_$it" } ?: ""))
    }
}

class LibraryActivity : Activity() {
    private external fun inspect(descriptors: IntArray, hasSave: Long, original: Long, archiveNames: ByteArray): ByteArray
    private data class Game(val tree: Uri, val id: String, val name: String, val chinese: Boolean = false, val patches: List<String> = emptyList()) {
        val key get() = LibraryPaths.identity(tree, id)
        val index get() = LibraryPaths.gameIndex(name)
        val location get() = LibraryPaths.location(tree, id)
    }
    private data class Entry(val id: String, val name: String, val directory: Boolean)
    private data class Report(val title: String, val detail: String, val game: Game? = null)
    private enum class Page(val title: String) { LIBRARY("游戏库"), RECENT("最近"), IMPORT("导入"), SETTINGS("设置") }
    private data class Candidate(val slot: Int, val file: File)
    private val prefs by lazy { getSharedPreferences("library_options", MODE_PRIVATE) }
    private val registry by lazy { getPreferences(MODE_PRIVATE) }
    private val worker = Executors.newSingleThreadExecutor()
    private val games = linkedMapOf<String, Game>()
    private var reports = emptyList<Report>()
    private var page = Page.LIBRARY
    private var detail: String? = null
    private var detailSheet: android.app.Dialog? = null
    private var backup: Pair<Game, Boolean>? = null
    private var query = ""
    private var message = ""
    private var scanning = false
    private lateinit var content: LinearLayout
    private lateinit var gameList: LinearLayout
    private var renderGeneration = 0
    private val ink = 0xff242031.toInt()
    private val muted = 0xff645a72.toInt()
    private val accent = 0xff5f388e.toInt()
    private val background = 0xfff7f3fa.toInt()
    private val surface = 0xfffffbff.toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.statusBarColor = background
        window.navigationBarColor = background
        window.decorView.systemUiVisibility = android.view.View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or
            (if (android.os.Build.VERSION.SDK_INT >= 26) android.view.View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR else 0)
        val saved = JSONArray(registry.getString("games", "[]"))
        for (i in 0 until saved.length()) {
            val row = saved.getJSONObject(i)
            val patches = row.optJSONArray("patches") ?: JSONArray()
            val game = Game(Uri.parse(row.getString("tree")), row.getString("id"), row.getString("name"),
                row.optBoolean("chinese"), List(patches.length()) { patches.getString(it) })
            games[game.key] = game
        }
        render()
        if ((0 until saved.length()).any { !saved.getJSONObject(it).has("chinese") }) {
            scan(grants())
        } else if (!registry.contains("games")) {
            val legacy = getSharedPreferences("MainActivity", MODE_PRIVATE).getString("dat_tree", null)
            grants().firstOrNull { it.toString() == legacy }?.let { scan(listOf(it)) }
        }
    }

    override fun onResume() {
        super.onResume()
        if (::content.isInitialized && !scanning) render()
    }

    private fun grants() = contentResolver.persistedUriPermissions.filter { it.isReadPermission }.map { it.uri }
    private fun authorized(game: Game) = game.tree in grants()
    private fun chooseFolder(game: Game? = null) {
        startActivityForResult(Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
            if (game != null && android.os.Build.VERSION.SDK_INT >= 26) putExtra(Docs.EXTRA_INITIAL_URI, game.location)
        }, if (game == null) 73 else 75)
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (resultCode != RESULT_OK) return
        val uri = data?.data ?: return
        if (requestCode == 74) {
            val (game, chinese) = backup ?: return
            backup = null
            exportBackup(game, chinese, uri)
            return
        }
        if (requestCode != 73 && requestCode != 75) return
        try {
            contentResolver.takePersistableUriPermission(uri, data.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION)
            detail = null
            page = Page.IMPORT
            scan(listOf(uri), review = requestCode == 73)
        } catch (_: SecurityException) {
            message = "目录授权失败，请重新选择目录。"
            render()
        }
    }

    private fun children(tree: Uri, id: String): List<Entry> {
        val projection = arrayOf(Docs.Document.COLUMN_DOCUMENT_ID, Docs.Document.COLUMN_DISPLAY_NAME, Docs.Document.COLUMN_MIME_TYPE)
        val uri = Docs.buildChildDocumentsUriUsingTree(tree, id)
        return contentResolver.query(uri, projection, null, null, null)?.use { cursor ->
            buildList {
                while (cursor.moveToNext()) add(Entry(cursor.getString(0), cursor.getString(1), cursor.getString(2) == Docs.Document.MIME_TYPE_DIR))
            }
        } ?: error("系统文件选择器无法读取目录")
    }

    private fun datFiles(entries: List<Entry>) = entries.filter { !it.directory && it.name.endsWith(".dat", true) && !it.name.equals("unins000.dat", true) }

    private fun inspectGame(game: Game, original: Boolean, candidate: Candidate? = null, entries: List<Entry>? = null): JSONObject {
        System.loadLibrary("nekopack_host")
        val descriptors = mutableListOf<ParcelFileDescriptor>()
        try {
            if (candidate != null) descriptors.add(ParcelFileDescriptor.open(candidate.file, ParcelFileDescriptor.MODE_READ_ONLY))
            val archives = datFiles(entries ?: children(game.tree, game.id))
            for (entry in archives) {
                try {
                    descriptors.add(contentResolver.openFileDescriptor(Docs.buildDocumentUriUsingTree(game.tree, entry.id), "r")
                        ?: error("文件不可读"))
                } catch (error: Exception) {
                    throw IllegalStateException("${entry.name}：${error.message}", error)
                }
            }
            check(descriptors.size > if (candidate == null) 0 else 1) { "未找到可读取的 DAT" }
            val result = JSONObject(String(inspect(descriptors.map { it.fd }.toIntArray(), if (candidate == null) 0 else 1,
                if (original) 1 else 0, archives.joinToString("\u0000") { it.name }.toByteArray(Charsets.UTF_8)), Charsets.UTF_8).substringBefore('\n'))
            check(!result.has("error")) { result.getString("error") }
            return result
        } finally {
            descriptors.forEach { it.close() }
        }
    }

    private fun scan(trees: List<Uri>, review: Boolean = false) {
        if (scanning) return
        scanning = true
        message = "正在读取目录和 DAT 索引…"
        reports = emptyList()
        render()
        worker.execute {
            val results = mutableListOf<Report>()
            val found = mutableListOf<Game>()
            for (tree in trees) {
                val root = Docs.getTreeDocumentId(tree)
                val rootName = runCatching {
                    contentResolver.query(Docs.buildDocumentUriUsingTree(tree, root), arrayOf(Docs.Document.COLUMN_DISPLAY_NAME), null, null, null)
                        ?.use { if (it.moveToFirst()) it.getString(0) else null }
                }.getOrNull() ?: root.substringAfterLast('/')
                val pending = java.util.ArrayDeque<Entry>()
                val seen = mutableSetOf<String>()
                pending.add(Entry(root, rootName, true))
                var candidates = 0
                while (pending.isNotEmpty()) {
                    val dir = pending.removeFirst()
                    if (!seen.add(dir.id)) continue
                    val entries = try { children(tree, dir.id) } catch (error: Exception) {
                        results.add(Report(dir.name, "目录读取失败 · ${error.message}"))
                        continue
                    }
                    entries.filter { it.directory }.forEach(pending::add)
                    val files = datFiles(entries)
                    val names = files.map { it.name.lowercase() }.toSet()
                    if ("system.dat" !in names && "image.dat" !in names) continue
                    candidates++
                    val missing = listOf("system.dat", "image.dat").filterNot { it in names }
                    if (missing.isNotEmpty()) {
                        results.add(Report(dir.name, "缺少 ${missing.joinToString("、")}"))
                        continue
                    }
                    val game = Game(tree, dir.id, dir.name)
                    runCatching { inspectGame(game, false, entries = entries) }.onSuccess { metadata ->
                        val chinese = metadata.getBoolean("chinese")
                        val recognized = metadata.getJSONArray("patches")
                        val patches = List(recognized.length()) { files[recognized.getInt(it)].name }
                        val unrecognized = files.filter { it.name.contains(Regex("chs|chinese|patch|cn\\.", RegexOption.IGNORE_CASE)) && it.name !in patches }.map { it.name }
                        val inspected = game.copy(chinese = chinese, patches = patches)
                        found.add(inspected)
                        val patch = when {
                            chinese -> "汉化剧情包已识别" + patches.takeIf { it.isNotEmpty() }?.joinToString("、", " · ").orEmpty()
                            unrecognized.isNotEmpty() -> "未确认汉化 · ${unrecognized.joinToString("、")}"
                            else -> "原版资源"
                        }
                        results.add(Report(dir.name, patch, inspected))
                    }.onFailure { results.add(Report(dir.name, "资源读取失败 · ${it.message}")) }
                }
                if (candidates == 0) results.add(Report(rootName, "未找到含 system.dat 或 image.dat 的游戏目录"))
            }
            runOnUiThread {
                if (isDestroyed) return@runOnUiThread
                val unique = found.distinctBy { it.key }
                unique.filter { !review || it.key in games }.forEach { games[it.key] = it }
                persistRegistry()
                reports = results.distinctBy { it.game?.key ?: "${it.title}\n${it.detail}" }
                message = "已识别 ${unique.size} 款游戏"
                scanning = false
                render()
            }
        }
    }

    private fun persistRegistry() {
        registry.edit().putString("games", JSONArray(games.values.map {
            JSONObject().put("tree", it.tree.toString()).put("id", it.id).put("name", it.name)
                .put("chinese", it.chinese).put("patches", JSONArray(it.patches))
        }).toString()).apply()
    }

    private fun original(game: Game) = !game.chinese || prefs.getBoolean("original_${game.key}", false)
    private fun saveFolder(game: Game, chinese: Boolean) =
        LibraryPaths.saveDirectory(filesDir, if (game.index < 0) game.location else game.tree, game.index.coerceAtLeast(0), chinese)

    private fun latestSave(game: Game, original: Boolean): Candidate? {
        val directory = saveFolder(game, !original && game.chinese)
        val manual = directory.listFiles()?.mapNotNull { file ->
            Regex("slot-(\\d{3})\\.nksv").matchEntire(file.name)?.groupValues?.get(1)?.toInt()
                ?.takeIf { it in 1..100 }?.let { Candidate(it, file) }
        }.orEmpty()
        val quick = runCatching {
            (1..10).mapNotNull { id -> QuickSlots(directory).slot(id)?.let { Candidate(100 + id, it) } }
        }.getOrDefault(emptyList())
        return (manual + quick).sortedByDescending { it.file.lastModified() }.firstOrNull {
            it.file.isFile && runCatching { inspectGame(game, original, it).getBoolean("valid") }.getOrDefault(false)
        }
    }

    private fun launch(game: Game, original: Boolean, slot: Int = 0) {
        if (!authorized(game)) { chooseFolder(game); return }
        registry.edit().putString("recent", game.key).putLong("last_played_${game.key}", System.currentTimeMillis()).apply()
        startActivity(Intent(this, MainActivity::class.java)
            .putExtra("library_entry", game.key)
            .putExtra("library_tree", game.tree.toString())
            .putExtra("library_directory", game.id)
            .putExtra("library_original", original)
            .putExtra("library_resume_slot", slot)
            .putExtra("title_game", game.index.coerceAtLeast(0))
            .putExtra("library_game_storage", game.index < 0)
            .putExtra("run_id", "library-" + UUID.randomUUID()))
    }

    private fun attachContinue(row: LinearLayout, game: Game, original: Boolean, generation: Int, summary: TextView? = null) {
        if (!authorized(game)) return
        worker.execute {
            val candidate = latestSave(game, original)
            runOnUiThread {
                if (candidate != null && !isDestroyed && generation == renderGeneration) {
                    row.addView(button("继续存档", true) { launch(game, original, candidate.slot) })
                    val slot = if (candidate.slot > 100) "快捷档 ${candidate.slot - 100}" else "档位 %03d".format(candidate.slot)
                    val stamp = android.text.format.DateFormat.getDateFormat(this).format(java.util.Date(candidate.file.lastModified()))
                    summary?.text = "最近可读取 · $slot · $stamp"
                } else if (!isDestroyed && generation == renderGeneration) {
                    summary?.text = "当前版本没有可读取的存档"
                }
            }
        }
    }

    override fun onConfigurationChanged(newConfig: android.content.res.Configuration) {
        super.onConfigurationChanged(newConfig)
        render()
    }

    private fun render() {
        val generation = ++renderGeneration
        val game = detail?.let(games::get)
        if (game != null) {
            renderDetailSheet(game, generation)
            return
        }
        val closingSheet = detailSheet
        detailSheet = null
        closingSheet?.dismiss()
        val wide = resources.configuration.orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE
        val root = column().apply { setBackgroundColor(this@LibraryActivity.background) }
        val body = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        val area = column().apply { isFocusableInTouchMode = true }
        val scroll = ScrollView(this).apply { isFillViewport = true }
        content = column().apply { setPadding(dp(16), dp(10), dp(16), dp(22)) }
        scroll.addView(content)
        val header = row().apply { setPadding(dp(16), dp(10), dp(16), dp(7)) }
        header.addView(mark(32))
        header.addView(label(page.title, 24, ink, true).apply { setPadding(dp(10), 0, 0, 0) },
            LinearLayout.LayoutParams(0, -2, 1f))
        if (page == Page.LIBRARY) header.addView(button("导入游戏", true) { showPage(Page.IMPORT) })
        area.addView(header)
        if (wide) body.addView(navigation(true), LinearLayout.LayoutParams(dp(86), -1))
        area.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        body.addView(area, LinearLayout.LayoutParams(0, -1, 1f))
        root.addView(body, LinearLayout.LayoutParams(-1, 0, 1f))
        if (!wide) root.addView(navigation(false))
        setContentView(root)
        area.requestFocus()
        when (page) {
            Page.LIBRARY -> renderLibrary(wide)
            Page.RECENT -> renderRecent(generation)
            Page.IMPORT -> renderImport()
            Page.SETTINGS -> renderSettings()
        }
    }

    private fun showPage(value: Page) {
        if (page == value && detail == null) return
        detail = null
        page = value
        render()
    }

    private fun navigation(wide: Boolean) = LinearLayout(this).apply {
        orientation = if (wide) LinearLayout.VERTICAL else LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER
        setPadding(dp(5), dp(5), dp(5), dp(5))
        setBackgroundColor(0xfff0e8f6.toInt())
        Page.values().filter { it != Page.RECENT }.forEach { destination ->
            val selected = destination == page
            addView(button(destination.title, selected) { showPage(destination) }.apply {
                isSelected = selected
                minimumHeight = dp(48)
                minHeight = dp(48)
                setPadding(dp(4), 0, dp(4), 0)
            }, if (wide) LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(5) }
            else LinearLayout.LayoutParams(0, -2, 1f).apply { marginEnd = dp(2) })
        }
    }

    private fun renderLibrary(wide: Boolean) {
        val search = EditText(this).apply {
            hint = "搜索游戏"
            contentDescription = "搜索游戏"
            setSingleLine()
            setTextColor(ink)
            setHintTextColor(muted)
            background = rounded(surface, 14, 0xffded2e7.toInt())
            setPadding(dp(14), 0, dp(14), 0)
            minimumHeight = dp(48)
            setText(query)
        }
        content.addView(search)
        renderRecent(renderGeneration, latestOnly = true)
        content.addView(label("全部游戏", 18, ink, true), spaced(18))
        gameList = column()
        content.addView(gameList, spaced(10))
        search.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                query = s.toString()
                renderGames(wide)
            }
            override fun afterTextChanged(s: Editable?) {}
        })
        renderGames(wide)
    }

    private fun renderGames(wide: Boolean) {
        gameList.removeAllViews()
        val filtered = games.values.filter { it.name.contains(query.trim(), true) }.sortedBy { it.name.lowercase() }
        if (filtered.isEmpty()) {
            val empty = card()
            empty.addView(label(if (games.isEmpty()) "游戏库为空" else "没有匹配的游戏", 18, ink, true))
            if (games.isEmpty()) empty.addView(button("导入游戏", true) { showPage(Page.IMPORT) }, spaced(10))
            gameList.addView(empty, spaced(4))
        }
        val columns = if (wide || resources.configuration.screenWidthDp >= 380) 2 else 1
        for (group in filtered.chunked(columns)) {
            val line = row().apply { gravity = Gravity.TOP }
            group.forEachIndexed { index, game ->
                val available = authorized(game)
                val entry = card().apply {
                    background = ripple(surface, 16, 0xffe0d4e8.toInt())
                    contentDescription = "游戏详情 ${game.name}"
                    setOnClickListener { openDetail(game) }
                    isFocusable = true
                }
                val tile = row().apply {
                    background = GradientDrawable(GradientDrawable.Orientation.TL_BR,
                        intArrayOf(0xffeadbf5.toInt(), 0xfff3eaf8.toInt())).apply { cornerRadius = dp(12).toFloat() }
                    gravity = Gravity.CENTER
                }
                tile.addView(mark(42))
                entry.addView(tile, LinearLayout.LayoutParams(-1, dp(75)))
                entry.addView(label(game.name, 16, ink, true).apply {
                    maxLines = 2
                    ellipsize = android.text.TextUtils.TruncateAt.END
                }, spaced(8))
                entry.addView(label(if (available) version(game, original(game)) else "目录授权已失效", 12, muted), spaced(4))
                if (!available) entry.addView(button("重新授权") { chooseFolder(game) }, spaced(6))
                line.addView(entry, LinearLayout.LayoutParams(0, -2, 1f).apply { if (index > 0) marginStart = dp(9) })
            }
            if (group.size < columns) line.addView(android.view.View(this), LinearLayout.LayoutParams(0, 1, 1f).apply { marginStart = dp(9) })
            gameList.addView(line, spaced(8))
        }
    }

    private fun renderRecent(generation: Int, latestOnly: Boolean = false) {
        val history = games.values.mapNotNull { game ->
            registry.getLong("last_played_${game.key}", 0).takeIf { it > 0 }?.let { game to it }
        }.sortedByDescending { it.second }
        if (history.isEmpty()) {
            if (!latestOnly) content.addView(label("还没有游玩记录", 17, muted))
            return
        }
        if (latestOnly) content.addView(label("最近游玩", 18, ink, true), spaced(18))
        (if (latestOnly) history.take(1) else history).forEachIndexed { index, (recent, playedAt) ->
            val panel = if (index == 0) hero() else card()
            panel.addView(label(recent.name, if (index == 0) 24 else 17, ink, true))
            val date = android.text.format.DateFormat.getDateFormat(this).format(java.util.Date(playedAt))
            panel.addView(label("${version(recent, original(recent))} · $date", 13, muted), spaced(5))
            val actions = row()
            actions.addView(button("启动游戏", index == 0) { launch(recent, original(recent)) })
            actions.addView(button("游戏详情") { openDetail(recent) })
            panel.addView(actions, spaced(8))
            if (index == 0) {
                val resume = column()
                panel.addView(resume)
                attachContinue(resume, recent, original(recent), generation)
            }
            content.addView(panel, spaced(if (index == 0) 0 else 8))
        }
    }

    private fun renderImport() {
        val tools = row()
        tools.addView(button("添加目录", true) { chooseFolder() }.apply { isEnabled = !scanning },
            LinearLayout.LayoutParams(0, -2, 1f).apply { marginEnd = dp(6) })
        tools.addView(button("扫描已授权目录") { scan(grants(), review = true) }.apply {
            isEnabled = !scanning && grants().isNotEmpty()
        }, LinearLayout.LayoutParams(0, -2, 1f))
        content.addView(tools)
        if (message.isNotEmpty()) content.addView(label(message, 15, accent, true), spaced(12))
        val pending = reports.mapNotNull { it.game }.filter { it.key !in games }
        if (pending.isNotEmpty()) content.addView(button("导入全部（${pending.size}）", true) { importGames(pending) }, spaced(10))
        if (reports.isEmpty() && !scanning) {
            content.addView(label("选择包含游戏 DAT 的文件夹，或包含多个游戏的上级目录。", 15, muted), spaced(14))
        }
        val ordered = reports.sortedBy { report ->
            when (val game = report.game) {
                null -> 1
                else -> if (game.key in games) 2 else 0
            }
        }
        ordered.forEach { report ->
            val line = card()
            line.addView(label(report.title, 16, ink, true))
            line.addView(label(report.detail, 13, muted), spaced(4))
            report.game?.let { game ->
                if (game.key in games) line.addView(label("已在游戏库", 13, accent), spaced(7))
                else line.addView(button("导入", true) { importGames(listOf(game)) }.apply {
                    contentDescription = "导入 ${game.name}"
                }, spaced(8))
            }
            content.addView(line, spaced(8))
        }
    }

    private fun importGames(imported: List<Game>) {
        imported.forEach { games[it.key] = it }
        persistRegistry()
        message = "已导入 ${imported.size} 款游戏"
        render()
    }

    private fun renderSettings() {
        val panel = card()
        panel.addView(button("鼠标与手势") { InputBindings(this).show(this) }, spaced(0))
        panel.addView(CheckBox(this).apply {
            text = "运行诊断"
            setTextColor(ink)
            isChecked = prefs.getBoolean("diagnostics", false)
            setOnCheckedChangeListener { _, checked -> prefs.edit().putBoolean("diagnostics", checked).apply() }
        }, spaced(6))
        content.addView(panel)
    }

    private fun version(game: Game, original: Boolean) = if (original || !game.chinese) "原版资源" else "中文补丁已启用"

    private fun openDetail(game: Game) {
        detail = game.key
        message = ""
        render()
        if (!authorized(game)) return
        worker.execute {
            val inspected = runCatching {
                val files = datFiles(children(game.tree, game.id))
                val result = inspectGame(game, false, entries = files)
                val patches = result.getJSONArray("patches")
                game.copy(chinese = result.getBoolean("chinese"), patches = List(patches.length()) { files[patches.getInt(it)].name })
            }
            runOnUiThread {
                if (isDestroyed || detail != game.key) return@runOnUiThread
                inspected.onSuccess {
                    if (it != games[game.key]) {
                        games[it.key] = it
                        persistRegistry()
                        render()
                    }
                }.onFailure {
                    message = "资源读取失败 · ${it.message}"
                    render()
                }
            }
        }
    }

    private fun renderDetailSheet(game: Game, generation: Int) {
        val shell = column().apply { setBackgroundColor(this@LibraryActivity.background) }
        val bar = row().apply { setPadding(dp(16), dp(10), dp(16), dp(8)) }
        bar.addView(button("返回游戏库") { detail = null; render() })
        bar.addView(label("游戏详情", 20, ink, true).apply { setPadding(dp(12), 0, 0, 0) })
        shell.addView(bar)
        content = column().apply { setPadding(dp(20), dp(8), dp(20), dp(24)) }
        shell.addView(ScrollView(this).apply { addView(content) }, LinearLayout.LayoutParams(-1, 0, 1f))
        renderDetail(game, generation)
        setContentView(shell)
    }

    private fun renderDetail(game: Game, generation: Int) {
        val original = original(game)
        val title = hero()
        title.addView(label(game.name, 28, ink, true))
        title.addView(label(version(game, original), 14, muted), spaced(6))
        content.addView(title, spaced(12))
        if (!authorized(game)) {
            content.addView(label("目录授权已失效", 16, accent), spaced(12))
            content.addView(button("重新授权", true) { chooseFolder(game) }, spaced(8))
            return
        }
        val versions = card()
        versions.addView(label("启动版本", 17, ink, true))
        val options = row()
        options.addView(button("原版", original) { prefs.edit().putBoolean("original_${game.key}", true).apply(); render() })
        if (game.chinese) options.addView(button("中文补丁", !original) { prefs.edit().putBoolean("original_${game.key}", false).apply(); render() })
        versions.addView(options, spaced(6))
        versions.addView(label(version(game, original), 13, muted), spaced(6))
        if (game.patches.isNotEmpty()) versions.addView(label(
            (if (original) "目录内未启用的补丁 · " else "生效的补丁 · ") + game.patches.joinToString("、"), 13, muted
        ), spaced(4))
        val actions = row()
        actions.addView(button("启动游戏", true) { launch(game, original) },
            LinearLayout.LayoutParams(0, -2, 1f).apply { marginEnd = dp(6) })
        actions.addView(button("查看存档") { launch(game, original, slot = -1) },
            LinearLayout.LayoutParams(0, -2, 1f))
        versions.addView(actions, spaced(10))
        val saveSummary = label("正在校验当前版本的存档…", 13, muted)
        versions.addView(saveSummary, spaced(6))
        val resume = column()
        versions.addView(resume)
        attachContinue(resume, game, original, generation, saveSummary)
        content.addView(versions, spaced(16))
        val directory = card()
        directory.addView(label("游戏目录", 17, ink, true))
        directory.addView(label(game.id.removePrefix("raw:"), 13, muted), spaced(6))
        content.addView(directory, spaced(12))
        val overrides = card()
        overrides.addView(label("单游戏覆盖", 17, ink, true))
        overrideSlider(overrides, game, "speed", "文字速度", 50)
        overrideSlider(overrides, game, "volume", "模拟器主音量", 100)
        val enabled = prefs.getBoolean("input_${game.key}", false)
        overrides.addView(CheckBox(this).apply {
            text = "单独设置鼠标与手势"; setTextColor(ink); isChecked = enabled
            setOnCheckedChangeListener { _, checked -> prefs.edit().putBoolean("input_${game.key}", checked).apply(); render() }
        })
        if (enabled) overrides.addView(button("编辑鼠标与手势") { InputBindings(this, game.key).show(this) })
        content.addView(overrides, spaced(12))
        val saves = card()
        saves.addView(label("存档和鉴赏进度", 17, ink, true))
        saves.addView(label("导出当前启动版本的备份 ZIP。", 13, muted))
        saves.addView(button("导出备份") {
            backup = game to (!original && game.chinese)
            startActivityForResult(Intent(Intent.ACTION_CREATE_DOCUMENT).setType("application/zip")
                .addCategory(Intent.CATEGORY_OPENABLE).putExtra(Intent.EXTRA_TITLE, "${game.name}-${if (original) "original" else "zh"}-backup.zip"), 74)
        }, spaced(8))
        content.addView(saves, spaced(12))
        if (message.isNotEmpty()) content.addView(label(message, 14, muted), spaced(8))
    }

    private fun overrideSlider(target: LinearLayout, game: Game, option: String, title: String, fallback: Int) {
        val key = "${option}_${game.key}"
        val current = prefs.getInt(key, -1)
        target.addView(CheckBox(this).apply {
            text = title; setTextColor(ink); isChecked = current >= 0
            setOnCheckedChangeListener { _, checked -> prefs.edit().putInt(key, if (checked) fallback else -1).apply(); render() }
        })
        if (current < 0) return
        val value = label("$current%", 14, muted)
        target.addView(value)
        target.addView(SeekBar(this).apply {
            max = 100; progress = current; contentDescription = title
            setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                override fun onStartTrackingTouch(bar: SeekBar) {}
                override fun onProgressChanged(bar: SeekBar, progress: Int, fromUser: Boolean) { value.text = "$progress%" }
                override fun onStopTrackingTouch(bar: SeekBar) { prefs.edit().putInt(key, bar.progress).apply() }
            })
        })
    }

    private fun exportBackup(game: Game, chinese: Boolean, destination: Uri) {
        worker.execute {
            val result = runCatching {
                val folder = saveFolder(game, chinese)
                val files = folder.listFiles()?.filter { it.isFile && (it.extension == "nksv" || it.name == "quick-index.json") }.orEmpty()
                val progressKey = "runtime_unlocks_v1_${game.location}_${game.index.coerceAtLeast(0)}"
                val app = getSharedPreferences("MainActivity", MODE_PRIVATE)
                val output = contentResolver.openOutputStream(destination, "wt") ?: error("无法写入所选位置")
                ZipOutputStream(output).use { zip ->
                    fun entry(name: String, write: () -> Unit) {
                        zip.putNextEntry(ZipEntry(name)); write(); zip.closeEntry()
                    }
                    entry("manifest.json") {
                        zip.write(JSONObject().put("game", game.name).put("identity", game.key).put("chinese", chinese).toString(2).toByteArray())
                    }
                    for (file in files) entry("saves/${file.name}") { file.inputStream().use { it.copyTo(zip) } }
                    app.getString(progressKey, null)?.let { value -> entry("unlocks.bin") { zip.write(android.util.Base64.decode(value, android.util.Base64.DEFAULT)) } }
                }
                files.count { it.extension == "nksv" }
            }
            runOnUiThread { if (!isDestroyed) {
                message = result.fold({ "已导出备份 · $it 个存档" }, { "导出失败 · ${it.message}" })
                render()
            } }
        }
    }

    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        when {
            detail != null -> { detail = null; render() }
            page != Page.LIBRARY -> showPage(Page.LIBRARY)
            else -> super.onBackPressed()
        }
    }
    override fun onDestroy() {
        detailSheet?.setOnDismissListener(null)
        detailSheet?.dismiss()
        worker.shutdownNow()
        super.onDestroy()
    }
    private fun column() = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
    private fun row() = LinearLayout(this).apply { gravity = Gravity.CENTER_VERTICAL }
    private fun mark(size: Int) = ImageView(this).apply {
        setImageResource(resources.getIdentifier("neko_mark", "drawable", packageName))
        layoutParams = LinearLayout.LayoutParams(dp(size), dp(size))
    }
    private fun card() = column().apply {
        background = rounded(surface, 16, 0xffe2d5e9.toInt())
        setPadding(dp(16), dp(15), dp(16), dp(15))
    }
    private fun hero() = column().apply {
        background = GradientDrawable(GradientDrawable.Orientation.TL_BR,
            intArrayOf(0xffeadbf5.toInt(), 0xfff4eafa.toInt())).apply {
            cornerRadius = dp(22).toFloat()
            setStroke(dp(1), 0xffdccce9.toInt())
        }
        setPadding(dp(18), dp(18), dp(18), dp(18))
    }
    private fun dp(value: Int) = (value * resources.displayMetrics.density + 0.5f).toInt()
    private fun spaced(top: Int) = LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(top) }
    private fun rounded(color: Int, radius: Int, stroke: Int = 0) = GradientDrawable().apply {
        setColor(color)
        cornerRadius = dp(radius).toFloat()
        if (stroke != 0) setStroke(dp(1), stroke)
    }
    private fun ripple(color: Int, radius: Int, stroke: Int = 0) = android.graphics.drawable.RippleDrawable(
        android.content.res.ColorStateList.valueOf(0x33ffffff), rounded(color, radius, stroke), null
    )
    private fun label(value: String, size: Int, color: Int, bold: Boolean = false) = TextView(this).apply {
        text = value; textSize = size.toFloat(); setTextColor(color)
        if (bold) typeface = Typeface.DEFAULT_BOLD
    }
    private fun button(value: String, primary: Boolean = false, click: () -> Unit) = Button(this).apply {
        text = value; isAllCaps = false; minimumHeight = dp(44); minWidth = dp(48)
        setPadding(dp(10), 0, dp(10), 0)
        setTextColor(android.content.res.ColorStateList(
            arrayOf(intArrayOf(-android.R.attr.state_enabled), intArrayOf()),
            intArrayOf(0xff817489.toInt(), if (primary) android.graphics.Color.WHITE else ink)
        ))
        val fill = android.graphics.drawable.StateListDrawable().apply {
            addState(intArrayOf(-android.R.attr.state_enabled), rounded(0xffeee7f0.toInt(), 12))
            addState(intArrayOf(), ripple(if (primary) accent else 0xffeee2f5.toInt(), 12))
        }
        background = fill
        layoutParams = LinearLayout.LayoutParams(-2, -2).apply { marginEnd = dp(6) }
        setOnClickListener { click() }
    }
}
