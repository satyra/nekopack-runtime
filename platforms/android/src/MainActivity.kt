package org.nekopack.probe

import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.os.ParcelFileDescriptor
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ImageView
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
    private external fun story(descriptors:IntArray,operation:Int,sequence:Long):ByteArray
    private var storyWorker:java.util.concurrent.ExecutorService?=null
    private var storyLoaded=false
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
        val root=File(filesDir,runId);root.mkdirs()
        val layout=LinearLayout(this);layout.orientation=LinearLayout.VERTICAL
        val image=ImageView(this);image.scaleType=ImageView.ScaleType.FIT_CENTER
        layout.addView(image,LinearLayout.LayoutParams(-1,0,1f))
        val text=TextView(this);text.text="正在读取剧情…";text.textSize=18f;layout.addView(text)
        val next=Button(this);next.text="继续";next.isEnabled=false;layout.addView(next)
        val close=Button(this);close.text="结束预览（无音频／转场）";layout.addView(close)
        setContentView(layout)
        val worker=Executors.newSingleThreadExecutor();storyWorker=worker
        var sequence=0L
        fun show(packet:ByteArray) {
            val split=packet.indexOf(10.toByte());check(split>=0)
            val metadata=JSONObject(String(packet,0,split,Charsets.UTF_8))
            check(!metadata.has("error")){metadata.optString("error")}
            val token=metadata.getLong("sequence")
            val png=packet.copyOfRange(split+1,packet.size)
            val bitmap=BitmapFactory.decodeByteArray(png,0,png.size) ?: error("invalid story bitmap")
            File(root,"page-$token.json").writeText(metadata.toString())
            File(root,"page-$token.png").writeBytes(png)
            // Polling may not acknowledge dialogue or move the PC.
            check(packet.contentEquals(story(intArrayOf(),1,0))){"poll advanced story"}
            runOnUiThread {if(!isDestroyed){sequence=token;image.setImageBitmap(bitmap);text.text=metadata.getString("speaker")+"\n"+metadata.getString("text");next.isEnabled=true}}
        }
        fun fail(error:Throwable) {
            File(root,"error.txt").writeText("${error.javaClass.name}: ${error.message}")
            if(storyLoaded)story(intArrayOf(),3,0)
            runOnUiThread{if(!isDestroyed){text.text="无法继续：${error.message}";next.isEnabled=false}}
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
            next.isEnabled=false;val token=sequence
            worker.execute{try{show(story(intArrayOf(),2,token))}catch(error:Throwable){fail(error)}}
        }
        close.setOnClickListener {
            next.isEnabled=false;close.isEnabled=false
            worker.execute{try{val result=story(intArrayOf(),3,0);check(String(result,Charsets.UTF_8).contains("\"closed\":true"));File(root,"closed.txt").writeText("closed; VM live bytes=0");runOnUiThread{finish()}}catch(error:Throwable){fail(error)}}
        }
    }
    override fun onDestroy() {
        storyWorker?.let {worker->worker.execute{if(storyLoaded)story(intArrayOf(),3,0)};worker.shutdown()}
        super.onDestroy()
    }

}
