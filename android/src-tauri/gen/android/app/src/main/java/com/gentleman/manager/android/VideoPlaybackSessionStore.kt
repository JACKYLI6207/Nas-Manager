package com.gentleman.manager.android

import android.content.Context
import org.json.JSONArray

/** 使用者選擇「背景繼續播放」時，供 App 內「接續觀看」使用。 */
object VideoPlaybackSessionStore {
    private const val PREFS = "video_playback_session"
    private const val KEY_ACTIVE = "active"
    private const val KEY_URI = "uri"
    private const val KEY_TITLE = "title"
    private const val KEY_PC_HOST = "pcHost"
    private const val KEY_PC_PORT = "pcPort"
    private const val KEY_PC_REL_PATH = "pcRelPath"
    private const val KEY_SUBTITLE_URIS = "subtitleUris"

    fun save(
        context: Context,
        uri: String,
        title: String,
        subtitleUris: List<String>,
        pcHost: String?,
        pcPort: Int,
        pcRelPath: String?,
    ) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_ACTIVE, true)
            .putString(KEY_URI, uri)
            .putString(KEY_TITLE, title)
            .putString(KEY_PC_HOST, pcHost)
            .putInt(KEY_PC_PORT, pcPort)
            .putString(KEY_PC_REL_PATH, pcRelPath)
            .putString(KEY_SUBTITLE_URIS, JSONArray(subtitleUris).toString())
            .commit()
    }

    fun clear(context: Context) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .clear()
            .commit()
    }

    fun isActive(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_ACTIVE, false)

    fun toJson(context: Context): String? {
        if (!isActive(context)) return null
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val uri = prefs.getString(KEY_URI, null)?.trim().orEmpty()
        if (uri.isEmpty()) return null
        val subtitleUris =
            runCatching {
                val arr = JSONArray(prefs.getString(KEY_SUBTITLE_URIS, "[]") ?: "[]")
                buildList {
                    for (i in 0 until arr.length()) {
                        val item = arr.optString(i, "").trim()
                        if (item.isNotEmpty()) add(item)
                    }
                }
            }.getOrDefault(emptyList())
        return org.json.JSONObject()
            .put("uri", uri)
            .put("title", prefs.getString(KEY_TITLE, "").orEmpty())
            .put("pcHost", prefs.getString(KEY_PC_HOST, "").orEmpty())
            .put("pcPort", prefs.getInt(KEY_PC_PORT, 0))
            .put("pcRelPath", prefs.getString(KEY_PC_REL_PATH, "").orEmpty())
            .put("subtitleUris", JSONArray(subtitleUris))
            .toString()
    }
}
