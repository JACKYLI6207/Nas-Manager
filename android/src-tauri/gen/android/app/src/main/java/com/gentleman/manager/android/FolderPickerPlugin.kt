package com.gentleman.manager.android

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import androidx.documentfile.provider.DocumentFile
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.io.BufferedReader
import java.io.InputStreamReader
import java.util.regex.Pattern
import java.util.UUID

@TauriPlugin
class FolderPickerPlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun pickOpenZip(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(
                Intent.EXTRA_MIME_TYPES,
                arrayOf("application/zip", "application/x-cbz", "application/octet-stream")
            )
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "pickOpenZipResult")
    }

    private fun resolvePickCancelled(invoke: Invoke) {
        val ret = JSObject()
        ret.put("cancelled", true)
        ret.put("uri", "")
        invoke.resolve(ret)
    }

    private fun resolveOnUi(invoke: Invoke, ret: JSObject) {
        activity.runOnUiThread { invoke.resolve(ret) }
    }

    private fun resolvePickCancelledOnUi(invoke: Invoke) {
        activity.runOnUiThread { resolvePickCancelled(invoke) }
    }

    private fun rejectOnUi(invoke: Invoke, message: String) {
        activity.runOnUiThread { invoke.reject(message) }
    }

    @ActivityCallback
    fun pickOpenZipResult(invoke: Invoke, result: androidx.activity.result.ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            resolvePickCancelledOnUi(invoke)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            rejectOnUi(invoke, "未取得檔案 URI")
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION
            )
        } catch (_: SecurityException) {
        }
        val ret = JSObject()
        ret.put("uri", uri.toString())
        resolveOnUi(invoke, ret)
    }

    @Command
    fun pickOpenTxt(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "text/plain"
            putExtra(
                Intent.EXTRA_MIME_TYPES,
                arrayOf("text/plain", "application/octet-stream", "*/*")
            )
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "pickOpenTxtResult")
    }

    @ActivityCallback
    fun pickOpenTxtResult(invoke: Invoke, result: androidx.activity.result.ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            resolvePickCancelled(invoke)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            invoke.reject("未取得檔案 URI")
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            )
        } catch (_: SecurityException) {
        }
        val ret = JSObject()
        ret.put("uri", uri.toString())
        val name = DocumentFile.fromSingleUri(activity, uri)?.name
        if (!name.isNullOrBlank()) {
            ret.put("name", name)
        }
        invoke.resolve(ret)
    }

    @Command
    fun pickOpenArchive(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(
                Intent.EXTRA_MIME_TYPES,
                arrayOf("application/json", "application/octet-stream", "text/plain", "*/*")
            )
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "pickOpenArchiveResult")
    }

    @ActivityCallback
    fun pickOpenArchiveResult(invoke: Invoke, result: androidx.activity.result.ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            resolvePickCancelled(invoke)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            invoke.reject("未取得檔案 URI")
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION
            )
        } catch (_: SecurityException) {
        }
        val ret = JSObject()
        ret.put("uri", uri.toString())
        val name = DocumentFile.fromSingleUri(activity, uri)?.name
        if (!name.isNullOrBlank()) {
            ret.put("name", name)
        }
        invoke.resolve(ret)
    }

    @Command
    fun appendLineToDocument(invoke: Invoke) {
        val args = parseAppendLineArgs(invoke)
        val uri = Uri.parse(args.uri)
        val line = args.line.trim()
        if (line.isEmpty()) {
            invoke.reject("追加內容不可為空")
            return
        }
        Thread {
            try {
                val input = activity.contentResolver.openInputStream(uri)
                    ?: throw IllegalStateException("無法讀取 TXT 檔案")
                val existing = input.bufferedReader().use { it.readText() }
                if (existing.lines().any { it.trim() == line }) {
                    activity.runOnUiThread { invoke.resolve(JSObject()) }
                    return@Thread
                }
                val outputText = buildString {
                    append(existing)
                    if (existing.isNotEmpty()) {
                        when {
                            existing.endsWith("\n\n") || existing.endsWith("\n\r\n") -> { }
                            existing.endsWith("\n") -> append('\n')
                            else -> append("\n\n")
                        }
                    }
                    append(line)
                    append('\n')
                }
                val output = activity.contentResolver.openOutputStream(uri, "wt")
                    ?: throw IllegalStateException("無法寫入 TXT 檔案")
                output.use { stream ->
                    stream.write(outputText.toByteArray(Charsets.UTF_8))
                    stream.flush()
                }
                activity.runOnUiThread { invoke.resolve(JSObject()) }
            } catch (ex: Exception) {
                activity.runOnUiThread {
                    invoke.reject(ex.message ?: "追加 TXT 行失敗")
                }
            }
        }.start()
    }

    @Command
    fun removeLineFromDocument(invoke: Invoke) {
        val args = parseAppendLineArgs(invoke)
        val uri = Uri.parse(args.uri)
        val line = args.line.trim()
        if (line.isEmpty()) {
            invoke.reject("移除內容不可為空")
            return
        }
        Thread {
            try {
                val input = activity.contentResolver.openInputStream(uri)
                    ?: throw IllegalStateException("無法讀取 TXT 檔案")
                val existing = input.bufferedReader().use { it.readText() }
                val kept = existing.lines().filter { it.trim() != line }.toMutableList()
                val removed = kept.size != existing.lines().count()
                if (!removed) {
                    val ret = JSObject()
                    ret.put("removed", false)
                    activity.runOnUiThread { invoke.resolve(ret) }
                    return@Thread
                }
                while (kept.isNotEmpty() && kept.last().trim().isEmpty()) {
                    kept.removeAt(kept.lastIndex)
                }
                val outputText = if (kept.isEmpty()) "" else kept.joinToString("\n") + "\n"
                val output = activity.contentResolver.openOutputStream(uri, "wt")
                    ?: throw IllegalStateException("無法寫入 TXT 檔案")
                output.use { stream ->
                    stream.write(outputText.toByteArray(Charsets.UTF_8))
                    stream.flush()
                }
                val ret = JSObject()
                ret.put("removed", true)
                activity.runOnUiThread { invoke.resolve(ret) }
            } catch (ex: Exception) {
                activity.runOnUiThread {
                    invoke.reject(ex.message ?: "移除 TXT 行失敗")
                }
            }
        }.start()
    }

    private fun isSeriesIncompleteArtifact(name: String, isDirectory: Boolean): Boolean {
        if (isDirectory) {
            return name.startsWith(".下載中-") || name.startsWith(".")
        }
        return name.endsWith(".part", ignoreCase = true) || name == "元數據.json"
    }

    private fun isSeriesCompletedContent(name: String, isDirectory: Boolean): Boolean {
        if (isSeriesIncompleteArtifact(name, isDirectory)) return false
        if (isDirectory) return true
        return name.endsWith(".zip", ignoreCase = true)
    }

    private fun subdirectoryHasDownloadedContent(dir: DocumentFile): Boolean {
        for (child in dir.listFiles() ?: emptyArray()) {
            val name = child.name?.trim().orEmpty()
            if (isSeriesCompletedContent(name, child.isDirectory)) {
                return true
            }
        }
        return false
    }

    @Command
    fun subdirectoryHasDownloadedContent(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SubdirectoryArgs::class.java)
            val root = DocumentFile.fromTreeUri(activity, Uri.parse(args.treeUri))
                ?: throw IllegalStateException("無法讀取下載目錄")
            val subdir = root.findFile(args.subdirectoryName.trim())
                ?: throw IllegalStateException("找不到子目錄")
            if (!subdir.isDirectory) {
                throw IllegalStateException("不是資料夾")
            }
            val ret = JSObject()
            ret.put("hasDownloadedContent", subdirectoryHasDownloadedContent(subdir))
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "檢查子目錄內容失敗")
        }
    }

    @Command
    fun tryRemoveEmptySubdirectory(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SubdirectoryArgs::class.java)
            val root = DocumentFile.fromTreeUri(activity, Uri.parse(args.treeUri))
                ?: throw IllegalStateException("無法讀取下載目錄")
            val subdirName = args.subdirectoryName.trim()
            val subdir = root.findFile(subdirName)
            val ret = JSObject()
            if (subdir == null || !subdir.isDirectory) {
                ret.put("removed", false)
                invoke.resolve(ret)
                return
            }
            if (subdirectoryHasDownloadedContent(subdir)) {
                ret.put("removed", false)
                invoke.resolve(ret)
                return
            }
            val removed = subdir.delete()
            ret.put("removed", removed)
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "刪除空子目錄失敗")
        }
    }

    private fun parseAppendLineArgs(invoke: Invoke): AppendLineArgs {
        val raw = invoke.getArgs()
        val uri = raw.getString("uri", "")?.trim().orEmpty()
        val line = raw.getString("line", "")?.trim().orEmpty()
        if (uri.isNotEmpty() && line.isNotEmpty()) {
            return AppendLineArgs(uri, line)
        }
        return invoke.parseArgs(AppendLineArgs::class.java)
    }

    @Command
    fun cacheDocumentToFile(invoke: Invoke) {
        val args = invoke.parseArgs(ReadArgs::class.java)
        val uri = Uri.parse(args.uri)
        Thread {
            try {
                val name = DocumentFile.fromSingleUri(activity, uri)?.name ?: "snapshot-cache.json"
                val safeName = name.replace(Regex("[^a-zA-Z0-9._-]"), "_")
                val outFile = java.io.File(activity.cacheDir, "gm-snap-${UUID.randomUUID()}-$safeName")
                val input = activity.contentResolver.openInputStream(uri)
                    ?: throw IllegalStateException("無法讀取檔案")
                input.use { stream ->
                    outFile.outputStream().use { output ->
                        val buffer = ByteArray(256 * 1024)
                        while (true) {
                            val read = stream.read(buffer)
                            if (read <= 0) break
                            output.write(buffer, 0, read)
                        }
                    }
                }
                val ret = JSObject()
                ret.put("path", outFile.absolutePath)
                activity.runOnUiThread { invoke.resolve(ret) }
            } catch (ex: Exception) {
                activity.runOnUiThread {
                    invoke.reject(ex.message ?: "快取檔案失敗")
                }
            }
        }.start()
    }

    @Command
    fun readBytes(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(ReadArgs::class.java)
            val uri = Uri.parse(args.uri)
            val stream = activity.contentResolver.openInputStream(uri)
                ?: throw IllegalStateException("無法開啟檔案")
            val bytes = stream.use { it.readBytes() }
            val ret = JSObject()
            ret.put("base64", android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP))
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "讀取失敗")
        }
    }

    @Command
    fun listReaderSources(invoke: Invoke) {
        val args = try {
            invoke.parseArgs(TreeArgs::class.java)
        } catch (ex: Exception) {
            rejectOnUi(invoke, ex.message ?: "參數錯誤")
            return
        }
        Thread {
            try {
                val treeUri = Uri.parse(args.treeUri)
                val root = DocumentFile.fromTreeUri(activity, treeUri)
                    ?: throw IllegalStateException("無法讀取目錄")
                val sources = JSArray()
                collectReaderSources(root, sources)
                val ret = JSObject()
                ret.put("sources", sources)
                resolveOnUi(invoke, ret)
            } catch (ex: Exception) {
                rejectOnUi(invoke, ex.message ?: "掃描目錄失敗")
            }
        }.start()
    }

    @Command
    fun loadReaderPages(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(ReadArgs::class.java)
            val uri = Uri.parse(args.uri)
            val doc = DocumentFile.fromSingleUri(activity, uri)
                ?: throw IllegalStateException("無法開啟來源")
            val pages = JSArray()
            val title = doc.name?.substringBeforeLast('.') ?: "本地漫畫"
            if (doc.isDirectory) {
                collectImagePages(doc, pages, uri.toString())
            } else {
                invoke.reject("ZIP 請先快取後由系統解析")
                return
            }
            val ret = JSObject()
            ret.put("title", title)
            ret.put("pages", pages)
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "載入頁面失敗")
        }
    }

    private fun isImageName(name: String): Boolean {
        val lower = name.lowercase()
        return lower.endsWith(".jpg") || lower.endsWith(".jpeg") || lower.endsWith(".png") ||
            lower.endsWith(".webp") || lower.endsWith(".gif") || lower.endsWith(".bmp")
    }

    private fun isZipName(name: String): Boolean {
        val lower = name.lowercase()
        return lower.endsWith(".zip") || lower.endsWith(".cbz")
    }

    private fun isShoucang(name: String): Boolean = name.equals("shoucang.jpg", ignoreCase = true)

    private fun dirHasImages(dir: DocumentFile): Boolean {
        val children = dir.listFiles() ?: return false
        return children.any { child ->
            !child.isDirectory && isImageName(child.name ?: "") && !isShoucang(child.name ?: "")
        }
    }

    private fun collectReaderSources(dir: DocumentFile, out: JSArray) {
        if (dirHasImages(dir)) {
            val item = JSObject()
            item.put("path", dir.uri.toString())
            item.put("label", "${dir.name ?: "根目錄"}（根目錄）")
            item.put("kind", "folder")
            out.put(item)
        }
        val children = dir.listFiles() ?: return
        for (child in children) {
            if (child.isDirectory) {
                if (dirHasImages(child)) {
                    val item = JSObject()
                    item.put("path", child.uri.toString())
                    item.put("label", child.name ?: "資料夾")
                    item.put("kind", "folder")
                    out.put(item)
                }
                continue
            }
            val name = child.name ?: continue
            if (isZipName(name)) {
                val item = JSObject()
                item.put("path", child.uri.toString())
                item.put("label", name.substringBeforeLast('.'))
                item.put("kind", "zip")
                out.put(item)
            }
        }
    }

    private fun collectImagePages(dir: DocumentFile, out: JSArray, baseUri: String) {
        val children = dir.listFiles()?.toList().orEmpty().sortedBy { it.name ?: "" }
        for (child in children) {
            if (child.isDirectory) continue
            val name = child.name ?: continue
            if (!isImageName(name) || isShoucang(name)) continue
            val item = JSObject()
            item.put("caption", name)
            item.put("pageId", "$baseUri\u001E${child.uri}")
            out.put(item)
        }
    }

    @Command
    fun pickDocumentTree(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "pickDocumentTreeResult")
    }

    @ActivityCallback
    fun pickDocumentTreeResult(invoke: Invoke, result: androidx.activity.result.ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            resolvePickCancelledOnUi(invoke)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            rejectOnUi(invoke, "未取得目錄 URI")
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            )
        } catch (_: SecurityException) {
            // 部分裝置可能已授權
        }
        val ret = JSObject()
        ret.put("uri", uri.toString())
        resolveOnUi(invoke, ret)
    }

    @Command
    fun readText(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(ReadArgs::class.java)
            val uri = Uri.parse(args.uri)
            val stream = activity.contentResolver.openInputStream(uri)
                ?: throw IllegalStateException("無法開啟檔案")
            stream.use { input ->
                val text = BufferedReader(InputStreamReader(input)).readText()
                val ret = JSObject()
                ret.put("text", text)
                invoke.resolve(ret)
            }
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "讀取失敗")
        }
    }

    @Command
    fun listSubdirectoryNames(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(TreeArgs::class.java)
            val root = DocumentFile.fromTreeUri(activity, Uri.parse(args.treeUri))
                ?: throw IllegalStateException("無法讀取目錄")
            val names = JSArray()
            for (child in root.listFiles() ?: emptyArray()) {
                if (!child.isDirectory) continue
                val name = child.name?.trim().orEmpty()
                if (name.isNotEmpty()) {
                    names.put(name)
                }
            }
            val ret = JSObject()
            ret.put("names", names)
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "列出子目錄失敗")
        }
    }

    @Command
    fun listTxtFiles(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(TreeArgs::class.java)
            val treeUri = Uri.parse(args.treeUri)
            val root = DocumentFile.fromTreeUri(activity, treeUri)
                ?: throw IllegalStateException("無法讀取目錄")
            val files = JSArray()
            collectTxtFiles(root, files)
            val ret = JSObject()
            ret.put("files", files)
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "掃描 TXT 失敗")
        }
    }

    private fun collectTxtFiles(dir: DocumentFile, out: JSArray) {
        val children = dir.listFiles() ?: return
        for (child in children) {
            if (child.isDirectory) {
                collectTxtFiles(child, out)
                continue
            }
            val name = child.name ?: continue
            if (!name.endsWith(".txt", ignoreCase = true)) continue
            val item = JSObject()
            item.put("uri", child.uri.toString())
            item.put("name", name)
            out.put(item)
        }
    }

    @Command
    fun listSnapshotFiles(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(TreeArgs::class.java)
            val treeUri = Uri.parse(args.treeUri)
            val root = DocumentFile.fromTreeUri(activity, treeUri)
                ?: throw IllegalStateException("無法讀取目錄")
            val files = JSArray()
            collectSnapshotFiles(root, files)
            val ret = JSObject()
            ret.put("files", files)
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "掃描失敗")
        }
    }

    private fun collectSnapshotFiles(dir: DocumentFile, out: JSArray) {
        val children = dir.listFiles() ?: return
        for (child in children) {
            if (child.isDirectory) {
                if (child.name.equals("old", ignoreCase = true) || child.name.equals("OLD", ignoreCase = true)) continue
                collectSnapshotFiles(child, out)
                continue
            }
            val name = child.name ?: continue
            if (name.startsWith("收藏漫畫存檔_") || name.startsWith("收藏分頁存檔_")) continue
            if (!name.contains("gm-snapshot", ignoreCase = true) &&
                !name.endsWith(".json", ignoreCase = true)
            ) {
                continue
            }
            val uri = child.uri.toString()
            val meta = readMetaSummary(child)
            val item = JSObject()
            if (meta.cateId != null) item.put("cateId", meta.cateId)
            item.put("label", meta.label)
            if (meta.scanTargetKind.isNotBlank()) item.put("scanTargetKind", meta.scanTargetKind)
            item.put("filePath", uri)
            item.put("metaId", meta.metaId)
            item.put("savedAt", meta.savedAt)
            item.put("totalCount", meta.totalCount)
            item.put("totalPages", meta.totalPages)
            item.put("scanCompletionPercent", meta.scanCompletionPercent)
            item.put("scanCompletedPages", meta.scanCompletedPages)
            item.put("modifiedMs", meta.modifiedMs)
            out.put(item)
        }
    }

    private data class MetaSummary(
        val cateId: Long?,
        val label: String,
        val scanTargetKind: String,
        val metaId: String,
        val savedAt: String,
        val totalCount: Long,
        val totalPages: Long,
        val scanCompletionPercent: Long,
        val scanCompletedPages: Long,
        val modifiedMs: Long,
    )

    private fun readMetaSummary(file: DocumentFile): MetaSummary {
        return try {
            val stream = activity.contentResolver.openInputStream(file.uri)
                ?: return MetaSummary(null, defaultLabel(file), "", "", "", 0, 0, 0, 0, 0)
            stream.use { input ->
                val buf = ByteArray(32768)
                val n = input.read(buf)
                val text = String(buf, 0, maxOf(n, 0), Charsets.UTF_8)
                val readLong = { key: String ->
                    Pattern.compile("\"$key\"\\s*:\\s*(-?\\d+)").matcher(text).let { m ->
                        if (m.find()) m.group(1)?.toLongOrNull() ?: 0L else 0L
                    }
                }
                val readStr = { key: String ->
                    Pattern.compile("\"$key\"\\s*:\\s*\"((?:\\\\.|[^\"\\\\])*)\"").matcher(text).let { m ->
                        if (m.find()) {
                            m.group(1)?.replace("\\\"", "\"")?.replace("\\\\", "\\") ?: ""
                        } else ""
                    }
                }
                val cateId = Pattern.compile("\"scanTargetCateId\"\\s*:\\s*(-?\\d+)").matcher(text).let { m ->
                    if (m.find()) m.group(1)?.toLongOrNull() else null
                }
                val label = readStr("scanTargetLabel").ifBlank { defaultLabel(file) }
                val scanTargetKind = readStr("scanTargetKind")
                val modifiedMs = file.lastModified()
                MetaSummary(
                    cateId,
                    label,
                    scanTargetKind,
                    readStr("id"),
                    readStr("savedAt"),
                    readLong("totalCount"),
                    readLong("totalPages"),
                    readLong("scanCompletionPercent"),
                    readLong("scanCompletedPages"),
                    modifiedMs,
                )
            }
        } catch (_: Exception) {
            MetaSummary(null, defaultLabel(file), "", "", "", 0, 0, 0, 0, 0)
        }
    }

    private fun snapshotFilePrefix(fileName: String): String {
        val stem = fileName.removeSuffix(".gm-snapshot.json")
        if (stem.length > 20) {
            val prefix = stem.substring(0, stem.length - 20)
            val timestamp = stem.substring(stem.length - 20).trimStart()
            if (timestamp.matches(Regex("^\\d{4}_\\d{2}_\\d{2}_\\d{2}_\\d{2}_\\d{2}$"))) {
                return prefix
            }
        }
        return stem
    }

    private fun deletePreviousSnapshot(root: DocumentFile, previousUri: String?) {
        val uri = previousUri?.trim()?.ifEmpty { null } ?: return
        val children = root.listFiles() ?: return
        for (child in children) {
            if (!child.isFile) continue
            if (child.uri.toString() == uri) {
                if (!child.delete()) {
                    throw IllegalStateException("無法刪除舊快照")
                }
                return
            }
        }
    }

    @Command
    fun publishSnapshotFile(invoke: Invoke) {
        Thread {
            try {
                val raw = invoke.getArgs()
                val treeUri = raw.getString("treeUri", "")?.trim().orEmpty()
                val fileName = raw.getString("fileName", "")?.trim().orEmpty()
                val sourcePath = raw.getString("sourcePath", "")?.trim().orEmpty()
                val cateId = raw.optLong("cateId", -1L)
                val previousUri = raw.getString("previousSnapshotPath", "")?.trim()?.ifEmpty { null }
                if (treeUri.isEmpty() || fileName.isEmpty() || sourcePath.isEmpty()) {
                    throw IllegalArgumentException("缺少 treeUri / fileName / sourcePath")
                }
                val src = File(sourcePath)
                if (!src.isFile) {
                    throw IllegalStateException("來源快照檔不存在: $sourcePath")
                }
                val root = DocumentFile.fromTreeUri(activity, Uri.parse(treeUri))
                    ?: throw IllegalStateException("無法開啟目標目錄")
                val existing = root.findFile(fileName)
                val target = when {
                    existing == null -> root.createFile("application/json", fileName)
                    existing.isDirectory -> {
                        existing.delete()
                        root.createFile("application/json", fileName)
                    }
                    else -> existing
                } ?: throw IllegalStateException("無法建立快照檔: $fileName")
                src.inputStream().use { input ->
                    tryOpenForWrite(target.uri)?.use { output ->
                        val stream = output ?: throw IllegalStateException("無法寫入快照檔")
                        val buffer = ByteArray(256 * 1024)
                        while (true) {
                            val read = input.read(buffer)
                            if (read <= 0) break
                            stream.write(buffer, 0, read)
                        }
                        stream.flush()
                    }
                }
                deletePreviousSnapshot(root, previousUri)
                val ret = JSObject()
                ret.put("uri", target.uri.toString())
                activity.runOnUiThread { invoke.resolve(ret) }
            } catch (ex: Exception) {
                activity.runOnUiThread {
                    invoke.reject(ex.message ?: "發布快照失敗")
                }
            }
        }.start()
    }

    @Command
    fun writeSnapshotToTree(invoke: Invoke) {
        Thread {
            try {
                val raw = invoke.getArgs()
                val treeUri = raw.getString("treeUri", "")?.trim().orEmpty()
                val fileName = raw.getString("fileName", "")?.trim().orEmpty()
                val content = raw.getString("content", "") ?: ""
                if (treeUri.isEmpty() || fileName.isEmpty()) {
                    throw IllegalArgumentException("缺少 treeUri 或 fileName")
                }
                val root = DocumentFile.fromTreeUri(activity, Uri.parse(treeUri))
                    ?: throw IllegalStateException("無法開啟目錄")
                val existing = root.findFile(fileName)
                val target = when {
                    existing == null -> root.createFile("application/json", fileName)
                    existing.isDirectory -> {
                        existing.delete()
                        root.createFile("application/json", fileName)
                    }
                    else -> existing
                } ?: throw IllegalStateException("無法建立快照檔: $fileName")
                tryOpenForWrite(target.uri)?.use { out ->
                    val stream = out ?: throw IllegalStateException("無法寫入快照檔")
                    val bytes = content.toByteArray(Charsets.UTF_8)
                    var offset = 0
                    val chunk = 256 * 1024
                    while (offset < bytes.size) {
                        val end = minOf(offset + chunk, bytes.size)
                        stream.write(bytes, offset, end - offset)
                        offset = end
                    }
                    stream.flush()
                }
                val ret = JSObject()
                ret.put("uri", target.uri.toString())
                activity.runOnUiThread { invoke.resolve(ret) }
            } catch (ex: Exception) {
                activity.runOnUiThread {
                    invoke.reject(ex.message ?: "寫入快照失敗")
                }
            }
        }.start()
    }

    private fun defaultLabel(file: DocumentFile): String {
        val stem = file.name?.substringBeforeLast('.') ?: return "未命名分類"
        return stem.ifBlank { "未命名分類" }
    }

    @Command
    fun copyFileToTree(invoke: Invoke) {
        try {
            val args = parseCopyToTreeArgs(invoke)
            val root = DocumentFile.fromTreeUri(activity, Uri.parse(args.treeUri))
                ?: throw IllegalStateException("無法開啟目標目錄")
            val src = File(args.sourcePath)
            if (!src.isFile) {
                throw IllegalStateException("來源檔案不存在: ${args.sourcePath}")
            }
            val parts = args.relativePath
                .split('/', '\\')
                .map { it.trim() }
                .filter { it.isNotEmpty() }
            if (parts.isEmpty()) {
                throw IllegalStateException("目標相對路徑不可為空")
            }

            var current = root
            for (part in parts.dropLast(1)) {
                val existing = current.findFile(part)
                val nextDir = when {
                    existing == null -> current.createDirectory(part)
                    existing.isDirectory -> existing
                    else -> {
                        existing.delete()
                        current.createDirectory(part)
                    }
                } ?: throw IllegalStateException("建立子目錄失敗: $part")
                current = nextDir
            }

            val fileName = parts.last()
            val existing = current.findFile(fileName)
            val target = when {
                existing == null -> current.createFile("application/octet-stream", fileName)
                existing.isDirectory -> {
                    existing.delete()
                    current.createFile("application/octet-stream", fileName)
                }
                else -> existing
            } ?: throw IllegalStateException("建立目標檔案失敗: $fileName")

            val output = tryOpenForWrite(target.uri)
            output.use { stream ->
                val outputStream = stream ?: throw IllegalStateException("無法開啟目標檔案輸出串流")
                src.inputStream().use { input ->
                    val buffer = ByteArray(256 * 1024)
                    while (true) {
                        val read = input.read(buffer)
                        if (read <= 0) break
                        outputStream.write(buffer, 0, read)
                    }
                    outputStream.flush()
                }
            }

            val ret = JSObject()
            ret.put("uri", target.uri.toString())
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "複製檔案失敗")
        }
    }

    /** 直接讀 JSON 參數，避免 Release 混淆後 Jackson 無法建構 CopyToTreeArgs。 */
    private fun parseCopyToTreeArgs(invoke: Invoke): CopyToTreeArgs {
        val raw = invoke.getArgs()
        val treeUri = raw.getString("treeUri", "")?.trim().orEmpty()
        val sourcePath = raw.getString("sourcePath", "")?.trim().orEmpty()
        val relativePath = raw.getString("relativePath", "")?.trim().orEmpty()
        if (treeUri.isEmpty() || sourcePath.isEmpty() || relativePath.isEmpty()) {
            throw IllegalArgumentException("copyFileToTree 缺少 treeUri / sourcePath / relativePath")
        }
        return CopyToTreeArgs(treeUri, sourcePath, relativePath)
    }

    private fun tryOpenForWrite(uri: Uri): java.io.OutputStream? {
        // 不同 ROM / provider 支援的 mode 不完全一致，依序降級嘗試
        return activity.contentResolver.openOutputStream(uri, "rwt")
            ?: activity.contentResolver.openOutputStream(uri, "w")
            ?: activity.contentResolver.openOutputStream(uri)
    }

    @Command
    fun pickUploadDocument(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "pickUploadDocumentResult")
    }

    @ActivityCallback
    fun pickUploadDocumentResult(invoke: Invoke, result: androidx.activity.result.ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            resolvePickCancelledOnUi(invoke)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            rejectOnUi(invoke, "未取得檔案 URI")
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION
            )
        } catch (_: SecurityException) {
        }
        val ret = JSObject()
        ret.put("uri", uri.toString())
        val name = DocumentFile.fromSingleUri(activity, uri)?.name
        if (!name.isNullOrBlank()) {
            ret.put("name", name)
        }
        resolveOnUi(invoke, ret)
    }

    @Command
    fun pickUploadFolder(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
            )
        }
        startActivityForResult(invoke, intent, "pickUploadFolderResult")
    }

    @ActivityCallback
    fun pickUploadFolderResult(invoke: Invoke, result: androidx.activity.result.ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            resolvePickCancelledOnUi(invoke)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            rejectOnUi(invoke, "未取得目錄 URI")
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION
            )
        } catch (_: SecurityException) {
        }
        val ret = JSObject()
        ret.put("uri", uri.toString())
        resolveOnUi(invoke, ret)
    }

    @Command
    fun listUploadFiles(invoke: Invoke) {
        try {
            val raw = invoke.getArgs()
            val uriStr = raw.getString("uri", "")?.trim().orEmpty()
            val kind = raw.getString("kind", "file")?.trim().orEmpty()
            if (uriStr.isEmpty()) {
                throw IllegalArgumentException("缺少 uri")
            }
            val files = JSArray()
            if (kind == "tree" || kind == "folder") {
                val root = DocumentFile.fromTreeUri(activity, Uri.parse(uriStr))
                    ?: throw IllegalStateException("無法讀取資料夾")
                val rootName = root.name?.trim().orEmpty()
                collectUploadFiles(root, "", files)
                if (files.length() == 0) {
                    collectUploadFilesViaContract(Uri.parse(uriStr), rootName, files)
                }
                if (rootName.isNotEmpty()) {
                    prependRootFolderName(files, rootName)
                }
                if (files.length() == 0) {
                    throw IllegalStateException("資料夾內沒有可上傳的檔案")
                }
            } else {
                val doc = DocumentFile.fromSingleUri(activity, Uri.parse(uriStr))
                    ?: throw IllegalStateException("無法讀取檔案")
                val name = doc.name ?: "file"
                val item = JSObject()
                item.put("uri", uriStr)
                item.put("relativePath", name)
                item.put("size", doc.length())
                files.put(item)
            }
            val ret = JSObject()
            ret.put("files", files)
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "列出上傳檔案失敗")
        }
    }

    private fun prependRootFolderName(files: JSArray, rootName: String) {
        for (i in 0 until files.length()) {
            val item = files.getJSONObject(i)
            val rel = item.optString("relativePath", "").trim()
            item.put("relativePath", if (rel.isEmpty()) rootName else "$rootName/$rel")
        }
    }

    /** 部分模擬器 DocumentFile.listFiles() 回傳空，改用 DocumentsContract 遞迴列檔。 */
    private fun collectUploadFilesViaContract(treeUri: Uri, rootName: String, out: JSArray) {
        val treeDocId = DocumentsContract.getTreeDocumentId(treeUri)
        walkUploadTreeChildren(treeUri, treeDocId, "", rootName, out)
    }

    private fun walkUploadTreeChildren(
        treeUri: Uri,
        parentDocId: String,
        relativePrefix: String,
        rootName: String,
        out: JSArray,
    ) {
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentDocId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
        )
        activity.contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
            val idCol = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameCol = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeCol = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
            val sizeCol = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
            while (cursor.moveToNext()) {
                val docId = cursor.getString(idCol) ?: continue
                val name = cursor.getString(nameCol)?.trim().orEmpty()
                if (name.isEmpty()) continue
                val mime = cursor.getString(mimeCol).orEmpty()
                val rel = if (relativePrefix.isEmpty()) name else "$relativePrefix/$name"
                if (DocumentsContract.Document.MIME_TYPE_DIR == mime) {
                    walkUploadTreeChildren(treeUri, docId, rel, rootName, out)
                    continue
                }
                val docUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, docId)
                val item = JSObject()
                item.put("uri", docUri.toString())
                item.put("relativePath", rel)
                item.put("size", if (sizeCol >= 0) cursor.getLong(sizeCol) else 0L)
                out.put(item)
            }
        }
    }

    private fun collectUploadFiles(dir: DocumentFile, prefix: String, out: JSArray) {
        val children = dir.listFiles() ?: return
        for (child in children) {
            val name = child.name?.trim().orEmpty()
            if (name.isEmpty()) continue
            if (child.isDirectory) {
                val next = if (prefix.isEmpty()) name else "$prefix/$name"
                collectUploadFiles(child, next, out)
                continue
            }
            val rel = if (prefix.isEmpty()) name else "$prefix/$name"
            val item = JSObject()
            item.put("uri", child.uri.toString())
            item.put("relativePath", rel)
            item.put("size", child.length())
            out.put(item)
        }
    }

    @Command
    fun probeTreeWritable(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(TreeArgs::class.java)
            val root = DocumentFile.fromTreeUri(activity, Uri.parse(args.treeUri))
                ?: throw IllegalStateException("無法開啟目標目錄")
            val testName = ".gm-write-test-${UUID.randomUUID()}.tmp"
            val doc = root.createFile("application/octet-stream", testName)
                ?: throw IllegalStateException("無法在目標目錄建立測試檔")
            val ok = try {
                tryOpenForWrite(doc.uri).use { out ->
                    if (out == null) throw IllegalStateException("無法開啟測試檔寫入")
                    out.write(byteArrayOf(0x47, 0x4d))
                    out.flush()
                }
                true
            } finally {
                doc.delete()
            }
            val ret = JSObject()
            ret.put("ok", ok)
            invoke.resolve(ret)
        } catch (ex: Exception) {
            invoke.reject(ex.message ?: "目錄寫入測試失敗")
        }
    }
}

private class ReadArgs {
    lateinit var uri: String
}

private class TreeArgs {
    lateinit var treeUri: String
}

@InvokeArg
class AppendLineArgs {
    var uri: String = ""
    var line: String = ""

    constructor()

    constructor(uri: String, line: String) {
        this.uri = uri
        this.line = line
    }
}

@InvokeArg
class SubdirectoryArgs {
    var treeUri: String = ""
    var subdirectoryName: String = ""
}

@InvokeArg
class CopyToTreeArgs {
    var treeUri: String = ""
    var sourcePath: String = ""
    var relativePath: String = ""

    constructor()

    constructor(treeUri: String, sourcePath: String, relativePath: String) {
        this.treeUri = treeUri
        this.sourcePath = sourcePath
        this.relativePath = relativePath
    }
}
