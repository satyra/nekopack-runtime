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
    private external fun story(descriptors:IntArray,operation:Int,sequence:Long,elapsedMillis:Long=0):ByteArray
    private val timeline=SessionTimeline()
    private val menuTimeline=SessionTimeline()
    @Volatile private var storyConfigOpen=false
    private var shortcutKey:((Int)->Unit)?=null
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
        if(intent.hasExtra("movie_probe_game")){
            val saved=getPreferences(MODE_PRIVATE).getString("dat_tree",null) ?: error("select DAT tree through SAF first")
            val game=intent.getIntExtra("movie_probe_game",0);require(game in 0..3)
            val runId=intent.getStringExtra("run_id") ?: "movie-manual";require(Regex("[a-zA-Z0-9_-]{1,64}").matches(runId))
            val root=File(filesDir,runId);root.mkdirs();storyRunRoot=root
            val stage=FrameLayout(this);setContentView(stage)
            val tree=Uri.parse(saved);val top=children(tree,DocumentsContract.getTreeDocumentId(tree))
            val items=if(top.any{it.first.endsWith(".dat",true)})top else children(tree,DocumentsContract.getDocumentId(top.single{it.first==game.toString()}.second))
            val uri=items.single{it.first.equals("op.mpg",true)}.second
            System.loadLibrary("nekopack_host")
            lifecycleFailure={error->File(root,"error.txt").writeText(error.toString());movieHost?.close()}
            movieHost=MovieHost(this,stage,root)
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
        val cursor=contentResolver.query(uri,projection,null,null,null) ?: error("provider returned no cursor")
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
        val runId=intent.getStringExtra("run_id") ?: "story-manual"
        require(Regex("[a-zA-Z0-9_-]{1,64}").matches(runId))
        val root=File(filesDir,runId);root.mkdirs();storyRunRoot=root
        val layout=LinearLayout(this);layout.orientation=LinearLayout.VERTICAL
        val image=ImageView(this);image.scaleType=ImageView.ScaleType.FIT_CENTER
        val stage=FrameLayout(this);stage.setBackgroundColor(android.graphics.Color.BLACK)
        val overlay=ImageView(this);overlay.scaleType=ImageView.ScaleType.FIT_CENTER;overlay.alpha=0f
        stage.addView(image,FrameLayout.LayoutParams(-1,-1));stage.addView(overlay,FrameLayout.LayoutParams(-1,-1))
        val dialogueView=DialogueView(this);stage.addView(dialogueView,FrameLayout.LayoutParams(-1,-1))
        layout.addView(stage,LinearLayout.LayoutParams(-1,0,1f))
        val text=TextView(this);text.text="正在读取剧情…";text.textSize=16f;text.maxLines=3;layout.addView(text)
        val next=Button(this);next.text="继续";next.isEnabled=false;val controls=LinearLayout(this);controls.addView(next,LinearLayout.LayoutParams(0,-2,1f))
        val close=Button(this);close.text="结束运行";controls.addView(close,LinearLayout.LayoutParams(0,-2,1f));layout.addView(controls)
        val settings=Button(this);settings.text="音量";settings.isEnabled=false;controls.addView(settings,LinearLayout.LayoutParams(0,-2,1f))
        val textSettings=Button(this);textSettings.text="文字";textSettings.isEnabled=false;controls.addView(textSettings,LinearLayout.LayoutParams(0,-2,1f))
        val originalSettings=Button(this);originalSettings.text="原版设置";originalSettings.isEnabled=false;controls.addView(originalSettings,LinearLayout.LayoutParams(0,-2,1f))
        val auto=Button(this);auto.text="自动：关";auto.isEnabled=false;controls.addView(auto,LinearLayout.LayoutParams(0,-2,1f))
        val skip=Button(this);skip.text="快进：关";skip.isEnabled=false;controls.addView(skip,LinearLayout.LayoutParams(0,-2,1f))
        setContentView(layout)
        layout.post{File(root,"layout.json").writeText(JSONObject().put("width",layout.width).put("height",layout.height).put("stage_width",stage.width).put("stage_height",stage.height).put("orientation",resources.configuration.orientation).toString())}
        val worker=Executors.newSingleThreadExecutor();storyWorker=worker
        val preferenceKey="runtime_config_v1_${tree}_$game"
        fun persistConfig(){
            if(!titleFirst)return
            val bytes=story(intArrayOf(),20,0)
            check(bytes.size==27 && bytes.copyOfRange(0,5).contentEquals(byteArrayOf(78,75,67,70,6))){String(bytes,Charsets.UTF_8)}
            check(getPreferences(MODE_PRIVATE).edit().putString(preferenceKey,android.util.Base64.encodeToString(bytes,android.util.Base64.NO_WRAP)).commit()){"configuration save failed"}
        }
        val readKey="runtime_read_v1_${tree}_${game}"+intent.getStringExtra("read_history_probe").let{if(it==null)"" else "_probe_$it"}
        var skipEnabled=false
        var autoEnabled=false
        var sequence=0L
        var selectedFiles:List<Pair<String,Uri>> = emptyList()

        lateinit var show:(ByteArray)->Unit
        lateinit var dialogue:DialoguePresentation
        var textConfig:JSONObject?=null
        fun applyTextConfig(bytes:ByteArray,scheduler:SessionTimeline=timeline){
            persistConfig()
            val config=JSONObject(String(bytes,Charsets.UTF_8));check(!config.has("error")){config.optString("error")}
            runOnUiThread{scheduler.post{if(!storyClosing){textConfig=config;textSettings.isEnabled=true;File(root,"text-config-events.jsonl").appendText(config.toString()+"\n")}}}
        }
        var audioConfig:JSONObject?=null
        fun applyAudioConfig(bytes:ByteArray,scheduler:SessionTimeline=timeline) {
            persistConfig()
            val config=JSONObject(String(bytes,Charsets.UTF_8));check(!config.has("error")){config.optString("error")}
            runOnUiThread{scheduler.post{if(!storyClosing){
                storyEffects!!.configureAudio(config);audioConfig=config;settings.isEnabled=true
                File(root,"audio-config-events.jsonl").appendText(config.toString()+"\n")
            }}}
        }
        fun fail(error:Throwable) {
            storyClosing=true
            File(root,"error.txt").writeText("${error.javaClass.name}: ${error.message}")
            if(storyLoaded)story(intArrayOf(),3,0)
            runOnUiThread{movieHost?.close();storyEffects?.close();if(!isDestroyed){text.visibility=android.view.View.VISIBLE;text.text="无法继续：${error.message}";next.isEnabled=false}}
        }
        lifecycleFailure={error->worker.execute{fail(error)}}
        dialogue=DialoguePresentation(dialogueView,timeline,root,{op,token,elapsed,done->
            val frameOp=if(storyEffects!!.voiceFinished())op+16 else op
            worker.execute{try{if(!storyClosing){
                val state=JSONObject(String(story(intArrayOf(),frameOp,token,elapsed),Charsets.UTF_8));check(!state.has("error")){state.optString("error")}
                if(state.getBoolean("advance")){
                    val history=story(intArrayOf(),31,0)
                    check(history.size>=5 && history.copyOfRange(0,5).contentEquals(byteArrayOf(78,75,82,68,1))){String(history,Charsets.UTF_8)}
                    check(getPreferences(MODE_PRIVATE).edit().putString(readKey,android.util.Base64.encodeToString(history,android.util.Base64.NO_WRAP)).commit()){"read history save failed"}
                }
                val frame=story(intArrayOf(),16,token);val split=frame.indexOf(10.toByte());check(split>=0){String(frame,Charsets.UTF_8)}
                val geometry=JSONObject(String(frame,0,split,Charsets.UTF_8));val png=frame.copyOfRange(split+1,frame.size)
                val bitmap=BitmapFactory.decodeByteArray(png,0,png.size) ?: error("invalid dialogue overlay")
                File(root,"dialogue-$token.json").writeText(geometry.toString())
                if(state.getLong("visible")==0L)File(root,"dialogue-$token-empty.png").writeBytes(png)
                if(state.getBoolean("complete"))File(root,"dialogue-$token.png").writeBytes(png)
                runOnUiThread{timeline.post(cancel={bitmap.recycle()}){if(!storyClosing){text.visibility=android.view.View.GONE;skipEnabled=state.getBoolean("skip");skip.text=if(skipEnabled)"快进：开" else "快进：关";autoEnabled=state.getBoolean("auto");auto.text=if(autoEnabled)"自动：开" else "自动：关";done(state,bitmap,geometry)}else bitmap.recycle()}}
            }}catch(error:Throwable){fail(error)}}
        },{enabled->skip.isEnabled=(enabled||skipEnabled)&&!storyConfigOpen;auto.isEnabled=(enabled||autoEnabled)&&!storyConfigOpen;next.isEnabled=enabled&&!(sequence==13L&&intent.getBooleanExtra("audio_state_probe",false));originalSettings.isEnabled=enabled&&!storyConfigOpen
            if(originalSettings.isEnabled&&dialogue.visibleCount>0&&sequence==intent.getLongExtra("settings_probe_sequence",-1)&&!intent.getBooleanExtra("settings_probe_used",false)){
                intent.putExtra("settings_probe_used",true);originalSettings.performClick()
            }
        },{stopVoice->
            storyEffects?.stopMotion();storyEffects?.finishDialogue(sequence,stopVoice);val token=sequence
            worker.execute{try{show(story(intArrayOf(),2,token))}catch(error:Throwable){fail(error)}}
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
        skip.setOnClickListener {
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
        var shortcutBusy=false
        shortcutKey={slot->
            if(!shortcutBusy&&!storyClosing&&!storyConfigOpen&&!timeline.paused&&sequence>0){
                shortcutBusy=true;val token=sequence
                worker.execute{try{
                    val result=JSONObject(String(story(intArrayOf(),33,token,slot.toLong()),Charsets.UTF_8))
                    check(!result.has("error")){result.optString("error")}
                    runOnUiThread{timeline.post{shortcutBusy=false;if(!storyClosing){
                        val status=result.getString("status");val action=result.getString("action")
                        if(storyConfigOpen)result.put("delivery","modal")
                        else if(status=="applied"){
                            autoEnabled=result.getBoolean("auto");skipEnabled=result.getBoolean("skip")
                            auto.text=if(autoEnabled)"自动：开" else "自动：关";skip.text=if(skipEnabled)"快进：开" else "快进：关"
                            dialogue.refresh();result.put("delivery","applied")
                        }else if(status=="request"){
                            if(action=="CONFIG"&&sequence==token&&originalSettings.isEnabled){originalSettings.performClick();result.put("delivery","settings")}
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
                    File(root,"${meta.optString("frame_kind","wipe")}-$token-$elapsed.rgba").writeBytes(rgba)
                    runOnUiThread{timeline.post(cancel={bitmap.recycle()}){if(!storyClosing){
                        val finished=if(kind=="motion")meta.getBoolean("finished") else false
                        ready(bitmap,finished)
                        if(kind=="motion"&&elapsed>0&&!finished&&intent.getBooleanExtra("close_on_motion",false)&&!intent.getBooleanExtra("motion_pause_used",false)&&(!intent.hasExtra("close_on_motion_sequence")||token==intent.getIntExtra("close_on_motion_sequence",0).toLong())){
                            intent.putExtra("motion_pause_used",true);moveTaskToBack(true)
                        }
                    }}}
                }
            }catch(error:Throwable){fail(error)}}
        }){error->worker.execute{fail(error)}}
        show={packet->
            if(storyClosing) Unit else {
                val split=packet.indexOf(10.toByte());check(split>=0)
                val metadata=JSONObject(String(packet,0,split,Charsets.UTF_8))
                if(metadata.has("error"))File(root,"boundary.json").writeText(metadata.toString())
                check(!metadata.has("error")){metadata.optString("error")}
                check(packet.contentEquals(story(intArrayOf(),1,0))){"poll advanced story"}
                val payload=packet.copyOfRange(split+1,packet.size)
                if(metadata.has("host_effect")) {
                    val token=metadata.getLong("token");val kind=metadata.getString("host_effect")
                    File(root,"effect-$token.json").writeText(metadata.toString())
                    if(kind in listOf("fade","wipe","shake","flash"))File(root,"effect-$token.png").writeBytes(payload)
                    val bitmap=if(kind in listOf("fade","wipe","shake","flash"))BitmapFactory.decodeByteArray(payload,0,payload.size) ?: error("invalid fade bitmap") else null
                    val file=if(kind in listOf("bgm","env","se","voice"))File(root,"audio-$token.ogg").also{
                        it.writeBytes(payload)
                        val digest=java.security.MessageDigest.getInstance("SHA-256").digest(payload).joinToString(""){b->"%02x".format(b.toInt() and 255)}
                        File(root,"audio-$token.json").writeText(JSONObject().put("kind",kind).put("name",metadata.getString("name")).put("sha256",digest).toString())
                    } else null
                    val shortClip=try {
                        if(file!=null && (kind=="voice" || (kind=="se" && !metadata.getBoolean("loop"))))ShortClip.read(file) else null
                    }catch(error:Exception){file?.delete();throw error}
                    val movieUri=if(kind=="movie")selectedFiles.single{it.first.equals(metadata.getString("name"),true)}.second else null
                    runOnUiThread{timeline.post(cancel={file?.delete();bitmap?.recycle()}){
                        if(storyClosing){file?.delete()}else {
                            if(kind=="shake" || kind=="wipe") {
                                val old=(image.drawable as? android.graphics.drawable.BitmapDrawable)?.bitmap
                                if(old!=null)File(root,"$kind-$token-old.png").outputStream().use{check(old.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it))}
                            }
                            // Probe-only lifecycle injection: preserve the original duration, background during the real request.
                            if((kind=="shake" && intent.getBooleanExtra("close_on_shake",false)) || (kind=="flash" && intent.getBooleanExtra("close_on_flash",false)))image.postDelayed({if(!storyClosing)moveTaskToBack(true)},if(kind=="flash")5L else 35L)
                            if(kind==intent.getStringExtra("pause_on_effect")&&!intent.getBooleanExtra("effect_pause_used",false)){
                                intent.putExtra("effect_pause_used",true)
                                image.postDelayed({if(!storyClosing)moveTaskToBack(true)},80)
                            }
                            if(kind=="movie"){
                                storyEffects!!.beginMovie(token)
                                movieHost=MovieHost(this,stage,root)
                                movieHost!!.start(movieUri!!,token,metadata.getBoolean("movie_left_skip"),{reason->
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
                    File(root,"page-$token.json").writeText(metadata.toString());File(root,"page-$token.png").writeBytes(payload)
                    runOnUiThread {timeline.post(cancel={bitmap.recycle()}){if(!isDestroyed&&!storyClosing){sequence=token;dialogue.show(token,metadata.getString("speaker"))
                        if(metadata.has("motion")){
                            storyEffects!!.runMotion(token)
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
                val savedRead=getPreferences(MODE_PRIVATE).getString(readKey,null)
                if(savedRead!=null){
                    val bytes=android.util.Base64.decode(savedRead,android.util.Base64.NO_WRAP)
                    val state=JSONObject(String(story(bytes.map{it.toInt() and 255}.toIntArray(),32,0),Charsets.UTF_8))
                    check(state.optBoolean("imported")){state.toString()}
                }
                applyAudioConfig(story(intArrayOf(),9,0))
                applyTextConfig(story(intArrayOf(),13,0))
                val font=File("/system/fonts/NotoSansCJK-Regular.ttc")
                ParcelFileDescriptor.open(font,ParcelFileDescriptor.MODE_READ_ONLY).use{fd->
                    val result=JSONObject(String(story(intArrayOf(fd.fd),15,0),Charsets.UTF_8));check(!result.has("error")){result.optString("error")}
                    result.put("host_font_source",font.absolutePath).put("font_collection_index",0)
                    File(root,"dialogue-font.json").writeText(result.toString())
                }
                show(packet)
        }
        val configControls=LinearLayout(this);configControls.orientation=LinearLayout.VERTICAL;configControls.visibility=android.view.View.GONE
        val configNotice=TextView(this);configNotice.text="已接通：文字/自动速度、跳过未读、粗体/边框、动态示例、透明度、音量、语音保留、快捷键编辑；快捷执行支持设置、自动、快进、退出。";configControls.addView(configNotice)
        val configButtons=LinearLayout(this);val configBack=Button(this);configBack.text="返回标题";val configReset=Button(this);configReset.text="恢复已接通选项默认值"
        configButtons.addView(configBack,LinearLayout.LayoutParams(0,-2,1f));configButtons.addView(configReset,LinearLayout.LayoutParams(0,-2,1f));configControls.addView(configButtons);layout.addView(configControls)
        lateinit var titleView:TitleView
        fun showTitle(packet:ByteArray,unlock:Boolean){
            val split=packet.indexOf(10.toByte());check(split>=0){String(packet,Charsets.UTF_8)}
            val metadata=JSONObject(String(packet,0,split,Charsets.UTF_8));check(!metadata.has("error")){metadata.optString("error")}
            if(metadata.optString("kind")=="story_settings_closed"){
                check(metadata.getLong("sequence")==sequence){"settings changed dialogue token"}
                check(story(intArrayOf(),1,0).contentEquals(File(root,"settings-story-checkpoint.bin").readBytes())){"settings advanced the script"}
                persistConfig();applyAudioConfig(story(intArrayOf(),9,0),menuTimeline);applyTextConfig(story(intArrayOf(),13,0),menuTimeline)
                runOnUiThread{menuTimeline.post{if(!storyClosing){try{
                    stage.removeView(titleView);configControls.visibility=android.view.View.GONE
                    dialogueView.visibility=android.view.View.VISIBLE;controls.visibility=android.view.View.VISIBLE
                    storyConfigOpen=false;timeline.resume();storyEffects!!.resume();dialogue.resume()
                    File(root,"story-settings-events.jsonl").appendText(JSONObject(metadata.toString()).put("event","closed").put("active_ms",timeline.now()).toString()+"\n")
                }catch(error:Throwable){lifecycleFailure(error)}}}}
                return
            }
            val isSettings=metadata.optString("kind")=="settings"
            if(metadata.optString("kind")!="title" && !isSettings){
                File(root,"title-started.json").writeText(metadata.toString())
                runOnUiThread{timeline.post{stage.removeView(titleView);dialogueView.visibility=android.view.View.VISIBLE;controls.visibility=android.view.View.VISIBLE;text.visibility=android.view.View.VISIBLE;text.text="正在读取剧情…"}}
                enterStory(packet);return
            }
            if(isSettings)persistConfig()
            val prefix=if(isSettings)"settings" else "title"
            val png=packet.copyOfRange(split+1,packet.size);File(root,"$prefix.png").writeBytes(png)
            File(root,"$prefix.json").writeText(metadata.toString())
            val bitmap=BitmapFactory.decodeByteArray(png,0,png.size) ?: error("invalid title PNG")
            runOnUiThread{menuTimeline.post(cancel={bitmap.recycle()}){
                if(!isDestroyed&&!storyClosing){
                    configBack.isEnabled=true;configReset.isEnabled=metadata.optInt("editing_slot",0)==0
                    configBack.text=if(metadata.optInt("editing_slot",0)>0)"取消选择" else if(storyConfigOpen)"返回剧情" else "返回标题"
                    configControls.visibility=if(isSettings)android.view.View.VISIBLE else android.view.View.GONE
                    titleView.contentDescription=if(isSettings)"原版设置页" else "游戏标题菜单"
                    titleView.present(bitmap);if(unlock)titleView.accepting=true
                    val snapshot=File(root,"$prefix-presented.tmp");snapshot.writeText(metadata.toString())
                    check(snapshot.renameTo(File(root,"$prefix-presented.json"))){"cannot publish presented menu state"}
                    text.text="标题主页 · 点击开始游戏"
                    if(isSettings && metadata.getBoolean("unsupported"))android.widget.Toast.makeText(this,"此选项尚未接入",android.widget.Toast.LENGTH_SHORT).show()
                    val action=metadata.optInt("action",0)
                    if(action==4)close.performClick()
                    else if(action!=0)android.widget.Toast.makeText(this,"此入口尚未接入；当前支持开始游戏",android.widget.Toast.LENGTH_SHORT).show()
                }else bitmap.recycle()
            }}
        }
        configBack.setOnClickListener{configBack.isEnabled=false;configReset.isEnabled=false;titleView.cancel();titleView.accepting=false;worker.execute{try{showTitle(story(intArrayOf(),22,0),true)}catch(error:Throwable){fail(error)}}}
        configReset.setOnClickListener{configBack.isEnabled=false;configReset.isEnabled=false;titleView.cancel();titleView.accepting=false;worker.execute{try{showTitle(story(intArrayOf(),22,1),true)}catch(error:Throwable){fail(error)}}}
        titleView=TitleView(this){kind,x,y->
            if(!storyClosing)worker.execute{try{if(!storyClosing)showTitle(story(intArrayOf(kind,x,y),18,0),kind==2)}catch(error:Throwable){fail(error)}}
        }
        originalSettings.setOnClickListener {
            storyConfigOpen=true;dialogue.suspend();timeline.pause();storyEffects!!.pause()
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
                val selected=if(top.any{it.first.endsWith(".dat",true)}) top else {
                    val child=top.single{it.first==game.toString()}
                    children(tree,DocumentsContract.getDocumentId(child.second))
                }
                selectedFiles=selected
                val opened=mutableListOf<ParcelFileDescriptor>()
                var packet=try {
                    for((_,uri) in selected.filter{it.first.endsWith(".dat",true)})opened.add(contentResolver.openFileDescriptor(uri,"r") ?: error("missing file descriptor"))
                    story(opened.map{it.fd}.toIntArray(),if(titleFirst)17 else 0,0)
                }finally{opened.forEach{it.close()}}
                if(titleFirst){
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
        next.setOnClickListener {dialogue.click()}
        textSettings.setOnClickListener {
            val config=textConfig!!;val panel=LinearLayout(this);panel.orientation=LinearLayout.VERTICAL
            val label=TextView(this);label.text="文字速度 ${config.getInt("speed")}%（下一页生效）";panel.addView(label)
            val speed=android.widget.SeekBar(this);speed.max=100;speed.progress=config.getInt("speed");speed.contentDescription="text_speed";panel.addView(speed)
            fun update(field:Long,value:Long){worker.execute{try{if(!storyClosing)applyTextConfig(story(intArrayOf(),14,field,value))}catch(error:Throwable){fail(error)}}}
            speed.setOnSeekBarChangeListener(object:android.widget.SeekBar.OnSeekBarChangeListener{
                override fun onStartTrackingTouch(bar:android.widget.SeekBar){}
                override fun onProgressChanged(bar:android.widget.SeekBar,value:Int,fromUser:Boolean){label.text="文字速度 $value%（下一页生效）"}
                override fun onStopTrackingTouch(bar:android.widget.SeekBar){update(0,bar.progress.toLong())}
            })
            val keep=android.widget.Switch(this);keep.text="换页保留语音";keep.contentDescription="keep_voice";keep.isChecked=config.getInt("keep_voice")==1;panel.addView(keep)
            keep.setOnCheckedChangeListener{_,checked->update(1,if(checked)1 else 0)}
            android.app.AlertDialog.Builder(this).setTitle("文字（当前会话）").setView(panel).setPositiveButton("返回剧情",null).show()
        }
        settings.setOnClickListener {
            val config=audioConfig!!
            val panel=LinearLayout(this);panel.orientation=LinearLayout.VERTICAL
            val names=listOf("BGM","语音","音效","环境音","系统音")
            for((index,kind) in listOf("bgm","voice","se","env","system").withIndex()){
                val label=TextView(this);label.text="${names[index]} ${config.getInt(kind)}%";panel.addView(label)
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
            android.app.AlertDialog.Builder(this).setTitle("音量（当前会话）").setView(panel).setPositiveButton("返回剧情",null).show()
        }
        close.setOnClickListener {
            next.isEnabled=false;close.isEnabled=false;storyClosing=true;dialogue.stop();movieHost?.close();storyEffects?.close()
            worker.execute{try{val result=story(intArrayOf(),3,0);check(String(result,Charsets.UTF_8).contains("\"closed\":true"));File(root,"closed.txt").writeText("closed; VM live bytes=0");runOnUiThread{finish()}}catch(error:Throwable){fail(error)}}
        }
    }
    override fun onKeyDown(keyCode:Int,event:android.view.KeyEvent):Boolean {
        if(keyCode in android.view.KeyEvent.KEYCODE_F1..android.view.KeyEvent.KEYCODE_F12 && shortcutKey!=null)return true
        return super.onKeyDown(keyCode,event)
    }
    override fun onKeyUp(keyCode:Int,event:android.view.KeyEvent):Boolean {
        if(keyCode in android.view.KeyEvent.KEYCODE_F1..android.view.KeyEvent.KEYCODE_F12 && shortcutKey!=null){
            if(!event.isCanceled)shortcutKey!!.invoke(keyCode-android.view.KeyEvent.KEYCODE_F1+1)
            return true
        }
        return super.onKeyUp(keyCode,event)
    }
    @Suppress("DEPRECATION")
    override fun onBackPressed() {
        if(settingsBack?.invoke()!=true)super.onBackPressed()
    }
    override fun onPause() {
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
        lifecycleGeneration++
        menuTimeline.resume();if(!storyConfigOpen){timeline.resume();try{storyEffects?.resume();movieHost?.resume()}catch(error:Throwable){lifecycleFailure(error)}}
        storyRunRoot?.let{File(it,"lifecycle-events.jsonl").appendText(JSONObject().put("event","resumed").put("time_ms",android.os.SystemClock.elapsedRealtime()).put("active_ms",timeline.now()).put("instance",instanceId).toString()+"\n")}
    }
    override fun onDestroy() {
        settingsBack=null;shortcutKey=null
        storyClosing=true;menuTimeline.close();timeline.close();movieHost?.close();storyEffects?.close()
        storyWorker?.let {worker->worker.execute{if(storyLoaded){val result=story(intArrayOf(),3,0);File(storyRunRoot!!,"lifecycle-closed.json").writeBytes(result)}};worker.shutdown()}
        super.onDestroy()
    }

}
