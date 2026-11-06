package org.nekopack.probe

import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.os.ParcelFileDescriptor
import android.widget.Button
import android.app.Activity
import android.os.Bundle
import android.os.Build
import android.system.Os
import android.system.OsConstants
import android.widget.TextView
import java.io.File

/** Validation shell only: all VM lifetime stays within one native call on its worker. */
class MainActivity : Activity() {
    private external fun runDat(descriptors: IntArray, choice: Int): ByteArray
    private external fun runChecks(root: ByteArray): ByteArray

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
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

}
