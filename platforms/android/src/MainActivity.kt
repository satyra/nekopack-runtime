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
    private var storyWorker:java.util.concurrent.ExecutorService?=null
    private var storyLoaded=false
    private var storyEffects:StoryEffects?=null
    private var storyRunRoot:File?=null
    @Volatile private var storyClosing=false
    private external fun runDat(descriptors: IntArray, choice: Int): ByteArray
    private external fun runChecks(root: ByteArray): ByteArray

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if(intent.hasExtra("story_game")) {
            val saved=getPreferences(MODE_PRIVATE).getString("dat_tree",null) ?: error("select DAT tree through SAF first")
            startStory(Uri.parse(saved),intent.getIntExtra("story_game",0))
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
        runTree(tree)
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

    private fun startStory(tree:Uri,game:Int) {
        require(game in 0..3)
        val runId=intent.getStringExtra("run_id") ?: "story-manual"
        require(Regex("[a-zA-Z0-9_-]{1,64}").matches(runId))
        val root=File(filesDir,runId);root.mkdirs();storyRunRoot=root
        val layout=LinearLayout(this);layout.orientation=LinearLayout.VERTICAL
        val image=ImageView(this);image.scaleType=ImageView.ScaleType.FIT_CENTER
        val stage=FrameLayout(this);stage.setBackgroundColor(android.graphics.Color.BLACK)
        val overlay=ImageView(this);overlay.scaleType=ImageView.ScaleType.FIT_CENTER;overlay.alpha=0f
        stage.addView(image,FrameLayout.LayoutParams(-1,-1));stage.addView(overlay,FrameLayout.LayoutParams(-1,-1))
        layout.addView(stage,LinearLayout.LayoutParams(-1,0,1f))
        val text=TextView(this);text.text="正在读取剧情…";text.textSize=16f;text.maxLines=3;layout.addView(text)
        val next=Button(this);next.text="继续";next.isEnabled=false;val controls=LinearLayout(this);controls.addView(next,LinearLayout.LayoutParams(0,-2,1f))
        val close=Button(this);close.text="结束运行";controls.addView(close,LinearLayout.LayoutParams(0,-2,1f));layout.addView(controls)
        setContentView(layout)
        layout.post{File(root,"layout.json").writeText(JSONObject().put("width",layout.width).put("height",layout.height).put("stage_width",stage.width).put("stage_height",stage.height).put("orientation",resources.configuration.orientation).toString())}
        val worker=Executors.newSingleThreadExecutor();storyWorker=worker
        var sequence=0L

        lateinit var show:(ByteArray)->Unit
        fun fail(error:Throwable) {
            storyClosing=true
            File(root,"error.txt").writeText("${error.javaClass.name}: ${error.message}")
            if(storyLoaded)story(intArrayOf(),3,0)
            runOnUiThread{storyEffects?.close();if(!isDestroyed){text.text="无法继续：${error.message}";next.isEnabled=false}}
        }
        storyEffects=StoryEffects(this,image,overlay,root,{token,elapsed,ready->
            worker.execute{try{
                if(!storyClosing){
                    val packet=story(intArrayOf(),5,token,elapsed);val split=packet.indexOf(10.toByte());check(split>=0)
                    val meta=JSONObject(String(packet,0,split,Charsets.UTF_8));check(!meta.has("error")){meta.optString("error")}
                    check(meta.getString("format")=="RGBA8")
                    val width=meta.getInt("width");val height=meta.getInt("height")
                    val rgba=packet.copyOfRange(split+1,packet.size);check(rgba.size.toLong()==width.toLong()*height*4)
                    val pixels=IntArray(width*height)
                    for(index in pixels.indices){val at=index*4;pixels[index]=((rgba[at+3].toInt() and 255) shl 24) or ((rgba[at].toInt() and 255) shl 16) or ((rgba[at+1].toInt() and 255) shl 8) or (rgba[at+2].toInt() and 255)}
                    val bitmap=android.graphics.Bitmap.createBitmap(pixels,width,height,android.graphics.Bitmap.Config.ARGB_8888)
                    File(root,"wipe-$token-$elapsed.rgba").writeBytes(rgba)
                    runOnUiThread{if(!storyClosing)ready(bitmap)}
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
                    if(kind in listOf("fade","wipe"))File(root,"effect-$token.png").writeBytes(payload)
                    val bitmap=if(kind in listOf("fade","wipe"))BitmapFactory.decodeByteArray(payload,0,payload.size) ?: error("invalid fade bitmap") else null
                    val file=if(kind in listOf("bgm","se","voice"))File(root,"audio-$token.ogg").also{
                        it.writeBytes(payload)
                        val digest=java.security.MessageDigest.getInstance("SHA-256").digest(payload).joinToString(""){b->"%02x".format(b.toInt() and 255)}
                        File(root,"audio-$token.json").writeText(JSONObject().put("kind",kind).put("name",metadata.getString("name")).put("sha256",digest).toString())
                    } else null
                    runOnUiThread{
                        if(storyClosing){file?.delete()}else storyEffects!!.run(metadata,bitmap,file){
                            if(!storyClosing)worker.execute{try{show(story(intArrayOf(),4,token))}catch(error:Throwable){fail(error)}}
                        }
                    }
                }else {
                    val token=metadata.getLong("sequence")
                    val bitmap=BitmapFactory.decodeByteArray(payload,0,payload.size) ?: error("invalid story bitmap")
                    File(root,"page-$token.json").writeText(metadata.toString());File(root,"page-$token.png").writeBytes(payload)
                    runOnUiThread {if(!isDestroyed&&!storyClosing){sequence=token;image.setImageBitmap(bitmap);text.text=metadata.getString("speaker")+"\n"+metadata.getString("text");next.isEnabled=true}}
                }
            }
        }
        worker.execute {
            try {
                System.loadLibrary("nekopack_host");storyLoaded=true
                val top=children(tree,DocumentsContract.getTreeDocumentId(tree))
                val selected=if(top.any{it.first.endsWith(".dat",true)}) top else {
                    val child=top.single{it.first==game.toString()}
                    children(tree,DocumentsContract.getDocumentId(child.second))
                }
                val opened=mutableListOf<ParcelFileDescriptor>()
                val packet=try {
                    for((_,uri) in selected.filter{it.first.endsWith(".dat",true)})opened.add(contentResolver.openFileDescriptor(uri,"r") ?: error("missing file descriptor"))
                    story(opened.map{it.fd}.toIntArray(),0,0)
                }finally{opened.forEach{it.close()}}
                show(packet)
            }catch(error:Throwable){fail(error)}
        }
        next.setOnClickListener {
            next.isEnabled=false;storyEffects?.stopVoice();val token=sequence
            worker.execute{try{show(story(intArrayOf(),2,token))}catch(error:Throwable){fail(error)}}
        }
        close.setOnClickListener {
            next.isEnabled=false;close.isEnabled=false;storyClosing=true;storyEffects?.close()
            worker.execute{try{val result=story(intArrayOf(),3,0);check(String(result,Charsets.UTF_8).contains("\"closed\":true"));File(root,"closed.txt").writeText("closed; VM live bytes=0");runOnUiThread{finish()}}catch(error:Throwable){fail(error)}}
        }
    }
    override fun onStop() {
        super.onStop()
        if(storyWorker!=null && !isFinishing){storyClosing=true;storyEffects?.close();finish()}
    }
    override fun onDestroy() {
        storyClosing=true;storyEffects?.close()
        storyWorker?.let {worker->worker.execute{if(storyLoaded){val result=story(intArrayOf(),3,0);File(storyRunRoot!!,"lifecycle-closed.json").writeBytes(result)}};worker.shutdown()}
        super.onDestroy()
    }

}
