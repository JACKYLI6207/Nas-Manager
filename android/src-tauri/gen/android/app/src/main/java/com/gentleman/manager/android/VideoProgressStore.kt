package com.gentleman.manager.android

import android.content.Context

/** 記錄影片播放進度，供「接續觀看」從中斷點繼續。 */
object VideoProgressStore {
    private const val PREFS = "video_playback_progress"

    fun key(
        pcHost: String?,
        pcPort: Int,
        pcRelPath: String?,
        uri: String?,
    ): String {
        if (!pcRelPath.isNullOrBlank() && !pcHost.isNullOrBlank() && pcPort > 0) {
            return "pc|$pcHost|$pcPort|${pcRelPath.replace('\\', '/')}"
        }
        if (!uri.isNullOrBlank()) return "uri|$uri"
        return "unknown"
    }

    fun save(
        context: Context,
        storageKey: String,
        positionMs: Long,
        durationMs: Long = 0L,
    ) {
        if (storageKey == "unknown" || positionMs < 3000L) return
        if (durationMs > 0L && positionMs >= durationMs - 5000L) {
            clear(context, storageKey)
            return
        }
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putLong("$storageKey|pos", positionMs)
            .putLong("$storageKey|dur", durationMs.coerceAtLeast(0L))
            .apply()
    }

    fun load(context: Context, storageKey: String): Long {
        return loadRecord(context, storageKey).first
    }

    fun loadDuration(context: Context, storageKey: String): Long {
        return loadRecord(context, storageKey).second
    }

    /** @return positionMs to durationMs (both 0 if none / finished) */
    fun loadRecord(context: Context, storageKey: String): Pair<Long, Long> {
        if (storageKey == "unknown") return 0L to 0L
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val pos = prefs.getLong("$storageKey|pos", 0L)
        val dur = prefs.getLong("$storageKey|dur", 0L)
        if (pos <= 0L) return 0L to dur.coerceAtLeast(0L)
        if (dur > 0L && pos >= dur - 3000L) return 0L to 0L
        return pos to dur.coerceAtLeast(0L)
    }

    fun clear(context: Context, storageKey: String) {
        if (storageKey == "unknown") return
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .remove("$storageKey|pos")
            .remove("$storageKey|dur")
            .apply()
    }
}
