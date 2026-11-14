package org.nekopack.probe

import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.os.ParcelFileDescriptor
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ImageView
import android.widget.FrameLayout
import android.graphics.BitmapFactory
import org.json.JSONObject
import java.util.concurrent.Executors
import android.app.Activity
import android.os.Bundle
import android.os.Build
import android.system.Os
import android.system.OsConstants
import android.widget.TextView
import java.io.File

/** Validation shell only: all VM lifetime stays within one native call on its worker. */
class MainActivity : Activity() {
    private external fun story(descriptors:IntArray,operation:Int,sequence:Long,elapsedMillis:Long=0,archiveNames:ByteArray=byteArrayOf()):ByteArray
    private val routeReplay by lazy { intent.getBooleanExtra("route_replay",false) }
    private val timeline by lazy { SessionTimeline(if(routeReplay)8 else 1) }
    private val menuTimeline=SessionTimeline()
    @Volatile private var storyConfigOpen=false
    @Volatile private var storySaveOpen=false
    @Volatile private var storyLogOpen=false
    private var storyLogBack:(()->Boolean)?=null
    private val libraryOptions by lazy { getSharedPreferences("library_options", MODE_PRIVATE) }
    private val inputBindings by lazy {
        val game=intent.getStringExtra("library_entry")
        InputBindings(this,game?.takeIf { libraryOptions.getBoolean("input_$it",false) })
    }
    private val storyPointer by lazy { StoryPointer(48f * resources.displayMetrics.density) }
    private var storyInputArea: ((android.view.MotionEvent) -> Boolean)? = null
    private var storyInputAction: ((StoryAction) -> Unit)? = null
    private var systemCommandPresentation:SystemCommandPresentation?=null
    private var systemCommandTouch:((android.view.MotionEvent)->Boolean)?=null
    private var storySpace: ((Boolean) -> Boolean)? = null
    private var rightMouseAction = StoryAction.HIDE
    private var roomBack: (() -> Boolean)? = null
    private var openEmulatorMenu: (() -> Unit)? = null
    private var emulatorMenu: EmulatorMenu? = null
    private var originalConfirmOpen = false
    private var foreground = false
    private var shortcutKey:((Int)->Unit)?=null
    private var storySaveBack:(()->Boolean)?=null
    private var storyMenuBack:(()->Boolean)?=null
    private var hiddenBack:(()->Boolean)?=null
    private var settingsBack:(()->Boolean)?=null
    private var lifecycleGeneration=0L
    private val instanceId=java.util.UUID.randomUUID().toString()
    private var lifecycleFailure:(Throwable)->Unit={throw it}
    private var storyWorker:java.util.concurrent.ExecutorService?=null
    private var movieHost:MovieHost?=null
    private var storyLoaded=false
    private var storyEffects:StoryEffects?=null
    private var storyRunRoot:File?=null
    @Volatile private var storyClosing=false
    private external fun runDat(descriptors: IntArray, choice: Int): ByteArray
    private external fun runChecks(root: ByteArray): ByteArray

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        hidePlayerStatusBar()
        intent.getStringExtra("library_tree")?.let { saved ->
            startStory(Uri.parse(saved), intent.getIntExtra("title_game", 0), true)
            return
        }
        if(intent.hasExtra("movie_probe_game") || intent.hasExtra("movie_probe_name")){
            val saved=getPreferences(MODE_PRIVATE).getString("dat_tree",null) ?: error("select DAT tree through SAF first")
            val game=intent.getIntExtra("movie_probe_game",0);require(game in 0..3)
            val runId=intent.getStringExtra("run_id") ?: "movie-manual";require(Regex("[a-zA-Z0-9_-]{1,64}").matches(runId))
            val root=File(filesDir,runId);root.mkdirs();storyRunRoot=root
            val stage=FrameLayout(this);setContentView(stage)
            val tree=Uri.parse(saved);val top=children(tree,DocumentsContract.getTreeDocumentId(tree))
            val items=if(top.any{it.first.endsWith(".dat",true)})top else children(tree,DocumentsContract.getDocumentId(top.single{it.first==game.toString()}.second))
            val movieName=intent.getStringExtra("movie_probe_name") ?: "op.mpg"
            require(movieName.isNotEmpty() && !movieName.contains('/') && !movieName.contains('\\'))
            val uri=items.single{it.first.equals(movieName,true)}.second
            System.loadLibrary("nekopack_host")
            lifecycleFailure={error->File(root,"error.txt").writeText(error.toString());movieHost?.close()}
            movieHost=MovieHost(this,stage,root,true)
            movieHost!!.start(uri,1,intent.getBooleanExtra("movie_left_skip",false),{reason->File(root,"movie-complete.json").writeText(JSONObject().put("reason",reason).toString());val text=TextView(this);text.text="视频验证完成";setContentView(text)},{error->File(root,"error.txt").writeText(error.toString())})
            return
        }
        if(intent.hasExtra("story_game") || intent.hasExtra("title_game")) {
            val saved=getPreferences(MODE_PRIVATE).getString("dat_tree",null) ?: error("select DAT tree through SAF first")
            startStory(Uri.parse(saved),intent.getIntExtra("title_game",intent.getIntExtra("story_game",0)),intent.hasExtra("title_game"))
            return
        }
        if (intent.getBooleanExtra("saf", false)) {
            if(intent.getBooleanExtra("reuse_grant",false)) {
                val saved=getPreferences(MODE_PRIVATE).getString("dat_tree",null) ?: error("no persisted DAT selection")
                runTree(Uri.parse(saved))
                return
            }
            val button=Button(this)
            button.text="Select original DAT directory (SAF)"
            button.setOnClickListener {
                startActivityForResult(Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION), 42)
            }
            setContentView(button)
            return
        }
        if(!intent.hasExtra("run_id")){
            val saved=getPreferences(MODE_PRIVATE).getString("dat_tree",null)
            if(saved!=null){startStory(Uri.parse(saved),0,true);return}
            val button=Button(this);button.text="选择原始 DAT 文件夹"
            button.setOnClickListener{startActivityForResult(Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION),42)}
            setContentView(button);return
        }
        val view = TextView(this)
        view.text = "NekoPack ARM64 validation running…"
        view.setPadding(24, 24, 24, 24)
        setContentView(view)
        val runId = intent.getStringExtra("run_id") ?: "manual"
        require(Regex("[a-zA-Z0-9_-]{1,64}").matches(runId))
        val root = File(filesDir, runId)
        root.mkdirs()
        Thread({
            val report = try {
                System.loadLibrary("nekopack_host")
                "RUN\t$runId\nANDROID\t${Build.VERSION.SDK_INT}\t${Build.SUPPORTED_ABIS.joinToString(",")}\n" +
                    "PAGE_SIZE\t${Os.sysconf(OsConstants._SC_PAGESIZE)}\n" +
                    String(runChecks(root.absolutePath.toByteArray(Charsets.UTF_8)), Charsets.UTF_8)
            } catch (error: Throwable) {
                "RUN\t$runId\nFAIL\t${error.javaClass.name}: ${error.message}\n"
            }
            val temporary = File(root, "report.tmp")
            temporary.writeText(report)
            check(temporary.renameTo(File(root, "report.txt")))
            runOnUiThread { view.text = report }
        }, "NekoPackValidation").start()
    }
    private fun children(tree: Uri, id: String): List<Pair<String, Uri>> {
        val uri=DocumentsContract.buildChildDocumentsUriUsingTree(tree,id)
        val projection=arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID,DocumentsContract.Document.COLUMN_DISPLAY_NAME)
        val result=mutableListOf<Pair<String, Uri>>()
        val cursor=try {
            contentResolver.query(uri,projection,null,null,null)
        } catch(error:RuntimeException) {
            throw IllegalStateException("无法读取游戏目录，请检查目录位置和授权",error)
        } ?: error("目录提供程序没有返回读取结果")
        cursor.use { while(it.moveToNext()) result.add(it.getString(1) to DocumentsContract.buildDocumentUriUsingTree(tree,it.getString(0))) }
        return result
    }
    override fun onActivityResult(requestCode:Int,resultCode:Int,data:Intent?) {
        super.onActivityResult(requestCode,resultCode,data)
        if(requestCode!=42 || resultCode!=RESULT_OK)return
        val tree=data?.data ?: error("missing directory URI")
        contentResolver.takePersistableUriPermission(tree,Intent.FLAG_GRANT_READ_URI_PERMISSION)
        getPreferences(MODE_PRIVATE).edit().putString("dat_tree",tree.toString()).apply()
        if(intent.getBooleanExtra("saf",false))runTree(tree) else startStory(tree,0,true)
    }
    private fun runTree(tree:Uri) {
        val view=TextView(this);view.text="Reading original DAT through SAF…";setContentView(view)
        val runId=intent.getStringExtra("run_id") ?: "saf-manual"
        require(Regex("[a-zA-Z0-9_-]{1,64}").matches(runId))
        val root=File(filesDir,runId);root.mkdirs()
        Thread({
            val report=try {
                System.loadLibrary("nekopack_host")
                val entries=children(tree,DocumentsContract.getTreeDocumentId(tree))
                val groups=if(entries.any{it.first.endsWith(".dat",true)}) listOf("game" to entries) else
                    entries.sortedBy{it.first}.map{it.first to children(tree,DocumentsContract.getDocumentId(it.second))}
                val summary=StringBuilder("RUN\t$runId\nSAF\t$tree\nANDROID\t${Build.VERSION.SDK_INT}\t${Build.SUPPORTED_ABIS.joinToString(",")}\n")
                for((name,items) in groups) {
                    require(Regex("[a-zA-Z0-9_-]+").matches(name))
                    val opened=mutableListOf<ParcelFileDescriptor>()
                    try {
                        val archives=items.filter{it.first.endsWith(".dat",true)}.sortedBy{it.first}
                    require(archives.isNotEmpty()){"no DAT archives in $name"}
                        for((_,uri) in archives)opened.add(contentResolver.openFileDescriptor(uri,"r") ?: error("provider returned no descriptor"))
                        for(choice in listOf(1,999)) {
                            val output=String(runDat(opened.map{it.fd}.toIntArray(),choice),Charsets.UTF_8)
                            File(root,"game-$name-$choice.txt").writeText(output)
                            if(choice==1)check(!output.startsWith("FAIL	")){output}
                            else check(output.contains("requested input does not identify an enabled spot")){output}
                        }
                        summary.append("PASS	dat	$name	${archives.size} archives\n")
                    } finally {opened.forEach{it.close()}}
                }
                summary.toString()
            } catch(error:Throwable){"RUN\t$runId\nFAIL\t${error.javaClass.name}: ${error.message}\n"}
            val temporary=File(root,"report.tmp");temporary.writeText(report)
            check(temporary.renameTo(File(root,"report.txt")))
            runOnUiThread{view.text=report}
        },"NekoPackSAF").start()
    }

    private fun startStory(tree:Uri,game:Int,titleFirst:Boolean=false) {
        require(game in 0..3)
        val libraryDirectory = intent.getStringExtra("library_directory")
        val gameLocation = if (libraryDirectory == null || libraryDirectory == DocumentsContract.getTreeDocumentId(tree)) {
            tree
        } else {
            DocumentsContract.buildDocumentUriUsingTree(tree, libraryDirectory)
        }
        val runId=intent.getStringExtra("run_id") ?: "story-manual"
        require(Regex("[a-zA-Z0-9_-]{1,64}").matches(runId))
        val root=File(filesDir,runId);root.mkdirs();storyRunRoot=root
        File(root,"session-created.json").writeText(JSONObject().put("instance",instanceId).put("parent",intent.getStringExtra("restart_parent") ?: JSONObject.NULL).put("time_ms",android.os.SystemClock.elapsedRealtime()).toString())
        val legacyControls=intent.getBooleanExtra("legacy_probe_controls",false) &&
            applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0
        val libraryEntry=intent.getStringExtra("library_entry")
        val diagnostics=libraryEntry==null||libraryOptions.getBoolean("diagnostics",false)
        val traceDialogue=diagnostics||legacyControls||routeReplay||intent.getBooleanExtra("dialogue_probe",false)
        var resumeSlot=intent.getIntExtra("library_resume_slot",0)
        val originalVersion=intent.getBooleanExtra("library_original",false)
        val speedOverride=libraryEntry?.let{libraryOptions.getInt("speed_$it",-1)} ?: -1
        val volumeOverride=libraryEntry?.let{libraryOptions.getInt("volume_$it",-1)} ?: -1
        val layout=LinearLayout(this);layout.orientation=LinearLayout.VERTICAL
        val image=ImageView(this);image.scaleType=ImageView.ScaleType.FIT_CENTER
        val stage=FrameLayout(this);stage.setBackgroundColor(android.graphics.Color.BLACK)
        val overlay=ImageView(this);overlay.scaleType=ImageView.ScaleType.FIT_CENTER;overlay.alpha=0f
        stage.addView(image,FrameLayout.LayoutParams(-1,-1));stage.addView(overlay,FrameLayout.LayoutParams(-1,-1))
        val dialogueView=DialogueView(this);stage.addView(dialogueView,FrameLayout.LayoutParams(-1,-1))
        layout.addView(stage,LinearLayout.LayoutParams(-1,0,1f))
        val text=TextView(this);text.text="正在读取剧情…";text.textSize=16f;text.maxLines=3
        fun dp(value:Int)=(value*resources.displayMetrics.density+0.5f).toInt()
        val quickPanel=LinearLayout(this).apply {
            orientation=LinearLayout.VERTICAL
            visibility=android.view.View.GONE
            contentDescription="快捷浮窗"
            setPadding(dp(6),dp(6),dp(6),dp(6))
            background=android.graphics.drawable.GradientDrawable().apply {
                cornerRadius=dp(12).toFloat()
                setColor(0xb51d1726.toInt())
            }
        }
        fun quickAction(label:String, action:()->Unit):Button = Button(this).apply {
            this.text=label
            isAllCaps=false
            setTextColor(android.graphics.Color.WHITE)
            background=android.graphics.drawable.RippleDrawable(
                android.content.res.ColorStateList.valueOf(0x44ffffff),
                android.graphics.drawable.GradientDrawable().apply {
                    cornerRadius=dp(9).toFloat()
                    setColor(0xcd30253e.toInt())
                    setStroke(dp(1), 0xff78668b.toInt())
                },null)
            setOnClickListener{action()}
            quickPanel.addView(this,LinearLayout.LayoutParams(dp(72),dp(44)).apply { bottomMargin=dp(4) })
        }
        if(legacyControls)layout.addView(text) else {
            text.setTextColor(android.graphics.Color.WHITE)
            text.elevation=dp(4).toFloat()
            stage.addView(text,FrameLayout.LayoutParams(-2,-2,android.view.Gravity.TOP or android.view.Gravity.CENTER_HORIZONTAL))
        }
        val next=Button(this);next.text="继续";next.isEnabled=false;val controls=LinearLayout(this);controls.addView(next,LinearLayout.LayoutParams(0,-2,1f))
        val close=Button(this);close.text="结束运行";controls.addView(close,LinearLayout.LayoutParams(0,-2,1f));if(legacyControls)layout.addView(controls)
        val settings=Button(this);settings.text="音量";settings.isEnabled=false;controls.addView(settings,LinearLayout.LayoutParams(0,-2,1f))
        val textSettings=Button(this);textSettings.text="文字";textSettings.isEnabled=false;controls.addView(textSettings,LinearLayout.LayoutParams(0,-2,1f))
        val originalSettings=Button(this);originalSettings.text="原版设置";originalSettings.isEnabled=false;controls.addView(originalSettings,LinearLayout.LayoutParams(0,-2,1f))
        val auto=Button(this);auto.text="自动：关";auto.isEnabled=false;controls.addView(auto,LinearLayout.LayoutParams(0,-2,1f))
        val skip=Button(this);skip.text="快进：关";skip.isEnabled=false;controls.addView(skip,LinearLayout.LayoutParams(0,-2,1f))
        val hide=Button(this);hide.text="隐藏对白";hide.isEnabled=false;controls.addView(hide,LinearLayout.LayoutParams(0,-2,1f))
        val menu=Button(this);menu.text="剧情菜单";menu.isEnabled=false;controls.addView(menu,LinearLayout.LayoutParams(0,-2,1f))
        setContentView(layout)
        layout.post{File(root,"layout.json").writeText(JSONObject().put("width",layout.width).put("height",layout.height).put("stage_width",stage.width).put("stage_height",stage.height).put("orientation",resources.configuration.orientation).toString())}
        val worker=Executors.newSingleThreadExecutor();storyWorker=worker
        val preferenceKey="runtime_config_v1_${gameLocation}_$game"
        fun persistConfig(){
            val bytes=story(intArrayOf(),20,0)
            check(bytes.size>=5 && bytes.copyOfRange(0,4).contentEquals(byteArrayOf(78,75,67,70))){String(bytes,Charsets.UTF_8)}
            val encoded=android.util.Base64.encodeToString(bytes,android.util.Base64.NO_WRAP)
            val prefs=getPreferences(MODE_PRIVATE)
            if(prefs.getString(preferenceKey,null)!=encoded){
                check(prefs.edit().putString(preferenceKey,encoded).commit()){"configuration save failed"}
            }
        }
        var replaying=false
        val nestedReplayCallers=java.util.ArrayDeque<Pair<Long,Boolean>>()
        var lastNativeChildReturn=0L
        val clearKey="runtime_clear_${gameLocation}_${game}"
        val probeSuffix=intent.getStringExtra("read_history_probe").let{if(it==null)"" else "_probe_$it"}
        val readKey="runtime_read_v1_${gameLocation}_${game}"+probeSuffix
        val progressKey="runtime_unlocks_v1_${gameLocation}_${game}"+probeSuffix
        var savedProgress=getPreferences(MODE_PRIVATE).getString(progressKey,null)
        fun persistUnlocks(){
            if(replaying || !titleFirst)return
            val bytes=story(intArrayOf(),62,0)
            check(bytes.size>=5 && bytes.copyOfRange(0,5).contentEquals(byteArrayOf(78,75,85,76,1))){String(bytes,Charsets.UTF_8)}
            if(bytes.size==5)return
            val encoded=android.util.Base64.encodeToString(bytes,android.util.Base64.NO_WRAP)
            if(savedProgress!=encoded){
                check(getPreferences(MODE_PRIVATE).edit().putString(progressKey,encoded).commit()){"unlock progress save failed"}
                savedProgress=encoded
            }
        }
        var skipEnabled=false
        var autoEnabled=false
        var dialogueHidden=false
        var hideBusy=false
        var classicStoryMenu=true
        var menuOpen=false
        var menuBusy=false

        var sequence=0L
        var selectedFiles:List<Pair<String,Uri>> = emptyList()

        lateinit var show:(ByteArray)->Unit
        lateinit var closeOriginalGame:()->Unit
        lateinit var dialogue:DialoguePresentation
        var systemEffects = false
        var confirmActions = false
        var actionConfirmations:JSONObject? = null
        // Original ViewDialog skips only an explicitly disabled table entry.
        fun confirmAction(name:String) = actionConfirmations?.optBoolean(name,true) ?: confirmActions
        var refreshShortcutIcons: () -> Unit = {}
        fun applyPresentation(config:JSONObject) {
            val mode = config.optInt("Screen")
            val scale = when(mode) {
                1 -> ImageView.ScaleType.FIT_XY
                2 -> ImageView.ScaleType.CENTER_CROP
                else -> ImageView.ScaleType.FIT_CENTER
            }
            image.scaleType=scale
            overlay.scaleType=scale
            dialogueView.screenMode=mode
            systemEffects=config.optInt("SysEffect",0)!=0
            actionConfirmations=config.optJSONObject("Dialog")
            confirmActions=actionConfirmations==null && config.optInt("Dialog",0)!=0
            storyEffects?.sceneEffects=config.optInt("Effect",1)!=0
            if(diagnostics)File(root,"presentation-events.jsonl").appendText(
                JSONObject().put("event","applied").put("screen",mode).put("scene_scale",scale.name)
                    .put("system_effects",systemEffects).put("confirm",confirmActions).toString()+"\n")
        }
        fun presentSystem(view:TitleView, bitmap:android.graphics.Bitmap, transition:Boolean) {
            view.present(bitmap)
            if(!transition)return
            view.animate().cancel()
            view.alpha=if(systemEffects)0f else 1f
            if(systemEffects)view.animate().alpha(1f).setDuration(150).start()
            if(diagnostics)File(root,"presentation-events.jsonl").appendText(
                JSONObject().put("event","system_transition").put("animated",systemEffects)
                    .put("view",view.contentDescription.toString()).toString()+"\n")
        }
        fun confirmOriginal(message:String,enabled:Boolean=confirmActions,positive:String="确定",action:()->Unit) {
            if(!enabled){action();return}
            if(originalConfirmOpen)return
            originalConfirmOpen=true
            val resumeAfterDialog=!timeline.paused
            if(resumeAfterDialog)timeline.pause()
            var accepted=false
            val dialog=android.app.AlertDialog.Builder(this).setMessage(message)
                .setNegativeButton("取消",null).setPositiveButton(positive){_,_->accepted=true}.create()
            dialog.setOnDismissListener{
                originalConfirmOpen=false
                if(!storyClosing&&!isFinishing){
                    if(resumeAfterDialog&&foreground)timeline.resume()
                    if(accepted)action()
                }
            }
            dialog.window?.addFlags(android.view.WindowManager.LayoutParams.FLAG_FULLSCREEN)
            dialog.show()
        }
        var textConfig:JSONObject?=null
        fun applyTextConfig(bytes:ByteArray,scheduler:SessionTimeline=timeline){
            persistConfig()
            val config=JSONObject(String(bytes,Charsets.UTF_8));check(!config.has("error")){config.optString("error")}
            runOnUiThread{scheduler.post{if(!storyClosing){
                applyPresentation(config)
                textConfig=config
                rightMouseAction=when(config.getInt("right_mouse")){
                    1->if(classicStoryMenu)StoryAction.MENU else StoryAction.CONFIG
                    2->StoryAction.CONFIG
                    else->StoryAction.HIDE
                }
                textSettings.isEnabled=true
                File(root,"text-config-events.jsonl").appendText(config.toString()+"\n")
            }}}
        }
        var audioConfig:JSONObject?=null
        var lastStoryAudioConfig:ByteArray?=null
        fun applyAudioConfig(bytes:ByteArray,scheduler:SessionTimeline=timeline) {
            persistConfig()
            val config=JSONObject(String(bytes,Charsets.UTF_8));check(!config.has("error")){config.optString("error")}
            lastStoryAudioConfig=bytes.copyOf()
            if(volumeOverride>=0)config.put("host_master",volumeOverride)
            runOnUiThread{scheduler.post{if(!storyClosing){
                storyEffects!!.configureAudio(config);audioConfig=config;settings.isEnabled=true
                File(root,"audio-config-events.jsonl").appendText(config.toString()+"\n")
            }}}
        }
        fun fail(error:Throwable) {
            if(storyClosing)return
            storyClosing=true;systemCommandPresentation?.finish()
            File(root,"error.txt").writeText(error.stackTraceToString())
            if(storyLoaded)story(intArrayOf(),3,0)
            runOnUiThread{movieHost?.close();storyEffects?.close();if(!isDestroyed){text.visibility=android.view.View.VISIBLE;text.text="无法继续：${error.message}";next.isEnabled=false}}
        }
        lifecycleFailure={error->worker.execute{fail(error)}}
        val commandPresentation=SystemCommandPresentation(timeline,{bitmap->image.setImageBitmap(bitmap)}){token,elapsed,event->
            val musicPlaying=storyEffects?.musicPlaying()==true
            worker.execute{try{if(!storyClosing){
                val feedback=JSONObject(String(story(intArrayOf(if(musicPlaying)1 else 0),81,token),Charsets.UTF_8))
                check(feedback.optBoolean("updated")){feedback.toString()}
                show(story(event,75,token,elapsed))
            }}catch(error:Throwable){fail(error)}}
        }
        systemCommandPresentation=commandPresentation
        dialogue=DialoguePresentation(dialogueView,timeline,root,traceDialogue,{op,token,elapsed,painted,done->
            val frameOp=if(storyEffects!!.voiceFinished())op+16 else op
            worker.execute{try{if(!storyClosing){
                val state=JSONObject(String(story(intArrayOf(),frameOp,token,elapsed),Charsets.UTF_8));check(!state.has("error")){state.optString("error")}
                val advancing=state.getBoolean("advance")
                if(!replaying&&advancing&&(!routeReplay||token%100==0L)){
                    val history=story(intArrayOf(),31,0)
                    check(history.size>=5 && history.copyOfRange(0,5).contentEquals(byteArrayOf(78,75,82,68,1))){String(history,Charsets.UTF_8)}
                    check(getPreferences(MODE_PRIVATE).edit().putString(readKey,android.util.Base64.encodeToString(history,android.util.Base64.NO_WRAP)).commit()){"read history save failed"}
                }
                val visible=state.getLong("visible")
                val (bitmap,geometry)=if((advancing&&!legacyControls)||(visible==painted&&!state.optBoolean("message_animating"))){
                    null to JSONObject()
                }else{
                    val frame=story(intArrayOf(),16,token,if(legacyControls)0 else 1)
                    val split=frame.indexOf(10.toByte());check(split>=0){String(frame,Charsets.UTF_8)}
                    val geometry=JSONObject(String(frame,0,split,Charsets.UTF_8))
                    if(traceDialogue)File(root,"dialogue-$token.json").writeText(geometry.toString())
                    val bitmap=if(legacyControls){
                        val png=frame.copyOfRange(split+1,frame.size)
                        File(root,"dialogue-$token.json").writeText(geometry.toString())
                        if(visible==0L)File(root,"dialogue-$token-empty.png").writeBytes(png)
                        if(state.getBoolean("complete"))File(root,"dialogue-$token.png").writeBytes(png)
                        BitmapFactory.decodeByteArray(png,0,png.size) ?: error("invalid dialogue overlay")
                    }else DialogueView.decodeRgba(frame,split+1,geometry)
                    bitmap to geometry
                }
                runOnUiThread{timeline.post(cancel={bitmap?.recycle()}){if(!storyClosing){text.visibility=android.view.View.GONE;skipEnabled=state.getBoolean("skip");skip.text=if(skipEnabled)"快进：开" else "快进：关";autoEnabled=state.getBoolean("auto");auto.text=if(autoEnabled)"自动：开" else "自动：关";done(state,bitmap,geometry)}else bitmap?.recycle()}}
            }}catch(error:Throwable){fail(error)}}
        },{enabled->menu.isEnabled=enabled&&classicStoryMenu&&!storyConfigOpen&&!hideBusy&&!dialogueHidden&&!menuOpen;hide.isEnabled=enabled&&!storyConfigOpen&&!hideBusy&&!dialogueHidden;skip.isEnabled=(enabled||skipEnabled)&&!storyConfigOpen;auto.isEnabled=(enabled||autoEnabled)&&!storyConfigOpen;refreshShortcutIcons();next.isEnabled=enabled&&!(sequence==13L&&intent.getBooleanExtra("audio_state_probe",false));originalSettings.isEnabled=enabled&&!storyConfigOpen
            if(originalSettings.isEnabled&&dialogue.visibleCount>0&&sequence==intent.getLongExtra("settings_probe_sequence",-1)&&!intent.getBooleanExtra("settings_probe_used",false)){
                intent.putExtra("settings_probe_used",true);originalSettings.performClick()
            }
            if(menu.isEnabled&&dialogue.visibleCount>0&&sequence==intent.getLongExtra("menu_probe_sequence",-1)&&!intent.getBooleanExtra("menu_probe_used",false)){
                intent.putExtra("menu_probe_used",true);menu.performClick()
            }
            if(hide.isEnabled&&dialogue.visibleCount>0&&sequence==intent.getLongExtra("hide_probe_sequence",-1)&&!intent.getBooleanExtra("hide_probe_used",false)){
                intent.putExtra("hide_probe_used",true);hide.performClick()
            }
        },{stopVoice->
            storyEffects?.stopMotion();storyEffects?.finishDialogue(sequence,stopVoice);val token=sequence
            worker.execute{try{
                val nextPacket=story(intArrayOf(),2,token)
                if(routeReplay&&skipEnabled){
                    val headerEnd=nextPacket.indexOf(10.toByte());check(headerEnd>=0)
                    val header=JSONObject(String(nextPacket,0,headerEnd,Charsets.UTF_8))
                    if(header.has("sequence")&&!header.has("motion")){
                        val batch=story(intArrayOf(),58,0)
                        val split=batch.indexOf(10.toByte());check(split>=0)
                        val info=JSONObject(String(batch,0,split,Charsets.UTF_8));check(!info.has("error")){info.optString("error")}
                        val pages=info.getJSONArray("pages")
                        for(i in 0 until pages.length()){
                            val page=pages.getJSONObject(i);val seq=page.getLong("sequence")
                            File(root,"page-$seq.json").writeText(page.toString())
                        }
                        show(batch.copyOfRange(split+1,batch.size))
                    }else show(nextPacket)
                }else show(nextPacket)
            }catch(error:Throwable){fail(error)}}
        })
        auto.setOnClickListener {
            val token=sequence;val enabled=!autoEnabled
            if(enabled)dialogue.suspend()
            auto.isEnabled=false;next.isEnabled=false;originalSettings.isEnabled=false
            worker.execute{try{
                val state=JSONObject(String(story(intArrayOf(),if(enabled)26 else 25,token),Charsets.UTF_8))
                check(!state.has("error")){state.optString("error")}
                runOnUiThread{timeline.post{if(!storyClosing){autoEnabled=state.getBoolean("auto");auto.text=if(autoEnabled)"自动：开" else "自动：关";if(enabled)dialogue.resume()}}}
            }catch(error:Throwable){fail(error)}}
        }
        fun toggleSkip(){
            val enabled=!skipEnabled;val token=sequence
            if(enabled)dialogue.suspend()
            skip.isEnabled=false;auto.isEnabled=false;next.isEnabled=false;originalSettings.isEnabled=false
            worker.execute{try{
                val state=JSONObject(String(story(intArrayOf(),if(enabled)30 else 29,token),Charsets.UTF_8))
                check(!state.has("error")){state.optString("error")}
                File(root,"skip-controls.jsonl").appendText(state.toString()+"\n")
                runOnUiThread{timeline.post{if(!storyClosing){skipEnabled=state.getBoolean("skip");skip.text=if(skipEnabled)"快进：开" else "快进：关";autoEnabled=state.getBoolean("auto");auto.text=if(autoEnabled)"自动：开" else "自动：关";if(enabled)dialogue.resume()}}}
            }catch(error:Throwable){fail(error)}}
        }
        skip.setOnClickListener{confirmOriginal("开始快进？",confirmActions&&!skipEnabled,action=::toggleSkip)}
        val hiddenView=ImageView(this);hiddenView.scaleType=ImageView.ScaleType.FIT_XY;hiddenView.setBackgroundColor(android.graphics.Color.BLACK)
        hiddenView.contentDescription="隐藏对话框；点击恢复"
        fun hiddenState(state:JSONObject){
            val snapshot=File(root,"hide-state.tmp");snapshot.writeText(state.toString())
            check(snapshot.renameTo(File(root,"hide-state.json"))){"cannot publish hide state"}
            File(root,"hide-events.jsonl").appendText(state.toString()+"\n")
        }
        fun restoreDialogue(){
            if(hideBusy||!dialogueHidden||storyClosing)return
            hideBusy=true;val token=sequence
            worker.execute{try{
                val state=JSONObject(String(story(intArrayOf(),35,token),Charsets.UTF_8));check(!state.has("error")){state.optString("error")}
                check(story(intArrayOf(),1,0).contentEquals(File(root,"hide-checkpoint.bin").readBytes())){"hidden dialogue advanced"}
                runOnUiThread{timeline.post{if(!storyClosing){
                    stage.removeView(hiddenView);hiddenView.setImageDrawable(null)
                    dialogueHidden=false;hideBusy=false;autoEnabled=false;skipEnabled=false;auto.text="自动：关";skip.text="快进：关"
                    dialogueView.visibility=android.view.View.VISIBLE;controls.visibility=android.view.View.VISIBLE
                    storyEffects!!.lifecycleAudio("dialogue_restored");hiddenState(state);dialogue.resume()
                }}}
            }catch(error:Throwable){fail(error)}}
        }
        hide.setOnClickListener {
            hideBusy=true;dialogue.suspend();controls.visibility=android.view.View.INVISIBLE;val token=sequence
            val snapshot=android.graphics.Bitmap.createBitmap(image.width,image.height,android.graphics.Bitmap.Config.ARGB_8888)
            image.draw(android.graphics.Canvas(snapshot))
            worker.execute{try{
                val state=JSONObject(String(story(intArrayOf(),34,token),Charsets.UTF_8));check(!state.has("error")){state.optString("error")}
                File(root,"hide-checkpoint.bin").writeBytes(story(intArrayOf(),1,0))
                if(diagnostics)File(root,"hide-base.png").outputStream().use{check(snapshot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it))}
                runOnUiThread{timeline.post{if(!storyClosing){
                    dialogueHidden=true;hideBusy=false;dialogueView.visibility=android.view.View.INVISIBLE;controls.visibility=android.view.View.INVISIBLE
                    hiddenView.setImageBitmap(snapshot);stage.addView(hiddenView,FrameLayout.LayoutParams(-1,-1))
                    storyEffects!!.lifecycleAudio("dialogue_hidden");hiddenState(state)
                }}}
            }catch(error:Throwable){fail(error)}}
        }
        hiddenView.setOnClickListener{restoreDialogue()}
        hiddenBack={if(!storyClosing&&(dialogueHidden||hideBusy)){restoreDialogue();true}else false}
        var uiAudioToken=-1000L
        fun playUiAudio(packet:ByteArray,afterStartVolume:Int?=null){
            val split=packet.indexOf(10.toByte());check(split>=0){String(packet,Charsets.UTF_8)}
            val meta=JSONObject(String(packet,0,split,Charsets.UTF_8));check(!meta.has("error")){meta.toString()}
            if(meta.optBoolean("audio_unavailable"))return
            val token=--uiAudioToken
            meta.put("token",token).put("ui_audio",true)
            if(afterStartVolume!=null)meta.put("restore_category_percent",afterStartVolume)
            val bytes=packet.copyOfRange(split+1,packet.size);val file=File(root,"audio-$token.ogg");file.writeBytes(bytes)
            val digest=java.security.MessageDigest.getInstance("SHA-256").digest(bytes).joinToString(""){"%02x".format(it.toInt() and 255)}
            File(root,"audio-$token.json").writeText(JSONObject().put("kind",meta.getString("host_effect")).put("name",meta.getString("name")).put("sha256",digest).toString())
            File(root,"effect-$token.json").writeText(meta.toString())
            val clip=try{ShortClip.read(file)}catch(error:Exception){file.delete();throw error}
            runOnUiThread{menuTimeline.post(cancel={file.delete()}){if(storyClosing){file.delete()}else{
                storyEffects!!.run(meta,null,file,clip){File(root,"ui-audio-events.jsonl").appendText(meta.toString()+"\n")}
            }}}
        }
        fun playOriginalRoomAudio(packet:ByteArray){
            val split=packet.indexOf(10.toByte());check(split>=0)
            val meta=JSONObject(String(packet,0,split,Charsets.UTF_8));check(!meta.has("error")){meta.toString()}
            val token=--uiAudioToken;meta.put("token",token).put("ui_audio",true)
            val kind=meta.getString("host_effect")
            val file=if(kind=="bgm")File(root,"audio-$token.ogg").also{it.writeBytes(packet.copyOfRange(split+1,packet.size))}else null
            if(traceDialogue){
                if(file!=null)meta.put("sha256",java.security.MessageDigest.getInstance("SHA-256").digest(file.readBytes()).joinToString(""){"%02x".format(it.toInt() and 255)})
                File(root,"room-native-audio.jsonl").appendText(meta.toString()+"\n")
            }
            runOnUiThread{timeline.post(cancel={file?.delete()}){if(storyClosing){file?.delete()}else{
                storyEffects!!.run(meta,null,file,null){if(traceDialogue)File(root,"room-native-audio-complete.jsonl").appendText(meta.toString()+"\n")}
            }}}
        }
        var lastMenuAudioConfig=""
        fun playSystemSounds(meta:JSONObject){
            val config=meta.optJSONObject("audio_config")
            val volume=config?.getInt("system")
            if(config!=null){
                if(volumeOverride>=0)config.put("host_master",volumeOverride)
                val encoded=config.toString()
                if(encoded!=lastMenuAudioConfig){
                    lastMenuAudioConfig=encoded
                    runOnUiThread{menuTimeline.post{if(!storyClosing){
                        storyEffects!!.configureAudio(config)
                        File(root,"audio-config-events.jsonl").appendText(encoded+"\n")
                    }}}
                }
            }
            val cues=meta.optJSONArray("system_sounds")
            if(cues==null||cues.length()==0){
                if(volume!=null)runOnUiThread{menuTimeline.post{if(!storyClosing)storyEffects!!.configureCategory("system",volume)}}
                return
            }
            for(i in 0 until cues.length()){
                val cue=cues.getJSONObject(i)
                playUiAudio(story(intArrayOf(cue.getInt("category_percent")),56,cue.getLong("id")),volume)
            }
        }
        fun playRoomMusic(packet:ByteArray){
            val split=packet.indexOf(10.toByte());check(split>=0){String(packet,Charsets.UTF_8)}
            val meta=JSONObject(String(packet,0,split,Charsets.UTF_8))
            check(!meta.has("error")){meta.toString()}
            if(meta.getString("host_effect")=="bgm"){
                playUiAudio(packet)
            }else{
                check(split==packet.size-1){"unexpected room music control payload"}
                meta.put("token",--uiAudioToken).put("ui_audio",true)
                runOnUiThread{menuTimeline.post{if(!storyClosing){
                    storyEffects!!.run(meta,null,null){File(root,"room-music-events.jsonl").appendText(meta.toString()+"\n")}
                }}}
            }
        }
        fun openLog(){
            storyLogOpen=true;dialogue.suspend();timeline.pause();controls.visibility=android.view.View.GONE;dialogueView.visibility=android.view.View.INVISIBLE
            autoEnabled=false;skipEnabled=false;auto.text="自动：关";skip.text="快进：关"
            storyEffects!!.lifecycleAudio("log_open")
            val token=sequence;var busy=true
            lateinit var view:TitleView
            val back=Button(this);back.text="返回剧情";if(legacyControls)layout.addView(back)
            fun closeLog(){if(!busy){busy=true;view.accepting=false;worker.execute{try{
                val result=JSONObject(String(story(intArrayOf(),53,token),Charsets.UTF_8));check(result.optString("kind")=="log_closed"){result.toString()}
                playUiAudio(story(intArrayOf(),56,result.getLong("system_sound")))
                check(story(intArrayOf(),1,0).contentEquals(File(root,"log-checkpoint.bin").readBytes())){"log advanced story"}
                runOnUiThread{menuTimeline.post{if(!storyClosing){
                    stage.removeView(view);layout.removeView(back);storyLogOpen=false;storyLogBack=null
                    controls.visibility=android.view.View.VISIBLE;dialogueView.visibility=android.view.View.VISIBLE;timeline.resume();dialogue.resume()
                    storyEffects!!.lifecycleAudio("log_closed");File(root,"log-events.jsonl").appendText(result.toString()+"\n")
                }}}
            }catch(error:Throwable){fail(error)}}}}
            fun present(packet:ByteArray,unlock:Boolean){
                val split=packet.indexOf(10.toByte());check(split>=0){String(packet,Charsets.UTF_8)}
                val meta=JSONObject(String(packet,0,split,Charsets.UTF_8));val png=packet.copyOfRange(split+1,packet.size)
                val sound=meta.getLong("system_sound");if(sound!=0L)playUiAudio(story(intArrayOf(),56,sound))
                val bitmap=BitmapFactory.decodeByteArray(png,0,png.size) ?: error("invalid message log bitmap")
                if(diagnostics)File(root,"message-log.png").writeBytes(png)
                runOnUiThread{menuTimeline.post(cancel={bitmap.recycle()}){if(!storyClosing){
                    presentSystem(view,bitmap,!view.hasFrame);if(unlock){busy=false;view.accepting=true}
                    meta.put("story_sequence",token).put("accepting",view.accepting)
                    val tmp=File(root,"message-log-presented.tmp");tmp.writeText(meta.toString());check(tmp.renameTo(File(root,"message-log-presented.json")))
                    if(meta.getInt("replay")>0){val index=meta.getLong("replay");worker.execute{try{playUiAudio(story(intArrayOf(),55,index))}catch(error:Throwable){fail(error)}}}
                }else bitmap.recycle()}}
            }
            view=TitleView(this){kind,x,y->if(!storyClosing){if(kind==2)busy=true;worker.execute{try{present(story(intArrayOf(kind,x,y),52,token),kind==2)}catch(error:Throwable){fail(error)}}}}
            view.contentDescription="原版消息日志";view.accepting=false;stage.addView(view,FrameLayout.LayoutParams(-1,-1))
            back.setOnClickListener{closeLog()};storyLogBack={if(storyLogOpen){closeLog();true}else false}
            worker.execute{try{File(root,"log-checkpoint.bin").writeBytes(story(intArrayOf(),1,0));present(story(intArrayOf(),51,token),true)}catch(error:Throwable){fail(error)}}
        }
        var chineseScenario=false
        fun saveDirectory():File {
            val storage=if(intent.getBooleanExtra("library_game_storage",false))gameLocation else tree
            val probe=intent.getStringExtra("save_probe")
            if(probe!=null)check(Regex("[a-zA-Z0-9_-]{1,64}").matches(probe))
            val directory=LibraryPaths.saveDirectory(filesDir,storage,game,chineseScenario,probe)
            check(directory.isDirectory||directory.mkdirs())
            return directory
        }
        lateinit var titleView:TitleView
        lateinit var returnTitleFromSave:(ByteArray)->Unit
        lateinit var enterLoadedStory:(ByteArray)->Unit
        fun openSaveSlots(initialSave:Boolean,fromTitle:Boolean=false,quick:Boolean=false,resume:Int=0,confirmed:Boolean=false){
            if(quick&&(!initialSave||actionConfirmations!=null)&&confirmAction(if(initialSave)"QSAVE" else "QLOAD")&&!confirmed){
                confirmOriginal(if(initialSave)"保存快捷存档？" else "读取快捷存档？",true){openSaveSlots(initialSave,fromTitle,quick,resume,true)}
                return
            }
            if(replaying){
                android.widget.Toast.makeText(this,"回想播放期间无法存读档",android.widget.Toast.LENGTH_SHORT).show()
                return
            }
            storySaveOpen=true
            if(fromTitle)titleView.accepting=false else dialogue.suspend()
            timeline.pause();controls.visibility=android.view.View.GONE
            val directory=saveDirectory()
            val quickSlots=QuickSlots(directory)
            var saving=initialSave;var page=0;var busy=true
            var visibleSlotIds=emptyList<Int>()
            val cached=mutableMapOf<Int,Pair<Long,Long>>()
            lateinit var view:TitleView
            lateinit var present:(ByteArray,Boolean)->Unit
            fun slotFile(id:Int):File?=if(id>100)quickSlots.slot(id-100) else File(directory,"slot-%03d.nksv".format(id))
            fun publish(event:String,id:Int=0,extra:JSONObject=JSONObject()){
                val value=extra.put("event",event).put("slot",id).put("page",page).put("saving",saving).put("from_title",fromTitle).put("sequence",sequence).put("directory",directory.name)
                val tmp=File(root,"save-state.tmp");tmp.writeText(value.toString());check(tmp.renameTo(File(root,"save-state.json")))
                File(root,"save-events.jsonl").appendText(value.toString()+"\n")
            }
            fun failure(error:Exception){
                if(fromTitle&&resume>0)worker.execute{try{present(story(intArrayOf(),47,0),true)}catch(fatal:Throwable){fail(fatal)}}
                if(quick)worker.execute{try{story(intArrayOf(),43,0);runOnUiThread{menuTimeline.post{
                    stage.removeView(view);storySaveOpen=false;storySaveBack=null;timeline.resume();storyEffects!!.resume()
                    controls.visibility=android.view.View.VISIBLE;dialogue.resume()
                }}}catch(fatal:Throwable){fail(fatal)}}
                runOnUiThread{menuTimeline.post{if(!storyClosing){busy=false;view.accepting=true;publish("error",0,JSONObject().put("message",error.message));android.widget.Toast.makeText(this,"存读档失败：${error.message}",android.widget.Toast.LENGTH_LONG).show()}}}
            }
            fun refresh():ByteArray {
                for(id in visibleSlotIds){
                    val file=slotFile(id);val signature=(file?.lastModified() ?: 0L) to (file?.length() ?: 0L)
                    if(cached[id]!=signature){
                        val bytes=if(file!=null && file.isFile){
                            val stamp=java.text.SimpleDateFormat("yyyy/MM/dd H:mm",java.util.Locale.ROOT).format(java.util.Date(file.lastModified())).toByteArray(Charsets.UTF_8)
                            ParcelFileDescriptor.open(file,ParcelFileDescriptor.MODE_READ_ONLY).use{fd->story(intArrayOf(fd.fd)+stamp.map{it.toInt() and 255}.toIntArray(),46,id.toLong())}
                        }else story(intArrayOf(),46,id.toLong())
                        val result=JSONObject(String(bytes,Charsets.UTF_8));check(result.optBoolean("updated")){result.optString("error",result.toString())};cached[id]=signature
                    }
                }
                return story(intArrayOf(),47,0)
            }
            fun leave(){stage.removeView(view);storySaveOpen=false;storySaveBack=null;timeline.resume();storyEffects!!.resume()}
            fun cancelSlots(){if(!busy){busy=true;view.accepting=false;worker.execute{try{
                val packet=story(intArrayOf(),43,0)
                if(fromTitle){returnTitleFromSave(packet);runOnUiThread{menuTimeline.post{publish("cancelled");leave()}}}
                else{val result=JSONObject(String(packet,Charsets.UTF_8));check(result.optBoolean("closed")){result.toString()};runOnUiThread{menuTimeline.post{publish("cancelled");leave();controls.visibility=android.view.View.VISIBLE;dialogue.resume()}}}
            }catch(error:Throwable){fail(error)}}}}
            fun operate(id:Int){
                busy=true;view.accepting=false;val saveMode=saving
                worker.execute{try{
                    playUiAudio(story(intArrayOf(),54,if(quick){if(saveMode)0L else 1L}else{if(saveMode)2L else 3L}))
                    val result=if(saveMode){
                        var state=JSONObject()
                        fun export(file:File){
                            state=ParcelFileDescriptor.open(file,ParcelFileDescriptor.MODE_CREATE or ParcelFileDescriptor.MODE_TRUNCATE or ParcelFileDescriptor.MODE_READ_WRITE).use{fd->JSONObject(String(story(intArrayOf(fd.fd),40,0),Charsets.UTF_8))}
                            check(state.optBoolean("saved")){state.optString("error",state.toString())}
                        }
                        if(quick)quickSlots.write(::export) else {
                            val temporary=File(directory,"slot-%03d.tmp".format(id));export(temporary)
                            Os.rename(temporary.absolutePath,slotFile(id)!!.absolutePath)
                        }
                        cached.remove(id);state
                    }else ParcelFileDescriptor.open(slotFile(id) ?: error("没有快捷存档"),ParcelFileDescriptor.MODE_READ_ONLY).use{fd->JSONObject(String(story(intArrayOf(fd.fd),41,0),Charsets.UTF_8))}.also{check(it.optBoolean("prepared")){it.optString("error",it.toString())}}
                    if(saveMode){
                        if(quick){
                            val closed=JSONObject(String(story(intArrayOf(),43,0),Charsets.UTF_8));check(closed.optBoolean("closed")){closed.toString()}
                            runOnUiThread{menuTimeline.post{publish("saved",id,result);leave();controls.visibility=android.view.View.VISIBLE;dialogue.resume();android.widget.Toast.makeText(this,"已快捷存档",android.widget.Toast.LENGTH_SHORT).show()}}
                        }else{val packet=refresh();present(packet,true);runOnUiThread{menuTimeline.post{publish("saved",id,result)}}}
                    }
                    else runOnUiThread{menuTimeline.post{if(!storyClosing){
                        dialogue.stop();storyEffects!!.resetForLoad();publish("prepared",id,result)
                        worker.execute{try{
                            val packet=story(intArrayOf(),42,0)
                            runOnUiThread{menuTimeline.post{leave();controls.visibility=android.view.View.VISIBLE;dialogueView.visibility=android.view.View.VISIBLE;publish("loaded",id)}}
                            if(fromTitle)enterLoadedStory(packet) else show(packet)
                        }catch(error:Throwable){fail(error)}}
                    }}}
                }catch(error:Exception){failure(error)}}
            }
            present={packet,unlock->
                val split=packet.indexOf(10.toByte());check(split>=0){String(packet,Charsets.UTF_8)}
                val meta=JSONObject(String(packet,0,split,Charsets.UTF_8));check(!meta.has("error")){meta.toString()}
                saving=meta.getBoolean("saving");page=meta.getInt("page")
                val ids=meta.getJSONArray("slot_ids");visibleSlotIds=(0 until ids.length()).map{ids.getInt(it)}
                if(meta.getBoolean("refresh")){present(refresh(),unlock)}else{
                    val png=packet.copyOfRange(split+1,packet.size);val bitmap=BitmapFactory.decodeByteArray(png,0,png.size) ?: error("invalid save UI bitmap")
                    if(diagnostics)File(root,"save-ui.png").writeBytes(png)
                    runOnUiThread{menuTimeline.post(cancel={bitmap.recycle()}){if(!storyClosing){
                        presentSystem(view,bitmap,!view.hasFrame);if(unlock){busy=false;view.accepting=true}
                        val tmp=File(root,"save-ui-presented.tmp");tmp.writeText(meta.put("accepting",unlock).toString());check(tmp.renameTo(File(root,"save-ui-presented.json")))
                        val action=meta.getInt("action")
                        when{action==-1->cancelSlots();action in visibleSlotIds->{
                                busy=false;view.accepting=true
                                val confirm=if(saving)confirmAction("OVERWRITE") && (actionConfirmations==null || slotFile(action)?.isFile==true) else confirmAction("LOAD")
                                confirmOriginal(if(saving)"保存到此存档？" else "读取此存档？",confirm){operate(action)}
                            }}
                    }else bitmap.recycle()}}
                }
            }
            view=TitleView(this){kind,x,y->if(!storyClosing){if(kind==2)busy=true;worker.execute{try{present(story(intArrayOf(kind,x,y),45,0),kind==2)}catch(error:Throwable){fail(error)}}}}
            view.contentDescription="原版存读档页";view.accepting=false;if(!quick)stage.addView(view,FrameLayout.LayoutParams(-1,-1))
            storySaveBack={if(storySaveOpen){cancelSlots();true}else false}
            val saveToken=sequence
            worker.execute{try{
                if(!fromTitle){
                    val ready=JSONObject(String(story(intArrayOf(),48,saveToken),Charsets.UTF_8))
                    if(!ready.optBoolean("ready")){
                        runOnUiThread{menuTimeline.post{publish("cancelled",0,ready);leave();controls.visibility=android.view.View.VISIBLE;dialogue.resume()}}
                        return@execute
                    }
                }
                if(quick){runOnUiThread{menuTimeline.post{autoEnabled=false;skipEnabled=false;auto.text="自动：关";skip.text="快进：关";publish("quick_open");operate(101)}}}
                else{
                    val packet=story(intArrayOf(),44,if(initialSave)1 else 0)
                    if(resume>0){
                        val info=JSONObject(String(packet,Charsets.UTF_8).substringBefore('\n'))
                        check(!info.has("error")){info.toString()}
                        runOnUiThread{menuTimeline.post{publish("open");operate(resume)}}
                    }else{present(packet,true);runOnUiThread{menuTimeline.post{publish("open")}}}
                }
            }catch(error:Throwable){fail(error)}}
        }
        var sidePresentSequence=0L
        lateinit var sideView:SideMenuView
        lateinit var shortcutReset: () -> Unit
        fun sideAvailable()=next.isEnabled&&!storyClosing&&!storySaveOpen&&!storyLogOpen&&!storyConfigOpen&&!menuOpen&&!dialogueHidden&&!autoEnabled&&!skipEnabled&&!timeline.paused
        fun navigateShortcut(back:Boolean,token:Long){
            next.isEnabled=false
            dialogue.stop()
            storyEffects!!.stopMotion()
            storyEffects!!.stopVoice()
            worker.execute{try{show(story(intArrayOf(),if(back)73 else 72,token))}catch(error:Throwable){fail(error)}}
        }
        fun presentSide(packet:ByteArray,token:Long){
            val split=packet.indexOf(10.toByte());check(split>=0){String(packet,Charsets.UTF_8)}
            val meta=JSONObject(String(packet,0,split,Charsets.UTF_8))
            check(!meta.has("error")){meta.getString("error")}
            val png=packet.copyOfRange(split+1,packet.size);val bitmap=BitmapFactory.decodeByteArray(png,0,png.size) ?: error("invalid side menu bitmap")
            if(diagnostics)File(root,"side-menu.png").writeBytes(png)
            runOnUiThread{timeline.post(cancel={bitmap.recycle()}){if(!storyClosing){
                sideView.present(bitmap,meta);meta.put("render_sequence",++sidePresentSequence).put("expanded",sideView.expanded).put("icons_visible",sideView.iconsVisible())
                val tmp=File(root,"side-menu-presented.tmp");tmp.writeText(meta.toString());check(tmp.renameTo(File(root,"side-menu-presented.json")))
                val action=meta.getString("action")
                if(action.isNotEmpty()&&sequence==token&&sideAvailable()){
                    sideView.collapse()
                    when(action){
                        "NEXT"->confirmOriginal("跳到下一个选项？",confirmAction("NEXT")){navigateShortcut(false,token)}
                        "BACK"->confirmOriginal("返回上一个选项？",confirmAction("BACK")){navigateShortcut(true,token)}
                        "QSAVE"->openSaveSlots(true,quick=true)
                        "QLOAD"->openSaveSlots(false,quick=true)
                        "SAVE"->openSaveSlots(true)
                        "LOAD"->openSaveSlots(false)
                        "AUTO"->auto.performClick()
                        "SKIP"->skip.performClick()
                        "CONFIG"->originalSettings.performClick()
                        "LOG"->openLog()
                        "HIDE"->hide.performClick()
                        "RESET"->confirmOriginal("返回标题？",confirmAction("RESET")){worker.execute{try{
                            val reset=JSONObject(String(story(intArrayOf(),64,token),Charsets.UTF_8))
                            check(reset.optBoolean("reset")){reset.toString()}
                            runOnUiThread{timeline.post{if(!storyClosing)shortcutReset()}}
                        }catch(error:Throwable){fail(error)}}}
                        "VOICE"->worker.execute{try{playUiAudio(story(intArrayOf(),65,token))}catch(error:Throwable){fail(error)}}
                        "QUIT"->close.performClick()
                    }
                    File(root,"side-menu-events.jsonl").appendText(meta.put("story_sequence",token).toString()+"\n")
                }
            }else bitmap.recycle()}}
        }
        fun loadSide(){
            val file=QuickSlots(saveDirectory()).slot(1)
            val packet=if(file!=null&&file.isFile)ParcelFileDescriptor.open(file,ParcelFileDescriptor.MODE_READ_ONLY).use{fd->story(intArrayOf(fd.fd),49,0)}else story(intArrayOf(),49,0)
            presentSide(packet,sequence)
        }
        sideView=SideMenuView(this,::sideAvailable,{if(!storyClosing)worker.execute{try{loadSide()}catch(error:Throwable){fail(error)}}},{kind,x,y->
            if(!storyClosing){val token=sequence;worker.execute{try{presentSide(story(intArrayOf(kind,x,y),50,token),token)}catch(error:Throwable){fail(error)}}}
        })
        stage.addView(sideView,FrameLayout.LayoutParams(-1,-1))
        fun returnToTitle(){
            storyClosing=true;systemCommandPresentation?.finish();shortcutKey=null;storyMenuBack=null;hiddenBack=null;settingsBack=null
            controls.visibility=android.view.View.GONE;dialogue.stop();movieHost?.close()
            storyEffects!!.lifecycleAudio("return_title_before_close");storyEffects!!.close()
            val audioClosed=storyEffects!!.cleanupState()
            val childRun="story_restart_"+java.util.UUID.randomUUID().toString().replace("-","")
            text.visibility=android.view.View.VISIBLE;text.text="正在返回标题…"
            worker.execute{try{
                check(audioClosed.getBoolean("closed") && audioClosed.getInt("players")==0 && audioClosed.getInt("audio_cache_files")==0){"audio cleanup incomplete"}
                persistConfig()
                if(!replaying){
                    val history=story(intArrayOf(),31,0)
                    check(history.size>=5 && history.copyOfRange(0,5).contentEquals(byteArrayOf(78,75,82,68,1))){String(history,Charsets.UTF_8)}
                    check(getPreferences(MODE_PRIVATE).edit().putString(readKey,android.util.Base64.encodeToString(history,android.util.Base64.NO_WRAP)).commit())
                }
                val nativeClosed=JSONObject(String(story(intArrayOf(),39,0),Charsets.UTF_8))
                check(nativeClosed.optBoolean("closed") && nativeClosed.getLong("vm_live_bytes")==0L){nativeClosed.toString()}
                storyLoaded=false
                val report=JSONObject().put("sequence",sequence).put("old_instance",instanceId).put("new_run",childRun)
                    .put("native",nativeClosed).put("audio",audioClosed).put("closed_at_ms",android.os.SystemClock.elapsedRealtime())
                val tmp=File(root,"return-title.tmp");tmp.writeText(report.toString());check(tmp.renameTo(File(root,"return-title.json")))
                runOnUiThread{menuTimeline.post{if(!isDestroyed&&!isFinishing){
                    val nextTitle=Intent(this,MainActivity::class.java).putExtra("title_game",game).putExtra("run_id",childRun).putExtra("restart_parent",runId)
                        .putExtra("lifecycle_probe",intent.getBooleanExtra("lifecycle_probe",false))
                        .putExtra("route_replay",routeReplay)
                    intent.getStringExtra("read_history_probe")?.let{nextTitle.putExtra("read_history_probe",it)}
                    intent.getStringExtra("library_tree")?.let { nextTitle.putExtra("library_tree", it) }
                    libraryDirectory?.let { nextTitle.putExtra("library_directory", it) }
                    nextTitle.putExtra("library_game_storage",intent.getBooleanExtra("library_game_storage",false))
                    libraryEntry?.let{nextTitle.putExtra("library_entry",it)}
                    nextTitle.putExtra("library_original",originalVersion)
                    if(replaying)nextTitle.putExtra("reopen_replay",true)
                    startActivity(nextTitle);finish()
                }}}
            }catch(error:Throwable){fail(error)}}
        }
        shortcutReset=::returnToTitle
        refreshShortcutIcons={sideView.invalidate()}
        lateinit var storyMenuView:TitleView
        fun presentStoryMenu(packet:ByteArray,unlock:Boolean,inputKind:Int=-1){
            val split=packet.indexOf(10.toByte());check(split>=0){String(packet,Charsets.UTF_8)}
            val meta=JSONObject(String(packet,0,split,Charsets.UTF_8));check(!meta.has("error")){meta.optString("error")}
            playSystemSounds(meta)
            meta.put("pointer_kind",inputKind)
            val closed=meta.getString("kind")=="story_menu_closed"
            if(closed)check(story(intArrayOf(),1,0).contentEquals(File(root,"menu-checkpoint.bin").readBytes())){"menu advanced dialogue"}
            val bitmap=if(closed)null else BitmapFactory.decodeByteArray(packet,split+1,packet.size-split-1) ?: error("invalid menu bitmap")
            if(diagnostics&&!closed)File(root,"story-menu.png").writeBytes(packet.copyOfRange(split+1,packet.size))
            runOnUiThread{timeline.post(cancel={bitmap?.recycle()}){if(!storyClosing){
                if(closed){
                    stage.removeView(storyMenuView);menuOpen=false;menuBusy=false
                    controls.visibility=android.view.View.VISIBLE;dialogueView.visibility=android.view.View.VISIBLE
                    autoEnabled=false;skipEnabled=false;auto.text="自动：关";skip.text="快进：关"
                    when(meta.getString("action")){
                        "CONFIG"->originalSettings.performClick()
                        "SAVE"->openSaveSlots(true)
                        "LOAD"->openSaveSlots(false)
                        "LOG"->openLog()
                        "QSAVE"->openSaveSlots(true,quick=true)
                        "QLOAD"->openSaveSlots(false,quick=true)
                        "RESET"->confirmOriginal("返回标题？",confirmAction("RESET"),action=::returnToTitle)
                        "QUIT"->close.performClick()
                        else->dialogue.resume()
                    }
                }else{
                    presentSystem(storyMenuView,bitmap!!,!storyMenuView.hasFrame);if(unlock){storyMenuView.accepting=true;menuBusy=false}
                    if(meta.getString("unsupported").isNotEmpty())android.widget.Toast.makeText(this,"此菜单动作尚未接通："+meta.getString("unsupported"),android.widget.Toast.LENGTH_SHORT).show()
                }
                meta.put("accepting",storyMenuView.accepting)
                val tmp=File(root,"story-menu-presented.tmp");tmp.writeText(meta.toString());check(tmp.renameTo(File(root,"story-menu-presented.json")))
                File(root,"story-menu-events.jsonl").appendText(meta.toString()+"\n")
            }else bitmap?.recycle()}}
        }
        storyMenuView=TitleView(this){kind,x,y->
            if(!storyClosing){if(kind==2)menuBusy=true;val token=sequence;worker.execute{try{presentStoryMenu(story(intArrayOf(kind,x,y),37,token),kind==2,kind)}catch(error:Throwable){fail(error)}}}
        }
        storyMenuView.contentDescription="原版剧情菜单"
        menu.setOnClickListener{
            menuOpen=true;menuBusy=true;dialogue.suspend();controls.visibility=android.view.View.INVISIBLE;dialogueView.visibility=android.view.View.INVISIBLE
            storyMenuView.accepting=false;stage.addView(storyMenuView,FrameLayout.LayoutParams(-1,-1));val token=sequence
            worker.execute{try{File(root,"menu-checkpoint.bin").writeBytes(story(intArrayOf(),1,0));presentStoryMenu(story(intArrayOf(),36,token),true)}catch(error:Throwable){fail(error)}}
        }
        storyMenuBack={
            if(menuOpen&&!storyClosing){
                if(!menuBusy){menuBusy=true;storyMenuView.cancel();storyMenuView.accepting=false;val token=sequence;worker.execute{try{presentStoryMenu(story(intArrayOf(),38,token),true)}catch(error:Throwable){fail(error)}}}
                true
            }else false
        }
        var shortcutBusy=false
        shortcutKey={slot->
            if(!shortcutBusy&&!storyClosing&&!storyConfigOpen&&!dialogueHidden&&!hideBusy&&!menuOpen&&!storySaveOpen&&!storyLogOpen&&!timeline.paused&&sequence>0){
                shortcutBusy=true;val token=sequence
                worker.execute{try{
                    val result=JSONObject(String(story(intArrayOf(),33,token,slot.toLong()),Charsets.UTF_8))
                    check(!result.has("error")){result.optString("error")}
                    runOnUiThread{timeline.post{shortcutBusy=false;if(!storyClosing){
                        val status=result.getString("status");val action=result.getString("action")
                        if(storyConfigOpen||menuOpen||storySaveOpen||storyLogOpen||dialogueHidden||hideBusy)result.put("delivery","modal")
                        else if(status=="applied"){
                            autoEnabled=result.getBoolean("auto");skipEnabled=result.getBoolean("skip")
                            auto.text=if(autoEnabled)"自动：开" else "自动：关";skip.text=if(skipEnabled)"快进：开" else "快进：关"
                            dialogue.refresh();result.put("delivery","applied")
                        }else if(status=="request"){
                            if(action=="CONFIG"&&sequence==token&&originalSettings.isEnabled){originalSettings.performClick();result.put("delivery","settings")}
                            else if(action=="MENU"&&sequence==token&&menu.isEnabled){menu.performClick();result.put("delivery","menu")}
                            else if(action=="SKIP"&&sequence==token&&skip.isEnabled){skip.performClick();result.put("delivery","skip")}
                            else if(action=="HIDE"&&sequence==token&&hide.isEnabled){hide.performClick();result.put("delivery","hidden")}
                            else if((action=="QSAVE"||action=="QLOAD")&&sequence==token){openSaveSlots(action=="QSAVE",quick=true);result.put("delivery","quick")}
                            else if((action=="SAVE"||action=="LOAD")&&sequence==token){openSaveSlots(action=="SAVE");result.put("delivery","slots")}
                            else if(action=="RESET"&&sequence==token){
                                confirmOriginal("返回标题？",confirmAction("RESET")){worker.execute{try{
                                    val reset=JSONObject(String(story(intArrayOf(),64,token),Charsets.UTF_8))
                                    check(reset.optBoolean("reset")){reset.toString()}
                                    runOnUiThread{timeline.post{if(!storyClosing)shortcutReset()}}
                                }catch(error:Throwable){fail(error)}}}
                                result.put("delivery","reset")
                            }
                            else if(action=="NEXT"&&sequence==token){confirmOriginal("跳到下一个选项？",confirmAction("NEXT")){navigateShortcut(false,token)};result.put("delivery","next")}
                            else if(action=="BACK"&&sequence==token){confirmOriginal("返回上一个选项？",confirmAction("BACK")){navigateShortcut(true,token)};result.put("delivery","back")}
                            else if(action=="VOICE"&&sequence==token){worker.execute{try{playUiAudio(story(intArrayOf(),65,token))}catch(error:Throwable){fail(error)}};result.put("delivery","voice")}
                            else if(action=="LOG"&&sequence==token){openLog();result.put("delivery","log")}
                            else if(action=="QUIT"){close.performClick();result.put("delivery","quit")}
                            else result.put("delivery","busy")
                        }else {
                            result.put("delivery",status)
                            if(status=="unsupported")android.widget.Toast.makeText(this,"此快捷动作尚未接通：$action",android.widget.Toast.LENGTH_SHORT).show()
                        }
                        File(root,"shortcut-events.jsonl").appendText(result.toString()+"\n")
                    }}}
                }catch(error:Throwable){fail(error)}}
            }
        }
        storyEffects=StoryEffects(this,image,overlay,root,timeline,{kind,token,elapsed,ready->
            worker.execute{try{
                if(!storyClosing && (kind!="motion" || storyEffects!!.isMotionActive(token))){
                    val packet=story(intArrayOf(),if(kind=="motion")8 else if(kind=="flash")6 else 5,token,elapsed);val split=packet.indexOf(10.toByte());check(split>=0)
                    val meta=JSONObject(String(packet,0,split,Charsets.UTF_8));check(!meta.has("error")){meta.optString("error")}
                    check(meta.getString("format")=="RGBA8")
                    val width=meta.getInt("width");val height=meta.getInt("height")
                    val rgba=packet.copyOfRange(split+1,packet.size);check(rgba.size.toLong()==width.toLong()*height*4)
                    val pixels=IntArray(width*height)
                    for(index in pixels.indices){val at=index*4;pixels[index]=((rgba[at+3].toInt() and 255) shl 24) or ((rgba[at].toInt() and 255) shl 16) or ((rgba[at+1].toInt() and 255) shl 8) or (rgba[at+2].toInt() and 255)}
                    val bitmap=android.graphics.Bitmap.createBitmap(pixels,width,height,android.graphics.Bitmap.Config.ARGB_8888)
                    if(diagnostics&&!routeReplay&&kind!="staffroll")File(root,"${meta.optString("frame_kind","wipe")}-$token-$elapsed.rgba").writeBytes(rgba)
                    runOnUiThread{timeline.post(cancel={bitmap.recycle()}){if(!storyClosing){
                        val finished=if(kind=="motion")meta.getBoolean("finished") else false
                        ready(bitmap,finished)
                        if(kind=="motion"&&elapsed>0&&!finished&&intent.getBooleanExtra("close_on_motion",false)&&!intent.getBooleanExtra("motion_pause_used",false)&&(!intent.hasExtra("close_on_motion_sequence")||token==intent.getIntExtra("close_on_motion_sequence",0).toLong())){
                            intent.putExtra("motion_pause_used",true);moveTaskToBack(true)
                        }
                    }}}
                }
            }catch(error:Throwable){fail(error)}}
        },audioTimeline=menuTimeline,diagnostics=diagnostics){error->worker.execute{fail(error)}}
        show=show@{packet->
            if(storyClosing) Unit else {
                val split=packet.indexOf(10.toByte());check(split>=0)
                val metadata=JSONObject(String(packet,0,split,Charsets.UTF_8))
                if(metadata.has("error"))File(root,"boundary.json").writeText(metadata.toString())
                check(!metadata.has("error")){metadata.optString("error")}
                if(metadata.optBoolean("native_child_return")){
                    val returned=metadata.getLong("native_child_return_id")
                    if(returned!=lastNativeChildReturn){
                        val caller=nestedReplayCallers.removeLast()
                        check(caller.first==metadata.getLong("native_child_return_token")){"native replay caller mismatch"}
                        replaying=caller.second;lastNativeChildReturn=returned
                    }
                }
                if(traceDialogue)metadata.put("platform_replaying",replaying)
                val currentAudioConfig=story(intArrayOf(),9,0)
                if(lastStoryAudioConfig?.contentEquals(currentAudioConfig)!=true)applyAudioConfig(currentAudioConfig)
                persistUnlocks()
                while(true){
                    val sound=story(intArrayOf(),77,0)
                    val boundary=sound.indexOf(10.toByte());check(boundary>=0)
                    val soundMeta=JSONObject(String(sound,0,boundary,Charsets.UTF_8));check(!soundMeta.has("error")){soundMeta.toString()}
                    if(soundMeta.optBoolean("empty"))break
                    playUiAudio(sound)
                }
                while(true){
                    val roomAudio=story(intArrayOf(),80,0);val boundary=roomAudio.indexOf(10.toByte());check(boundary>=0)
                    val roomMeta=JSONObject(String(roomAudio,0,boundary,Charsets.UTF_8));check(!roomMeta.has("error")){roomMeta.toString()}
                    if(roomMeta.optBoolean("empty"))break
                    playOriginalRoomAudio(roomAudio)
                }
                if(diagnostics&&!routeReplay)check(packet.contentEquals(story(intArrayOf(),1,0))){"poll advanced story"}
                val payload=packet.copyOfRange(split+1,packet.size)
                if(metadata.optString("kind")!="system_command")runOnUiThread{timeline.post{commandPresentation.finish()}}
                if(metadata.optString("kind")=="story_return_title") {
                    val checkpoint=metadata.getJSONObject("checkpoint")
                    File(root,"story-ending.json").writeText(checkpoint.toString())
                    val slots=checkpoint.getJSONArray("slots")
                    val cleared=(0 until slots.length()).any {
                        val slot=slots.getJSONArray(it)
                        slot.getInt(0)==71 && slot.getInt(1)==904 && slot.getInt(2)==1
                    }
                    if(cleared && !replaying) {
                        check(getPreferences(MODE_PRIVATE).edit().putBoolean(clearKey,true).commit())
                    }
                    runOnUiThread{timeline.post{if(!storyClosing)returnToTitle()}}
                }else if(metadata.optString("kind")=="system_exit") {
                    check(metadata.getString("reason")=="original_quit")
                    if(traceDialogue)File(root,"system-command-events.jsonl").appendText(metadata.toString()+"\n")
                    runOnUiThread{timeline.post{if(!storyClosing)closeOriginalGame()}}
                }else if(metadata.optString("kind")=="system_command") {
                    if(metadata.optBoolean("load_catalog")){
                        val directory=saveDirectory();val quick=QuickSlots(directory)
                        val date=java.text.SimpleDateFormat("yyyy/MM/dd",java.util.Locale.ROOT)
                        val time=java.text.SimpleDateFormat("H:mm",java.util.Locale.ROOT)
                        for(id in (1..100).toList()+(101..110).toList()){
                            val slot=if(id<=100)File(directory,"slot-%03d.nksv".format(id))else quick.slot(id-100)
                            if(slot!=null && slot.isFile){
                                val stamp=slot.lastModified();val fields="$stamp\t${date.format(java.util.Date(stamp))}\t${time.format(java.util.Date(stamp))}".toByteArray(Charsets.UTF_8)
                                val result=ParcelFileDescriptor.open(slot,ParcelFileDescriptor.MODE_READ_ONLY).use{fd->JSONObject(String(story(intArrayOf(fd.fd)+fields.map{it.toInt() and 255}.toIntArray(),78,id.toLong()),Charsets.UTF_8))}
                                check(result.optBoolean("registered")){result.toString()}
                            }
                        }
                        val result=JSONObject(String(story(intArrayOf(),79,metadata.getLong("token")),Charsets.UTF_8));check(result.optBoolean("installed")){result.toString()}
                    }
                    if(metadata.optBoolean("room_replay")){
                        if(traceDialogue)File(root,"system-command-events.jsonl").appendText(metadata.toString()+"\n")
                        if(intent.getBooleanExtra("room_metadata_only",false)){
                            runOnUiThread{timeline.post{commandPresentation.finish();next.isEnabled=false}}
                        }else {
                            nestedReplayCallers.addLast(metadata.getLong("token") to replaying);replaying=true
                            show(story(intArrayOf(),82,metadata.getLong("token")))
                        }
                        return@show
                    }
                    if(metadata.optBoolean("save_config") || metadata.optBoolean("restore_screen"))applyTextConfig(story(intArrayOf(),13,0))
                    val bitmap=if(metadata.getBoolean("has_frame"))BitmapFactory.decodeByteArray(payload,0,payload.size) ?: error("invalid system command bitmap") else null
                    if(traceDialogue)File(root,"system-command-events.jsonl").appendText(metadata.toString()+"\n")
                    runOnUiThread{timeline.post(cancel={bitmap?.recycle()}){if(storyClosing){bitmap?.recycle()}else{
                        dialogue.stop();next.isEnabled=false
                        commandPresentation.accept(metadata,bitmap)
                    }}}
                }else if(metadata.has("host_effect")) {
                    val token=metadata.getLong("token");val kind=metadata.getString("host_effect")
                    if(diagnostics)File(root,"effect-$token.json").writeText(metadata.toString())
                    if(diagnostics&&kind in listOf("fade","wipe","shake","flash"))File(root,"effect-$token.png").writeBytes(payload)
                    val bitmap=if(kind in listOf("fade","wipe","shake","flash"))BitmapFactory.decodeByteArray(payload,0,payload.size) ?: error("invalid fade bitmap") else null
                    val file=if(kind in listOf("bgm","env","se","voice","staffroll","bgv1","bgv2"))File(root,"audio-$token.ogg").also{
                        it.writeBytes(payload)
                        if(diagnostics){
                        val digest=java.security.MessageDigest.getInstance("SHA-256").digest(payload).joinToString(""){b->"%02x".format(b.toInt() and 255)}
                        File(root,"audio-$token.json").writeText(JSONObject().put("kind",kind).put("name",metadata.getString("name")).put("sha256",digest).toString())
                        }
                    } else null
                    val shortClip=try {
                        if(file!=null && (kind=="voice" || (kind=="se" && !metadata.getBoolean("loop"))))ShortClip.read(file) else null
                    }catch(error:Exception){file?.delete();throw error}
                    val movieUri=if(kind=="movie")selectedFiles.single{it.first.equals(metadata.getString("name"),true)}.second else null
                    runOnUiThread{timeline.post(cancel={file?.delete();bitmap?.recycle()}){
                        if(storyClosing){file?.delete()}else {
                            if(diagnostics&&(kind=="shake" || kind=="wipe")) {
                                val old=(image.drawable as? android.graphics.drawable.BitmapDrawable)?.bitmap
                                if(old!=null)File(root,"$kind-$token-old.png").outputStream().use{check(old.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it))}
                            }
                            // Probe-only lifecycle injection: preserve the original duration, background during the real request.
                            if((kind=="shake" && intent.getBooleanExtra("close_on_shake",false)) || (kind=="flash" && intent.getBooleanExtra("close_on_flash",false)))image.postDelayed({if(!storyClosing)moveTaskToBack(true)},if(kind=="flash")5L else 35L)
                            if(kind==intent.getStringExtra("pause_on_effect")&&!intent.getBooleanExtra("effect_pause_used",false)){
                                intent.putExtra("effect_pause_used",true)
                                image.postDelayed({if(!storyClosing)moveTaskToBack(true)},80)
                            }
                            if(kind=="select"){
                                val rows = org.json.JSONArray(String(payload, Charsets.UTF_8))
                                val options = Array(rows.length()) { rows.getString(it) }
                                val choices=android.app.AlertDialog.Builder(this)
                                    .setItems(options) { _, index ->
                                        if (!storyClosing) worker.execute {
                                            try {
                                                show(story(intArrayOf(), 57, token, (index + 1).toLong()))
                                            } catch (error: Throwable) {
                                                fail(error)
                                            }
                                        }
                                    }
                                    .setCancelable(false)
                                    .show()
                                if(traceDialogue)File(root,"selection-presented.json").writeText(JSONObject(metadata.toString()).put("options_count",rows.length()).toString())
                                if(metadata.optBoolean("focus_first")&&options.isNotEmpty()){
                                    choices.listView.requestFocus()
                                    choices.listView.setSelection(0)
                                    choices.listView.setItemChecked(0,true)
                                    choices.setOnKeyListener{_,key,event->
                                        if(key==android.view.KeyEvent.KEYCODE_ENTER||key==android.view.KeyEvent.KEYCODE_DPAD_CENTER){
                                            if(event.action==android.view.KeyEvent.ACTION_UP && !event.isCanceled){
                                                val list=choices.listView
                                                val position=list.selectedItemPosition.coerceAtLeast(0)
                                                list.performItemClick(list.getChildAt(position-list.firstVisiblePosition),position,list.adapter.getItemId(position))
                                            }
                                            true
                                        }else false
                                    }
                                }
                            }else if(kind=="staffroll"){
                                val cancel=Button(this);cancel.text="返回剧情"
                                stage.addView(cancel,FrameLayout.LayoutParams(-2,-2,android.view.Gravity.TOP or android.view.Gravity.END))
                                dialogueView.visibility=android.view.View.INVISIBLE
                                cancel.setOnClickListener{storyEffects!!.cancelCredits()}
                                storyEffects!!.run(metadata,null,file){
                                    stage.removeView(cancel);dialogueView.visibility=android.view.View.VISIBLE
                                    if(!storyClosing)worker.execute{try{show(story(intArrayOf(),4,token))}catch(error:Throwable){fail(error)}}
                                }
                            }else if(kind=="movie"){
                                storyEffects!!.beginMovie(token)
                                val controlsVisibility=controls.visibility
                                val quickVisibility=quickPanel.visibility
                                controls.visibility=android.view.View.GONE
                                quickPanel.visibility=android.view.View.GONE
                                movieHost=MovieHost(this,stage,root,traceDialogue)
                                movieHost!!.start(movieUri!!,token,metadata.getBoolean("movie_left_skip"),{reason->
                                    movieHost=null
                                    controls.visibility=controlsVisibility
                                    quickPanel.visibility=quickVisibility
                                    timeline.post{if(!storyClosing)worker.execute{try{show(story(intArrayOf(),7,token,reason.toLong()))}catch(error:Throwable){fail(error)}}}
                                },{error->worker.execute{fail(error)}})
                            }else storyEffects!!.run(metadata,bitmap,file,shortClip){
                            if(!storyClosing)worker.execute{try{show(story(intArrayOf(),4,token))}catch(error:Throwable){fail(error)}}
                            }
                        }
                    }}
                }else {
                    val token=metadata.getLong("sequence")
                    val bitmap=BitmapFactory.decodeByteArray(payload,0,payload.size) ?: error("invalid story bitmap")
                    if(traceDialogue)File(root,"page-$token.json").writeText(metadata.toString())
                    loadSide()
                    if(diagnostics&&!routeReplay)File(root,"page-$token.png").writeBytes(payload)
                    runOnUiThread {timeline.post(cancel={bitmap.recycle()}){if(!isDestroyed&&!storyClosing){sequence=token;dialogue.show(token,metadata.getString("speaker"))
                        if(metadata.has("motion")){
                            val motion=metadata.getJSONObject("motion")
                            storyEffects!!.runMotion(token,motion.getLong("duration_ms"),motion.optBoolean("delayed"))
                        }else image.setImageBitmap(bitmap)
                        if(token==13L&&intent.getBooleanExtra("audio_state_probe",false)){
                            next.isEnabled=false
                            probeAudioState(this,storyEffects!!,root){error->worker.execute{fail(error)}}
                        }
                    }}}
                }
            }
        }
        fun enterStory(packet:ByteArray){
                val boundary=packet.indexOf(10.toByte());check(boundary>=0){"invalid story entry packet"}
                val entry=JSONObject(String(packet,0,boundary,Charsets.UTF_8))
                check(!entry.has("error")){entry.getString("error")}
                val capability=JSONObject(String(story(intArrayOf(),63,0),Charsets.UTF_8))
                check(!capability.has("error")){capability.getString("error")}
                classicStoryMenu=capability.getBoolean("classic_story_menu")
                loadSide()
                val savedRead=if(replaying)null else getPreferences(MODE_PRIVATE).getString(readKey,null)
                if(savedRead!=null){
                    val bytes=android.util.Base64.decode(savedRead,android.util.Base64.NO_WRAP)
                    val state=JSONObject(String(story(bytes.map{it.toInt() and 255}.toIntArray(),32,0),Charsets.UTF_8))
                    check(state.optBoolean("imported")){state.toString()}
                }
                applyAudioConfig(story(intArrayOf(),9,0))
                applyTextConfig(if(speedOverride>=0)story(intArrayOf(),69,libraryOptions.getInt("speed_$libraryEntry",speedOverride).toLong()) else story(intArrayOf(),13,0))
                val font=File("/system/fonts/NotoSansCJK-Regular.ttc")
                ParcelFileDescriptor.open(font,ParcelFileDescriptor.MODE_READ_ONLY).use{fd->
                    val result=JSONObject(String(story(intArrayOf(fd.fd),15,0),Charsets.UTF_8));check(!result.has("error")){result.optString("error")}
                    result.put("host_font_source",font.absolutePath).put("font_collection_index",0)
                    File(root,"dialogue-font.json").writeText(result.toString())
                }
                show(packet)
        }
        val configControls=LinearLayout(this);configControls.orientation=LinearLayout.VERTICAL;configControls.visibility=android.view.View.GONE
        val configNotice=TextView(this);configNotice.text="已接通：文字/自动速度、跳过未读、粗体/边框、动态示例、透明度、音量、语音保留、快捷键编辑；快捷执行支持剧情菜单、设置、快捷存读、隐藏、自动、快进、退出。";configControls.addView(configNotice)
        val configButtons=LinearLayout(this);val configBack=Button(this);configBack.text="返回标题";val configReset=Button(this);configReset.text="所有设定初始化"
        configButtons.addView(configBack,LinearLayout.LayoutParams(0,-2,1f));configButtons.addView(configReset,LinearLayout.LayoutParams(0,-2,1f));configControls.addView(configButtons);if(legacyControls)layout.addView(configControls)
        lateinit var rooms:TitleRooms
        fun playReplay() {
            replaying=true
            titleView.accepting=false
            worker.execute{try{
                val packet=story(intArrayOf(),61,0)
                runOnUiThread{menuTimeline.post{
                    stage.removeView(titleView)
                    dialogueView.visibility=android.view.View.VISIBLE
                    controls.visibility=android.view.View.VISIBLE
                }}
                enterStory(packet)
            }catch(error:Throwable){fail(error)}}
        }
        var reopenReplay=intent.getBooleanExtra("reopen_replay",false)
        lateinit var resetSettings: (Long) -> Unit
        var settingsTickPending = false
        fun showTitle(packet:ByteArray,unlock:Boolean,previewTick:Boolean=false){
            val split=packet.indexOf(10.toByte());check(split>=0){String(packet,Charsets.UTF_8)}
            val metadata=JSONObject(String(packet,0,split,Charsets.UTF_8));check(!metadata.has("error")){metadata.optString("error")}
            val isSettings=metadata.optString("kind")=="settings"
            val dragFrame=isSettings&&metadata.has("bounds")
            if(isSettings || !dragFrame || (metadata.optJSONArray("system_sounds")?.length() ?: 0)>0)playSystemSounds(metadata)
            if(metadata.optString("kind")=="story_settings_closed"){
                check(metadata.getLong("sequence")==sequence){"settings changed dialogue token"}
                check(story(intArrayOf(),1,0).contentEquals(File(root,"settings-story-checkpoint.bin").readBytes())){"settings advanced the script"}
                persistConfig();applyAudioConfig(story(intArrayOf(),9,0),menuTimeline);applyTextConfig(story(intArrayOf(),13,0),menuTimeline);loadSide()
                runOnUiThread{menuTimeline.post{if(!storyClosing){try{
                    stage.removeView(titleView);configControls.visibility=android.view.View.GONE
                    dialogueView.visibility=android.view.View.VISIBLE;controls.visibility=android.view.View.VISIBLE
                    storyConfigOpen=false;timeline.resume();storyEffects!!.resumeStory();dialogue.resume()
                    File(root,"story-settings-events.jsonl").appendText(JSONObject(metadata.toString()).put("event","closed").put("active_ms",timeline.now()).toString()+"\n")
                }catch(error:Throwable){lifecycleFailure(error)}}}}
                return
            }
            if(metadata.optString("kind")=="title")chineseScenario=metadata.optBoolean("chinese_scenario")
            if(metadata.optString("kind")!="title" && !isSettings){
                File(root,"title-started.json").writeText(metadata.toString())
                runOnUiThread{timeline.post{stage.removeView(titleView);dialogueView.visibility=android.view.View.VISIBLE;controls.visibility=android.view.View.VISIBLE;text.visibility=android.view.View.VISIBLE;text.text="正在读取剧情…"}}
                enterStory(packet);return
            }
            if(isSettings&&!previewTick&&!dragFrame)persistConfig()
            val prefix=if(isSettings)"settings" else "title"
            if(!previewTick&&!dragFrame){
                if(diagnostics)File(root,"$prefix.png").writeBytes(packet.copyOfRange(split+1,packet.size))
                File(root,"$prefix.json").writeText(metadata.toString())
            }
            val bitmap=if(dragFrame)DialogueView.decodeRgba(packet,split+1,metadata)
                else BitmapFactory.decodeByteArray(packet,split+1,packet.size-split-1) ?: error("invalid title PNG")
            runOnUiThread{menuTimeline.post(cancel={bitmap.recycle()}){
                if(!isDestroyed&&!storyClosing){
                    if(previewTick||dragFrame){
                        titleView.present(bitmap)
                        if(previewTick&&diagnostics)File(root,"settings-ticks.jsonl").appendText(
                            JSONObject().put("input_sequence",metadata.getLong("input_sequence")).toString()+"\n")
                    }else{
                        configBack.isEnabled=true;configReset.isEnabled=metadata.optInt("editing_slot",0)==0
                        configBack.text=if(metadata.optInt("editing_slot",0)>0)"取消选择" else if(storyConfigOpen)"返回剧情" else "返回标题"
                        configControls.visibility=if(isSettings)android.view.View.VISIBLE else android.view.View.GONE
                        val description=if(isSettings)"原版设置页" else "游戏标题菜单"
                        val transition=titleView.contentDescription!=description
                        if(isSettings||metadata.optString("kind")=="title")applyPresentation(if(isSettings)metadata.getJSONObject("config") else metadata)
                        if(transition)titleView.contentDescription=description
                        presentSystem(titleView,bitmap,transition);if(unlock)titleView.accepting=true
                    }
                    val tickDelay=metadata.optLong("tick_ms")
                    if((isSettings||metadata.optString("kind")=="title")&&!dragFrame&&tickDelay>0&&!settingsTickPending){
                        settingsTickPending=true
                        val scheduledAt=menuTimeline.now()
                        menuTimeline.post(tickDelay.coerceAtLeast(66)){
                            if(!storyClosing&&titleView.accepting&&!titleView.isTracking&&
                                titleView.contentDescription==(if(isSettings)"原版设置页" else "游戏标题菜单")){
                                val elapsed=menuTimeline.now()-scheduledAt
                                worker.execute{try{
                                    val update=story(intArrayOf(),70,elapsed)
                                    runOnUiThread{settingsTickPending=false}
                                    if(update.isNotEmpty())showTitle(update,false,true)
                                }catch(error:Throwable){fail(error)}}
                            }else settingsTickPending=false
                        }
                    }
                    if(previewTick&&!isSettings&&tickDelay<=0){
                        val snapshot=File(root,"title-presented.tmp");snapshot.writeText(metadata.toString())
                        check(snapshot.renameTo(File(root,"title-presented.json"))){"cannot publish completed title animation"}
                    }
                    if(previewTick || dragFrame && !diagnostics)return@post
                    val snapshot=File(root,"$prefix-presented.tmp");snapshot.writeText(metadata.toString())
                    check(snapshot.renameTo(File(root,"$prefix-presented.json"))){"cannot publish presented menu state"}
                    if(dragFrame)return@post
                    text.text="标题主页 · 点击开始游戏"
                    if(isSettings && metadata.getBoolean("unsupported"))android.widget.Toast.makeText(this,"此选项尚未接入",android.widget.Toast.LENGTH_SHORT).show()
                    val action=metadata.optInt("action",0)
                    if(!isSettings&&action==0&&resumeSlot==-1){
                        resumeSlot=0
                        openSaveSlots(false,true)
                    }
                    else if(!isSettings&&action==0&&resumeSlot in 1..110){
                        val slot=resumeSlot;resumeSlot=0
                        openSaveSlots(false,true,resume=slot)
                    }
                    else if(action!=0 && action==metadata.optInt("load_action",0))openSaveSlots(false,true)
                    else if(action!=0 && action==metadata.optInt("quit_action",4))close.performClick()
                    else if(metadata.optBoolean("room_action"))rooms.open(action)
                    else if(!isSettings && reopenReplay){
                        reopenReplay=false
                        rooms.open(metadata.getInt("replay_action"))
                    }
                    else if(action!=0)android.widget.Toast.makeText(this,"此入口尚未接入；当前支持开始游戏",android.widget.Toast.LENGTH_SHORT).show()
                }else bitmap.recycle()
            }}
        }
        resetSettings={ command ->
            configBack.isEnabled=false;configReset.isEnabled=false;titleView.cancel();titleView.accepting=false
            worker.execute{try{showTitle(story(intArrayOf(),22,command),true)}catch(error:Throwable){fail(error)}}
        }
        returnTitleFromSave={packet->showTitle(packet,true)}
        enterLoadedStory={packet->
            File(root,"title-loaded.json").writeText("{\"loaded\":true}")
            runOnUiThread{menuTimeline.post{stage.removeView(titleView);dialogueView.visibility=android.view.View.VISIBLE;controls.visibility=android.view.View.VISIBLE}}
            enterStory(packet)
        }
        configBack.setOnClickListener{configBack.isEnabled=false;configReset.isEnabled=false;titleView.cancel();titleView.accepting=false;worker.execute{try{showTitle(story(intArrayOf(),22,0),true)}catch(error:Throwable){fail(error)}}}
        configReset.setOnClickListener{resetSettings(1)}
        class TitleMove(var input:IntArray)
        val titleMoveLock=Any()
        var pendingTitleMove:TitleMove?=null
        fun dispatchTitlePointer(input:IntArray){
            try{if(!storyClosing)showTitle(story(input,18,0),input[0]==2)}catch(error:Throwable){fail(error)}
        }
        titleView=TitleView(this){kind,x,y->
            if(!storyClosing){
                val input=intArrayOf(kind,x,y)
                if(kind==1){
                    val batch=synchronized(titleMoveLock){
                        val queued=pendingTitleMove
                        if(queued!=null){queued.input=input;null}
                        else TitleMove(input).also{pendingTitleMove=it}
                    }
                    if(batch!=null)worker.execute{
                        val move=synchronized(titleMoveLock){
                            if(pendingTitleMove===batch)pendingTitleMove=null
                            batch.input
                        }
                        dispatchTitlePointer(move)
                    }
                }else {
                    synchronized(titleMoveLock){pendingTitleMove=null}
                    worker.execute{dispatchTitlePointer(input)}
                }
            }
        }
        rooms=TitleRooms(this,stage,titleView,worker,
            {operation,indexes,id->story(indexes,operation,id)},
            {metadata->
                repeat(metadata.optInt("room_music_count")){playRoomMusic(story(intArrayOf(),74,0))}
                playSystemSounds(metadata)
            },
            {metadata->
                val snapshot=File(root,"room-presented.tmp")
                snapshot.writeText(metadata.toString())
                check(snapshot.renameTo(File(root,"room-presented.json"))){"cannot publish room state"}
            }, ::fail, ::playReplay)
        roomBack=rooms::back
        originalSettings.setOnClickListener {
            storyConfigOpen=true;dialogue.suspend();timeline.pause();storyEffects!!.pauseStory()
            controls.visibility=android.view.View.GONE;dialogueView.visibility=android.view.View.GONE
            titleView.accepting=false;stage.addView(titleView,FrameLayout.LayoutParams(-1,-1))
            val token=sequence
            File(root,"story-settings-events.jsonl").appendText(JSONObject().put("event","opened").put("sequence",token).put("active_ms",timeline.now()).toString()+"\n")
            worker.execute{try{File(root,"settings-story-checkpoint.bin").writeBytes(story(intArrayOf(),1,0));showTitle(story(intArrayOf(),23,token),true)}catch(error:Throwable){fail(error)}}
        }
        settingsBack={
            if(storyConfigOpen || configControls.visibility==android.view.View.VISIBLE){
                // Consume Back while a settings request is in flight, too.
                if(configBack.isEnabled)configBack.performClick()
                true
            }else false
        }
        if(titleFirst){dialogueView.visibility=android.view.View.GONE;controls.visibility=android.view.View.GONE;text.visibility=android.view.View.GONE;stage.addView(titleView,FrameLayout.LayoutParams(-1,-1))}
        worker.execute {
            try {
                System.loadLibrary("nekopack_host");storyLoaded=true
                val top=children(tree,DocumentsContract.getTreeDocumentId(tree))
                val selected=if(libraryDirectory != null) {
                    children(tree, libraryDirectory)
                } else if(top.any{it.first.endsWith(".dat",true)}) top else {
                    val child=top.single{it.first==game.toString()}
                    children(tree,DocumentsContract.getDocumentId(child.second))
                }
                selectedFiles=selected
                val opened=mutableListOf<ParcelFileDescriptor>()
                val progressFile=if(titleFirst)savedProgress?.let{encoded->
                    File.createTempFile("unlocks-",".bin",root).apply{
                        writeBytes(android.util.Base64.decode(encoded,android.util.Base64.NO_WRAP))
                    }
                }else null
                var packet=try {
                    if(progressFile!=null)opened.add(ParcelFileDescriptor.open(progressFile,ParcelFileDescriptor.MODE_READ_ONLY))
                    val archives=selected.filter{(it.first.endsWith(".dat",true)||it.first.endsWith(".pk2",true))&&!it.first.equals("unins000.dat",true)} + selected.filter{it.first.endsWith(".exe",true)}
                    if(traceDialogue)File(root,"mount-inputs.json").writeText(org.json.JSONArray(archives.map{it.first}).toString())
                    require(archives.isNotEmpty()){"游戏目录中没有可读取的 DAT，请检查目录位置和授权"}
                    for((_,uri) in archives)opened.add(contentResolver.openFileDescriptor(uri,"r") ?: error("missing file descriptor"))
                    val cleared=titleFirst && getPreferences(MODE_PRIVATE).getBoolean(clearKey,false)
                    story(opened.map{it.fd}.toIntArray(),if(titleFirst)17 else 0,if(cleared)1L else 0L,(if(progressFile!=null)1L else 0L) or (if(originalVersion)2L else 0L),archives.joinToString("\u0000"){it.first}.toByteArray(Charsets.UTF_8))
                }finally{
                    opened.forEach{it.close()}
                    progressFile?.delete()
                }
                if(titleFirst){
                    val end=packet.indexOf(10.toByte()).let{if(it<0)packet.size else it}
                    val initial=JSONObject(String(packet,0,end,Charsets.UTF_8))
                    check(!initial.has("error")){initial.getString("error")}
                    ParcelFileDescriptor.open(File("/system/fonts/NotoSansCJK-Regular.ttc"),ParcelFileDescriptor.MODE_READ_ONLY).use{fd->
                        val installed=JSONObject(String(story(intArrayOf(fd.fd),24,0),Charsets.UTF_8))
                        check(installed.optBoolean("installed")){installed.toString()}
                    }
                    val saved=getPreferences(MODE_PRIVATE).getString(preferenceKey,null)
                    if(saved!=null){val bytes=android.util.Base64.decode(saved,android.util.Base64.NO_WRAP);packet=story(bytes.map{it.toInt() and 255}.toIntArray(),19,0)}
                    showTitle(packet,true)
                }else enterStory(packet)
            }catch(error:Throwable){fail(error)}
        }
        val stageBounds=android.graphics.Rect()
        val floatingBounds=android.graphics.Rect()
        val decorPosition=IntArray(2)
        systemCommandTouch={event->
            if(!commandPresentation.active||storyInputArea?.invoke(event)!=true)false
            else {
                val location=IntArray(2);image.getLocationOnScreen(location)
                val point=floatArrayOf(event.rawX-location[0]-image.paddingLeft,event.rawY-location[1]-image.paddingTop)
                val inverse=android.graphics.Matrix()
                check(image.imageMatrix.invert(inverse)){"system command image matrix is singular"}
                inverse.mapPoints(point)
                val action=when(event.actionMasked){
                    android.view.MotionEvent.ACTION_DOWN->1
                    android.view.MotionEvent.ACTION_MOVE->3
                    else->0
                }
                commandPresentation.pointer(action,point[0].toInt(),point[1].toInt())
            }
        }
        storyInputArea={event->
            window.decorView.getLocationOnScreen(decorPosition)
            val x=(event.x+decorPosition[0]).toInt()
            val y=(event.y+decorPosition[1]).toInt()
            !storyClosing&&!storyConfigOpen&&!storySaveOpen&&!storyLogOpen&&!menuOpen&&
                !dialogueHidden&&!hideBusy&&!timeline.paused&&movieHost?.isActive!=true&&(sequence>0||commandPresentation.active)&&
                stage.getGlobalVisibleRect(stageBounds)&&stageBounds.contains(x,y)&&
                !(quickPanel.visibility==android.view.View.VISIBLE&&quickPanel.getGlobalVisibleRect(floatingBounds)&&floatingBounds.contains(x,y))
        }
        storyInputAction=action@{ action ->
            if(commandPresentation.active){
                when(action){
                    StoryAction.NEXT->commandPresentation.input(1)
                    StoryAction.MENU,StoryAction.HIDE->commandPresentation.input(2)
                    else->Unit
                }
                return@action
            }
            val button=when(action){
                StoryAction.NEXT->next
                StoryAction.MENU->menu
                StoryAction.HIDE->hide
                StoryAction.AUTO->auto
                StoryAction.SKIP->skip
                StoryAction.CONFIG->originalSettings
                StoryAction.LOG->{openLog();null}
                StoryAction.NONE->null
            }
            if(button?.isEnabled==true)button.performClick()
        }
        storySpace={activate->
            if(storyClosing||sequence==0L||storyConfigOpen||storySaveOpen||storyLogOpen||menuOpen||movieHost?.isActive==true||timeline.paused)false
            else {
                if(activate&&hiddenBack?.invoke()!=true){
                    val action=when(textConfig?.getInt("space_key")){1->StoryAction.HIDE;2->StoryAction.SKIP;else->StoryAction.NEXT}
                    storyInputAction?.invoke(action)
                }
                true
            }
        }
        next.setOnClickListener {dialogue.click()}
        image.setOnClickListener {
            if(commandPresentation.input(1))Unit
            else if(next.isEnabled&&!storyClosing&&!storyConfigOpen&&!storySaveOpen&&!storyLogOpen&&!menuOpen&&!dialogueHidden&&!hideBusy&&!timeline.paused){
                next.performClick()
            }
        }
        fun textPanel(dark:Boolean):LinearLayout {
            val config=textConfig!!;val panel=LinearLayout(this);panel.orientation=LinearLayout.VERTICAL
            val label=TextView(this);if(dark)label.setTextColor(android.graphics.Color.WHITE);label.text="文字速度 ${config.getInt("speed")}%（下一页生效）";panel.addView(label)
            val speed=android.widget.SeekBar(this);speed.max=100;speed.progress=config.getInt("speed");speed.contentDescription="text_speed";panel.addView(speed)
            fun update(field:Long,value:Long){worker.execute{try{if(!storyClosing){
                if(field==0L&&speedOverride>=0){
                    libraryOptions.edit().putInt("speed_$libraryEntry",value.toInt()).apply()
                    applyTextConfig(story(intArrayOf(),69,value))
                }else applyTextConfig(story(intArrayOf(),14,field,value))
            }}catch(error:Throwable){fail(error)}}}
            speed.setOnSeekBarChangeListener(object:android.widget.SeekBar.OnSeekBarChangeListener{
                override fun onStartTrackingTouch(bar:android.widget.SeekBar){}
                override fun onProgressChanged(bar:android.widget.SeekBar,value:Int,fromUser:Boolean){label.text="文字速度 $value%（下一页生效）"}
                override fun onStopTrackingTouch(bar:android.widget.SeekBar){update(0,bar.progress.toLong())}
            })
            val keep=android.widget.Switch(this);if(dark)keep.setTextColor(android.graphics.Color.WHITE);keep.text="换页保留语音";keep.contentDescription="keep_voice";keep.isChecked=config.getInt("keep_voice")==1;panel.addView(keep)
            keep.setOnCheckedChangeListener{_,checked->update(1,if(checked)1 else 0)}
            return panel
        }
        textSettings.setOnClickListener {
            android.app.AlertDialog.Builder(this).setTitle("文字（当前会话）").setView(textPanel(false)).setPositiveButton("返回剧情",null).show()
        }
        fun audioPanel(dark:Boolean):LinearLayout {
            val config=audioConfig!!
            val panel=LinearLayout(this);panel.orientation=LinearLayout.VERTICAL
            val names=listOf("BGM","语音","音效","环境音","系统音")
            for((index,kind) in listOf("bgm","voice","se","env","system").withIndex()){
                val label=TextView(this);if(dark)label.setTextColor(android.graphics.Color.WHITE);label.text="${names[index]} ${config.getInt(kind)}%";panel.addView(label)
                val slider=android.widget.SeekBar(this);slider.max=100;slider.progress=config.getInt(kind);slider.contentDescription="volume_$kind";panel.addView(slider)
                slider.setOnSeekBarChangeListener(object:android.widget.SeekBar.OnSeekBarChangeListener {
                    override fun onStartTrackingTouch(bar:android.widget.SeekBar){}
                    override fun onProgressChanged(bar:android.widget.SeekBar,value:Int,fromUser:Boolean){label.text="${names[index]} $value%"}
                    override fun onStopTrackingTouch(bar:android.widget.SeekBar){
                        val value=bar.progress
                        worker.execute{try{if(!storyClosing)applyAudioConfig(story(intArrayOf(),10,index.toLong(),value.toLong()))}catch(error:Throwable){fail(error)}}
                    }
                })
            }
            return panel
        }
        settings.setOnClickListener {
            android.app.AlertDialog.Builder(this).setTitle(if(titleFirst)"音量（自动保存）" else "音量（当前会话）").setView(audioPanel(false)).setPositiveButton("返回剧情",null).show()
        }
        quickAction("快捷存档"){if(next.isEnabled&&!timeline.paused&&!storyClosing)openSaveSlots(true,quick=true)}
        quickAction("快捷读档"){if(next.isEnabled&&!timeline.paused&&!storyClosing)openSaveSlots(false,quick=true)}
        quickAction("自动阅读"){if(auto.isEnabled&&!timeline.paused&&!storyClosing)auto.performClick()}
        quickAction("快进"){if(skip.isEnabled&&!timeline.paused&&!storyClosing)skip.performClick()}
        quickAction("隐藏浮窗"){quickPanel.visibility=android.view.View.GONE}
        if(!legacyControls)stage.addView(quickPanel,FrameLayout.LayoutParams(-2,-2,android.view.Gravity.END or android.view.Gravity.CENTER_VERTICAL).apply {
            marginEnd=dp(8)
        })
        var exitToLibrary=false
        fun openDrawer(section:String){
            if(!storyClosing && emulatorMenu==null){
                val inStory=sequence>0 && controls.visibility==android.view.View.VISIBLE && movieHost?.isActive!=true
                timeline.pause()
                storyEffects?.pause()
                movieHost?.pause()
                val drawer=EmulatorMenu(this){
                    emulatorMenu=null
                    if(foreground&&!storyClosing)resumeStoryRuntime()
                }
                emulatorMenu=drawer
                if(section.isNotEmpty()){
                    drawer.section(section)
                    when(section){
                        "显示" -> if(textSettings.isEnabled)drawer.panel(textPanel(true))
                        "音频" -> if(settings.isEnabled)drawer.panel(audioPanel(true))
                        "输入" -> drawer.item("鼠标与手势",keepOpen=true){inputBindings.show(this)}
                    }
                    drawer.item("返回模拟器菜单"){openDrawer("")}
                }else{
                    drawer.item(if(inStory)"继续游戏" else "继续",primary=true,action={})
                    if(inStory){
                        drawer.section("快捷操作")
                        drawer.pair(
                            EmulatorMenu.Action("存档",next.isEnabled&&!replaying){openSaveSlots(true)},
                            EmulatorMenu.Action("读档",next.isEnabled&&!replaying){openSaveSlots(false)}
                        )
                        drawer.pair(
                            EmulatorMenu.Action(auto.text.toString(),auto.isEnabled){auto.performClick()},
                            EmulatorMenu.Action(skip.text.toString(),skip.isEnabled){skip.performClick()}
                        )
                        if(!legacyControls){
                            val visible=quickPanel.visibility==android.view.View.VISIBLE
                            drawer.item(if(visible)"关闭快捷浮窗" else "开启快捷浮窗"){
                                quickPanel.visibility=if(visible)android.view.View.GONE else android.view.View.VISIBLE
                            }
                        }
                    }
                    drawer.section("设置")
                    if(inStory){
                        drawer.pair(
                            EmulatorMenu.Action("显示",textSettings.isEnabled){openDrawer("显示")},
                            EmulatorMenu.Action("音频",settings.isEnabled){openDrawer("音频")}
                        )
                        drawer.pair(
                            EmulatorMenu.Action("输入",true){openDrawer("输入")},
                            EmulatorMenu.Action("原版设置",originalSettings.isEnabled){originalSettings.performClick()}
                        )
                        drawer.section("剧情")
                        drawer.pair(
                            EmulatorMenu.Action("隐藏对白",hide.isEnabled){hide.performClick()},
                            EmulatorMenu.Action("消息记录",next.isEnabled){openLog()}
                        )
                        drawer.item("剧情菜单",menu.isEnabled){menu.performClick()}
                    }else drawer.item("输入"){openDrawer("输入")}
                    if(configControls.visibility==android.view.View.VISIBLE){
                        drawer.item(configBack.text.toString(),configBack.isEnabled){configBack.performClick()}
                        drawer.item(configReset.text.toString(),configReset.isEnabled){configReset.performClick()}
                    }
                    if(storyLogOpen)drawer.item("返回剧情"){storyLogBack?.invoke()}
                    if(storySaveOpen)drawer.item("关闭存读档"){storySaveBack?.invoke()}
                    if(replaying)drawer.item("退出回想"){if(!storyClosing)returnToTitle()}
                    drawer.item("返回游戏库",close.isEnabled){
                        if(!storyClosing){exitToLibrary=true;close.performClick()}
                    }
                }
                drawer.show()
            }
        }
        openEmulatorMenu={openDrawer("")}
        fun closeGame(){
            next.isEnabled=false;close.isEnabled=false;storyClosing=true;systemCommandPresentation?.finish();dialogue.stop();movieHost?.close();storyEffects?.close()
            worker.execute{try{
                if(storyLoaded && JSONObject(String(story(intArrayOf(),66,0),Charsets.UTF_8)).optBoolean("ready")){
                    QuickSlots(saveDirectory()).write{file->
                        ParcelFileDescriptor.open(file,ParcelFileDescriptor.MODE_CREATE or ParcelFileDescriptor.MODE_READ_WRITE).use{fd->
                            val saved=JSONObject(String(story(intArrayOf(fd.fd),66,0),Charsets.UTF_8))
                            check(saved.optBoolean("saved")){saved.toString()}
                            File(root,"exit-autosave.json").writeText(saved.toString())
                        }
                    }
                }
                val result=story(intArrayOf(),3,0);check(String(result,Charsets.UTF_8).contains("\"closed\":true"));File(root,"closed.txt").writeText("closed; VM live bytes=0");runOnUiThread{
                if(exitToLibrary)startActivity(Intent(this,LibraryActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP))
                finish()
            }}catch(error:Throwable){fail(error)}}
        }
        closeOriginalGame=::closeGame
        close.setOnClickListener{confirmOriginal("退出游戏？",confirmAction("QUIT")&&!exitToLibrary,action=::closeGame)}
    }
    override fun dispatchTouchEvent(event:android.view.MotionEvent):Boolean {
        if(systemCommandTouch?.invoke(event)==true)return true
        val deliver=storyInputAction
        if(deliver!=null && storyPointer.touch(event,storyInputArea?.invoke(event)==true,inputBindings,deliver)){
            if(event.actionMasked==android.view.MotionEvent.ACTION_UP){
                val cancel=android.view.MotionEvent.obtain(event)
                cancel.action=android.view.MotionEvent.ACTION_CANCEL
                super.dispatchTouchEvent(cancel)
                cancel.recycle()
            }
            return true
        }
        return super.dispatchTouchEvent(event)
    }
    override fun dispatchGenericMotionEvent(event:android.view.MotionEvent):Boolean {
        val deliver=storyInputAction
        if(event.isFromSource(android.view.InputDevice.SOURCE_MOUSE)&&event.actionButton==android.view.MotionEvent.BUTTON_SECONDARY&&
            event.actionMasked==android.view.MotionEvent.ACTION_BUTTON_RELEASE&&hiddenBack?.invoke()==true)return true
        if(deliver!=null && storyPointer.mouse(event,storyInputArea?.invoke(event)==true,inputBindings,rightMouseAction,deliver))return true
        return super.dispatchGenericMotionEvent(event)
    }
    override fun onKeyDown(keyCode:Int,event:android.view.KeyEvent):Boolean {
        if(movieHost?.isActive==true && keyCode in android.view.KeyEvent.KEYCODE_F1..android.view.KeyEvent.KEYCODE_F12)return true
        if(keyCode in android.view.KeyEvent.KEYCODE_F1..android.view.KeyEvent.KEYCODE_F12 && shortcutKey!=null)return true
        return super.onKeyDown(keyCode,event)
    }
    override fun dispatchKeyEvent(event:android.view.KeyEvent):Boolean {
        if(systemCommandPresentation?.active==true && event.keyCode in listOf(android.view.KeyEvent.KEYCODE_SPACE,android.view.KeyEvent.KEYCODE_ENTER,android.view.KeyEvent.KEYCODE_DPAD_CENTER)){
            if(event.action==android.view.KeyEvent.ACTION_UP&&!event.isCanceled)systemCommandPresentation!!.input(1)
            return true
        }
        if(event.keyCode==android.view.KeyEvent.KEYCODE_SPACE){
            if(movieHost?.isActive==true)return true
            if(storySpace?.invoke(event.action==android.view.KeyEvent.ACTION_UP&&!event.isCanceled)==true)return true
        }
        return super.dispatchKeyEvent(event)
    }
    override fun onKeyUp(keyCode:Int,event:android.view.KeyEvent):Boolean {
        if(movieHost?.isActive==true && keyCode in android.view.KeyEvent.KEYCODE_F1..android.view.KeyEvent.KEYCODE_F12)return true
        if(!event.isCanceled && keyCode==android.view.KeyEvent.KEYCODE_ENTER && hiddenBack?.invoke()==true)return true
        if(keyCode in android.view.KeyEvent.KEYCODE_F1..android.view.KeyEvent.KEYCODE_F12 && shortcutKey!=null){
            if(!event.isCanceled)shortcutKey!!.invoke(keyCode-android.view.KeyEvent.KEYCODE_F1+1)
            return true
        }
        return super.onKeyUp(keyCode,event)
    }
    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        if(systemCommandPresentation?.input(2)==true)return
        if(roomBack?.invoke()==true || storyLogBack?.invoke()==true || storySaveBack?.invoke()==true || storyMenuBack?.invoke()==true ||
            hiddenBack?.invoke()==true || settingsBack?.invoke()==true)return
        val open=openEmulatorMenu
        if(open!=null && !storyClosing)open()
        else super.onBackPressed()
    }
    override fun onPause() {
        foreground=false
        lifecycleGeneration++
        menuTimeline.pause();timeline.pause();storyEffects?.pause();movieHost?.pause()
        if(intent.getBooleanExtra("lifecycle_probe",false)){
            val generation=lifecycleGeneration
            for((delay,label) in listOf(500L to "paused_stable_1",1500L to "paused_stable_2")){
                window.decorView.postDelayed({if(!storyClosing&&timeline.paused&&lifecycleGeneration==generation)storyEffects?.lifecycleAudio(label)},delay)
            }
        }
        storyRunRoot?.let{File(it,"lifecycle-events.jsonl").appendText(JSONObject().put("event","paused").put("time_ms",android.os.SystemClock.elapsedRealtime()).put("active_ms",timeline.now()).put("instance",instanceId).toString()+"\n")}
        if(intent.getBooleanExtra("finish_while_paused",false)&&!isFinishing)window.decorView.postDelayed({finish()},300)
        super.onPause()
    }
    override fun onResume() {
        super.onResume()
        hidePlayerStatusBar()
        lifecycleGeneration++
        foreground=true
        menuTimeline.resume()
        if(emulatorMenu==null)resumeStoryRuntime()
        storyRunRoot?.let{File(it,"lifecycle-events.jsonl").appendText(JSONObject().put("event","resumed").put("time_ms",android.os.SystemClock.elapsedRealtime()).put("active_ms",timeline.now()).put("instance",instanceId).toString()+"\n")}
    }
    override fun onWindowFocusChanged(hasFocus:Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if(hasFocus)hidePlayerStatusBar()
    }

    @Suppress("DEPRECATION")
    private fun hidePlayerStatusBar() {
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_FULLSCREEN)
        if(android.os.Build.VERSION.SDK_INT>=30) {
            window.decorView.windowInsetsController?.let {
                it.systemBarsBehavior=android.view.WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                it.hide(android.view.WindowInsets.Type.statusBars())
            }
        } else {
            window.decorView.systemUiVisibility=window.decorView.systemUiVisibility or
                android.view.View.SYSTEM_UI_FLAG_FULLSCREEN or android.view.View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
        }
    }
    private fun resumeStoryRuntime() {
        val modal=storyConfigOpen||storySaveOpen||storyLogOpen||originalConfirmOpen
        if(!modal)timeline.resume()
        try{
            storyEffects?.resume()
            if(!modal)movieHost?.resume()
        }catch(error:Throwable){lifecycleFailure(error)}
    }
    override fun onDestroy() {
        foreground=false
        emulatorMenu?.dismiss()
        openEmulatorMenu=null
        roomBack=null
        settingsBack=null;shortcutKey=null;hiddenBack=null;storyMenuBack=null;storySaveBack=null;storyLogBack=null
        storyClosing=true;systemCommandPresentation?.finish();menuTimeline.close();timeline.close();movieHost?.close();storyEffects?.close()
        storyWorker?.let {worker->worker.execute{if(storyLoaded){val result=story(intArrayOf(),3,0);File(storyRunRoot!!,"lifecycle-closed.json").writeBytes(result)}};worker.shutdown()}
        super.onDestroy()
    }

}
