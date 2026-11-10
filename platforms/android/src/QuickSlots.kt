package org.nekopack.probe

import android.util.AtomicFile
import org.json.JSONArray
import java.io.File
import java.util.UUID

/** Newest-first ten-entry index. Publish only after the native snapshot is synced. */
class QuickSlots(private val directory:File) {
    private val index=AtomicFile(File(directory,"quick-index.json"))
    private fun names():List<String> {
        val bytes=try { index.readFully() } catch(e:java.io.FileNotFoundException) {
            if(index.baseFile.exists())throw e
            return emptyList()
        }
        check(bytes.size<=4096){"quick index exceeds budget"}
        val array=JSONArray(String(bytes,Charsets.UTF_8));check(array.length()<=10){"invalid quick slot count"}
        val names=(0 until array.length()).map{array.getString(it)}
        check(names.distinct().size==names.size && names.all{Regex("quick-[a-f0-9-]{36}\\.nksv").matches(it)}){"invalid quick slot index"}
        return names
    }
    fun slot(id:Int):File? {
        require(id in 1..10)
        return names().getOrNull(id-1)?.let{File(directory,it)}
    }
    fun write(export:(File)->Unit):File {
        val old=names()
        // Reclaim unpublished or evicted generations before creating the next one.
        val files=directory.listFiles() ?: error("cannot list quick slot directory")
        for(file in files)if(file.name.startsWith("quick-")&&file.extension=="nksv"&&file.name !in old){
            check(file.delete()){"cannot remove stale quick snapshot"}
        }
        val target=File(directory,"quick-${UUID.randomUUID()}.nksv")
        export(target)
        val next=listOf(target.name)+old.take(9)
        val stream=index.startWrite()
        try { stream.write(JSONArray(next).toString().toByteArray(Charsets.UTF_8));index.finishWrite(stream) }
        catch(e:Exception){index.failWrite(stream);throw e}
        return target
    }
}
