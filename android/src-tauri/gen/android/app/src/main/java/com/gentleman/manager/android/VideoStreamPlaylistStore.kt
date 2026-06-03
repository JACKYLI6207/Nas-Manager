package com.gentleman.manager.android

import org.json.JSONArray
import org.json.JSONObject

/** 遠端串流播放清單（由前端 sync，供播放器內切換曲目）。 */
object VideoStreamPlaylistStore {
    data class Item(
        val host: String,
        val port: Int,
        val relPath: String,
        val title: String,
    )

    @Volatile
    private var items: List<Item> = emptyList()

    @Volatile
    private var currentRelPath: String? = null

    private fun normPath(path: String): String =
        path.trim().replace('\\', '/').trimEnd('/')

    fun pathsEqual(a: String, b: String): Boolean {
        val na = normPath(a)
        val nb = normPath(b)
        if (na.equals(nb, ignoreCase = true)) return true
        val fa = na.substringAfterLast('/')
        val fb = nb.substringAfterLast('/')
        return fa.isNotEmpty() && fa.equals(fb, ignoreCase = true)
    }

    fun setFromJson(playlistJson: String?, currentRelPath: String?) {
        if (playlistJson.isNullOrBlank()) {
            clear()
            return
        }
        val parsed = mutableListOf<Item>()
        runCatching {
            val arr = JSONArray(playlistJson)
            for (i in 0 until arr.length()) {
                val o = arr.optJSONObject(i) ?: continue
                val host = o.optString("host", "").trim()
                val port = o.optInt("port", 0)
                val relPath = o.optString("relPath", "").trim()
                val title = o.optString("title", "").trim().ifBlank { relPath.substringAfterLast('/') }
                if (host.isNotEmpty() && port > 0 && relPath.isNotEmpty()) {
                    parsed.add(Item(host, port, relPath, title))
                }
            }
        }
        items = parsed
        val hinted = currentRelPath?.trim()?.takeIf { it.isNotEmpty() }
        this.currentRelPath = hinted
        alignCurrentToPath(hinted)
    }

    fun setCurrentRelPath(relPath: String?) {
        currentRelPath = relPath?.trim()?.takeIf { it.isNotEmpty() }
        alignCurrentToPath(currentRelPath)
    }

    /** 若 current 與列表不一致，依路徑（含檔名）對齊索引。 */
    fun alignCurrentToPath(path: String?) {
        val hint = path?.trim()?.takeIf { it.isNotEmpty() } ?: return
        if (items.isEmpty()) return
        if (currentIndex() >= 0) return
        val idx = items.indexOfFirst { pathsEqual(it.relPath, hint) }
        if (idx >= 0) {
            currentRelPath = items[idx].relPath
        }
    }

    fun clear() {
        items = emptyList()
        currentRelPath = null
    }

    fun hasPlaylist(): Boolean = items.size > 1

    /** 已同步播放列表（含單曲串流）；用於上一部／下一部與壓灰判定。 */
    fun isPlaylistMode(): Boolean = items.isNotEmpty()

    fun getItems(): List<Item> = items

    fun getCurrentRelPath(): String? = currentRelPath

    fun currentIndex(): Int {
        val cur = currentRelPath?.trim()?.takeIf { it.isNotEmpty() } ?: return -1
        return items.indexOfFirst { pathsEqual(it.relPath, cur) }
    }

    fun hasPrevious(): Boolean {
        val idx = currentIndex()
        return idx > 0
    }

    fun hasNext(): Boolean {
        val idx = currentIndex()
        return idx >= 0 && idx < items.size - 1
    }

    fun getPrevious(): Item? {
        val idx = currentIndex()
        if (idx <= 0) return null
        return items[idx - 1]
    }

    fun getNext(): Item? {
        val idx = currentIndex()
        if (idx < 0 || idx >= items.size - 1) return null
        return items[idx + 1]
    }

    fun toJsonArray(): JSONArray {
        val arr = JSONArray()
        for (item in items) {
            arr.put(
                JSONObject()
                    .put("host", item.host)
                    .put("port", item.port)
                    .put("relPath", item.relPath)
                    .put("title", item.title),
            )
        }
        return arr
    }
}
