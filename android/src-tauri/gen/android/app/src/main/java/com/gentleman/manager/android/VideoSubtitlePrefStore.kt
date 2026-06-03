package com.gentleman.manager.android

import android.content.Context
import org.json.JSONArray

/** 字幕偏好：全域尺寸 + 每部影片的外掛字幕與內嵌軌選擇。 */
object VideoSubtitlePrefStore {
    private const val PREFS = "video_subtitle_prefs"
    private const val KEY_GLOBAL_TEXT_SIZE = "global_text_size_sp"
    private const val KEY_SIMP_TO_TRAD = "global_simp_to_trad"
    private const val KEY_SUBTITLE_OFFSET_X = "global_subtitle_offset_x"
    private const val KEY_SUBTITLE_OFFSET_Y = "global_subtitle_offset_y"
    private const val KEY_PGS_SCALE = "global_pgs_scale"
    private const val DEFAULT_TEXT_SIZE_SP = 16f
    private const val DEFAULT_PGS_SCALE = 1f

    data class TextTrackSelection(
        val disabled: Boolean,
        val groupId: String?,
        val trackIndex: Int,
    )

    fun loadTextSizeSp(context: Context): Float {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        return prefs.getFloat(KEY_GLOBAL_TEXT_SIZE, DEFAULT_TEXT_SIZE_SP)
            .coerceIn(8f, 48f)
    }

    fun saveTextSizeSp(context: Context, sizeSp: Float) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putFloat(KEY_GLOBAL_TEXT_SIZE, sizeSp.coerceIn(8f, 48f))
            .apply()
    }

    fun loadSimpToTrad(context: Context): Boolean =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(KEY_SIMP_TO_TRAD, false)

    fun saveSimpToTrad(context: Context, enabled: Boolean) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_SIMP_TO_TRAD, enabled)
            .apply()
    }

    fun loadSubtitleOffset(context: Context): Pair<Float, Float> {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        return prefs.getFloat(KEY_SUBTITLE_OFFSET_X, 0f) to prefs.getFloat(KEY_SUBTITLE_OFFSET_Y, 0f)
    }

    fun saveSubtitleOffset(context: Context, offsetX: Float, offsetY: Float) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putFloat(KEY_SUBTITLE_OFFSET_X, offsetX)
            .putFloat(KEY_SUBTITLE_OFFSET_Y, offsetY)
            .apply()
    }

    fun loadPgsScale(context: Context): Float {
        return context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getFloat(KEY_PGS_SCALE, DEFAULT_PGS_SCALE)
            .coerceIn(0.5f, 2f)
    }

    fun savePgsScale(context: Context, scale: Float) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putFloat(KEY_PGS_SCALE, scale.coerceIn(0.5f, 2f))
            .apply()
    }

    fun loadExternalUris(context: Context, videoKey: String): List<String> {
        if (videoKey == "unknown") return emptyList()
        val raw =
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .getString(externalKey(videoKey), null)
                ?: return emptyList()
        return runCatching {
            val arr = JSONArray(raw)
            buildList {
                for (i in 0 until arr.length()) {
                    val item = arr.optString(i, "").trim()
                    if (item.isNotEmpty()) add(item)
                }
            }
        }.getOrDefault(emptyList())
    }

    fun saveExternalUris(context: Context, videoKey: String, uris: List<String>) {
        if (videoKey == "unknown") return
        val cleaned = uris.map { it.trim() }.filter { it.isNotEmpty() }.distinct()
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putString(externalKey(videoKey), JSONArray(cleaned).toString())
            .apply()
    }

    fun loadTextTrack(context: Context, videoKey: String): TextTrackSelection? {
        if (videoKey == "unknown") return null
        val raw =
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .getString(textTrackKey(videoKey), null)
                ?: return null
        val parts = raw.split("|", limit = 3)
        if (parts.size < 2) return null
        return when (parts[0]) {
            "off" -> TextTrackSelection(disabled = true, groupId = null, trackIndex = -1)
            "on" -> {
                if (parts.size < 3) return null
                TextTrackSelection(
                    disabled = false,
                    groupId = parts[1].ifBlank { null },
                    trackIndex = parts[2].toIntOrNull() ?: return null,
                )
            }
            else -> null
        }
    }

    fun saveTextTrack(context: Context, videoKey: String, selection: TextTrackSelection?) {
        if (videoKey == "unknown") return
        val editor = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
        if (selection == null) {
            editor.remove(textTrackKey(videoKey))
        } else if (selection.disabled) {
            editor.putString(textTrackKey(videoKey), "off")
        } else {
            val groupId = selection.groupId ?: return
            editor.putString(
                textTrackKey(videoKey),
                "on|$groupId|${selection.trackIndex}",
            )
        }
        editor.apply()
    }

    private fun externalKey(videoKey: String) = "ext|$videoKey"

    private fun textTrackKey(videoKey: String) = "text|$videoKey"
}
