package com.gentleman.manager.android

import android.content.Context
import android.net.Uri
import androidx.core.content.FileProvider
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.nio.ByteBuffer
import java.nio.charset.Charset
import java.nio.charset.CodingErrorAction
import java.security.MessageDigest

/**
 * 外掛字幕常見 UTF-8 / GB18030 / GBK / Big5 等編碼；ExoPlayer 只認 UTF-8。
 * 以嚴格解碼 + 字幕結構評分選最佳編碼，轉成 UTF-8 快取後再載入。
 */
object SubtitleCharsetHelper {
    private const val CACHE_VERSION = 5

    private val CANDIDATE_CHARSETS =
        listOf(
            "GB18030",
            "GBK",
            "Big5",
            "Big5-HKSCS",
            "UTF-8",
            "ISO-8859-1",
            "Windows-1252",
            "UTF-16LE",
            "UTF-16BE",
            "Shift_JIS",
            "EUC-KR",
        )

    private val SRT_TIMING = Regex("""\d{2}:\d{2}:\d{2},\d{3}\s*-->\s*\d{2}:\d{2}:\d{2}""")
    private val ASS_CHARSET = Regex("(?im)^\\s*Charset\\s*=\\s*(.+?)\\s*$")

    fun prepareUtf8SubtitleUri(
        context: Context,
        sourceUri: Uri,
        labelHint: String,
        convertSimpToTrad: Boolean = false,
    ): Uri {
        val cacheDir = File(context.cacheDir, "subs_utf8").apply { mkdirs() }
        val key =
            MessageDigest.getInstance("MD5")
                .digest(
                    "v$CACHE_VERSION|trad=$convertSimpToTrad|${sourceUri}|$labelHint".toByteArray(),
                )
                .joinToString("") { "%02x".format(it) }
        val ext = labelHint.substringAfterLast('.', "srt").lowercase()
        val cacheFile = File(cacheDir, "$key.$ext")
        if (cacheFile.exists() && cacheFile.length() > 0) {
            return toContentUri(context, cacheFile)
        }
        val raw = readAllBytes(context, sourceUri)
        var text = String(convertToUtf8Bytes(raw), Charsets.UTF_8)
        if (convertSimpToTrad) {
            text = convertSimplifiedToTraditional(text)
        }
        cacheFile.writeBytes(wrapUtf8Bom(text))
        return toContentUri(context, cacheFile)
    }

    fun convertToUtf8Bytes(bytes: ByteArray): ByteArray {
        if (bytes.isEmpty()) return bytes

        val bomless = stripBom(bytes)
        if (isStrictUtf8(bomless)) {
            return wrapUtf8Bom(String(bomless, Charsets.UTF_8))
        }

        parseDeclaredCharset(bomless)?.let { declared ->
            decodeForScore(declared, bomless)?.let { (text, score) ->
                if (score > 0) {
                    return wrapUtf8Bom(text)
                }
            }
        }

        var bestText: String? = null
        var bestScore = Int.MIN_VALUE

        for (name in CANDIDATE_CHARSETS) {
            val charset = charsetOrNull(name) ?: continue
            decodeForScore(charset, bomless)?.let { (text, score) ->
                if (score > bestScore) {
                    bestScore = score
                    bestText = text
                }
            }
        }

        bestText?.let { return wrapUtf8Bom(it) }

        // 最後保底：GB18030 涵蓋絕大多數簡繁中文字幕位元組
        return wrapUtf8Bom(String(bomless, Charset.forName("GB18030")))
    }

    private fun toContentUri(context: Context, file: File): Uri =
        FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            file,
        )

    private fun readAllBytes(context: Context, uri: Uri): ByteArray {
        return when (uri.scheme?.lowercase()) {
            "http", "https" -> {
                val conn = (URL(uri.toString()).openConnection() as HttpURLConnection).apply {
                    connectTimeout = 30_000
                    readTimeout = 120_000
                    requestMethod = "GET"
                }
                conn.inputStream.use { it.readBytes() }.also { conn.disconnect() }
            }
            else -> context.contentResolver.openInputStream(uri)!!.use { it.readBytes() }
        }
    }

    private fun stripBom(bytes: ByteArray): ByteArray {
        if (bytes.size >= 3 &&
            bytes[0] == 0xEF.toByte() &&
            bytes[1] == 0xBB.toByte() &&
            bytes[2] == 0xBF.toByte()
        ) {
            return bytes.copyOfRange(3, bytes.size)
        }
        if (bytes.size >= 2) {
            if (bytes[0] == 0xFE.toByte() && bytes[1] == 0xFF.toByte()) {
                return bytes.copyOfRange(2, bytes.size)
            }
            if (bytes[0] == 0xFF.toByte() && bytes[1] == 0xFE.toByte()) {
                return bytes.copyOfRange(2, bytes.size)
            }
        }
        return bytes
    }

    private fun isStrictUtf8(bytes: ByteArray): Boolean {
        if (bytes.isEmpty()) return true
        return runCatching {
            Charsets.UTF_8
                .newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT)
                .decode(ByteBuffer.wrap(bytes))
            true
        }.getOrDefault(false)
    }

    private fun wrapUtf8Bom(text: String): ByteArray {
        val normalized = text.replace("\r\n", "\n").replace('\r', '\n')
        val body = normalized.toByteArray(Charsets.UTF_8)
        return byteArrayOf(0xEF.toByte(), 0xBB.toByte(), 0xBF.toByte()) + body
    }

    private fun parseDeclaredCharset(bytes: ByteArray): Charset? {
        val sampleSize = minOf(bytes.size, 8192)
        val header = String(bytes, 0, sampleSize, Charsets.ISO_8859_1)
        val match = ASS_CHARSET.find(header) ?: return null
        return charsetOrNull(match.groupValues[1].trim())
    }

    private fun charsetOrNull(name: String): Charset? {
        val normalized =
            name.trim().uppercase().replace("_", "-").replace(" ", "")
        val mapped =
            when (normalized) {
                "GB2312", "CP936", "MS936", "936", "WINDOWS-936", "CN-GB" -> "GB18030"
                "CP950", "950" -> "Big5"
                "UTF8" -> "UTF-8"
                "UTF16", "UTF-16" -> "UTF-16LE"
                else -> name.trim()
            }
        return runCatching { Charset.forName(mapped) }.getOrNull()
    }

    private fun decodeForScore(charset: Charset, bytes: ByteArray): Pair<String, Int>? {
        val text =
            runCatching {
                charset
                    .newDecoder()
                    .onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT)
                    .decode(ByteBuffer.wrap(bytes))
                    .toString()
            }.getOrNull() ?: return null
        val score = scoreSubtitleText(text)
        return if (score > Int.MIN_VALUE / 2) text to score else null
    }

    private fun scoreSubtitleText(text: String): Int {
        if (text.isBlank()) return Int.MIN_VALUE / 2

        var score = 0
        var cjk = 0
        var kana = 0
        var replacement = 0
        var controls = 0

        for (ch in text) {
            when {
                ch == '\uFFFD' -> {
                    replacement++
                    score -= 80
                }
                ch == '\u0000' -> {
                    controls++
                    score -= 100
                }
                ch.code in 0x4E00..0x9FFF -> {
                    cjk++
                    score += 4
                }
                ch.code in 0x3400..0x4DBF -> {
                    cjk++
                    score += 4
                }
                ch.code in 0x3040..0x30FF -> {
                    kana++
                    score += 1
                }
                ch.code in 0x3000..0x303F || ch.code in 0xFF00..0xFFEF -> score += 2
                ch.isLetterOrDigit() -> score += 1
                ch == '\n' || ch == '\r' || ch == ' ' || ch == '\t' -> Unit
                ch in ".,!?;:'\"-–—()[]<>/\\" -> score += 1
                ch.code < 0x20 -> {
                    controls++
                    score -= 20
                }
                ch.code in 0x80..0x9F -> {
                    controls++
                    score -= 15
                }
            }
        }

        val timingHits = SRT_TIMING.findAll(text).count()
        score += timingHits * 8

        if (replacement > 0 && replacement > text.length / 64) {
            score -= replacement * 40
        }
        if (controls > text.length / 32) {
            score -= controls * 10
        }

        // GBK 被誤當 UTF-8 常出現少量假日文；真中文字幕通常 CJK >> kana
        if (cjk in 1..<500 && kana > cjk / 2 && timingHits == 0) {
            score -= kana * 3
        }

        if (timingHits == 0 && cjk == 0 && text.length > 80) {
            score -= 50
        }

        return score
    }

    enum class ChineseScript {
        SIMPLIFIED,
        TRADITIONAL,
        UNKNOWN,
    }

    /** 常見簡體字（繁體字幕較少單獨出現） */
    private val SIMPLIFIED_MARKERS = charArrayOf(
        '国', '学', '语', '门', '风', '车', '东', '丝', '让', '认', '这', '说', '广', '电', '龙',
    )

    /** 常見繁體字（簡體字幕較少單獨出現） */
    private val TRADITIONAL_MARKERS = charArrayOf(
        '國', '學', '語', '門', '風', '車', '東', '絲', '讓', '認', '這', '說', '廣', '電', '龍',
    )

    fun detectChineseScript(text: String): ChineseScript {
        if (text.isBlank()) return ChineseScript.UNKNOWN
        var simplifiedHits = 0
        var traditionalHits = 0
        for (ch in text) {
            if (SIMPLIFIED_MARKERS.contains(ch)) simplifiedHits++
            if (TRADITIONAL_MARKERS.contains(ch)) traditionalHits++
        }
        return when {
            simplifiedHits > traditionalHits && simplifiedHits >= 1 -> ChineseScript.SIMPLIFIED
            traditionalHits > simplifiedHits && traditionalHits >= 1 -> ChineseScript.TRADITIONAL
            else -> ChineseScript.UNKNOWN
        }
    }

    fun chineseScriptSuffix(text: String): String {
        return when (detectChineseScript(text)) {
            ChineseScript.SIMPLIFIED -> " (簡體)"
            ChineseScript.TRADITIONAL -> " (繁體)"
            ChineseScript.UNKNOWN -> ""
        }
    }

    fun embeddedSubtitleDisplayName(
        format: androidx.media3.common.Format,
        displayName: String,
        chineseOrdinal: Int,
        chineseTrackCount: Int,
    ): String {
        val base = displayName.trim().ifEmpty { return displayName }
        if (!isChineseSubtitleTrack(format, base)) return base
        if (base.contains("簡體") || base.contains("繁體") || base.contains("简体") || base.contains("繁体")) {
            return base
        }
        resolveScriptFromMetadata(format)?.let { script ->
            return "$base ($script)"
        }
        val script =
            when {
                chineseTrackCount <= 1 -> "繁體"
                chineseTrackCount == 2 ->
                    if (chineseOrdinal == 0) "簡體" else "繁體"
                else ->
                    when (chineseOrdinal) {
                        0 -> "簡體"
                        1 -> "繁體"
                        else -> "繁體"
                    }
            }
        return "$base ($script)"
    }

    fun isChineseSubtitleTrack(
        format: androidx.media3.common.Format,
        displayName: String,
    ): Boolean {
        val lang = format.language?.lowercase().orEmpty()
        if (lang.startsWith("zh") || lang in CHINESE_LANG_CODES) return true
        val blob = "${displayName.lowercase()} ${format.label.orEmpty().lowercase()}"
        return blob.contains("中文") || blob.contains("chinese") || blob.contains("中國")
    }

    fun isChineseSubtitleName(name: String): Boolean {
        val blob = name.lowercase()
        if (blob.contains("中文") || blob.contains("chinese") || blob.contains("mandarin") || blob.contains("cantonese")) {
            return true
        }
        if (blob.contains("简体") || blob.contains("簡體") || blob.contains("繁体") || blob.contains("繁體")) {
            return true
        }
        return blob.contains("zh") || CHINESE_LANG_CODES.any { blob.contains(it) }
    }

    fun localizeVlcTrackName(name: String): String {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return "字幕"
        if (trimmed.any { it.code >= 0x4E00 }) return trimmed
        val lower = trimmed.lowercase()
        return when {
            lower == "disable" -> "關閉字幕"
            lower.contains("chinese") && lower.contains("simp") -> "中文"
            lower.contains("chinese") && lower.contains("trad") -> "中文"
            lower.contains("chinese") -> "中文"
            lower.startsWith("subtitle ") -> "字幕 ${trimmed.substringAfter(" ").trim()}"
            lower.startsWith("track ") -> "字幕 ${trimmed.substringAfter(" ").trim()}"
            lower.startsWith("spu ") -> "字幕 ${trimmed.substringAfter(" ").trim()}"
            else -> trimmed
        }
    }

    fun resolveScriptFromTrackName(name: String): String? {
        val blob = name.lowercase()
        when {
            blob.contains("hans") || blob.contains("simp") || blob.contains("简体") ||
                blob.contains("简中") || blob.contains("gb") || blob.contains("chs") ||
                blob.contains("sc") || blob.contains("zh-cn") || blob.contains("zh_cn") ||
                blob.contains("zh-sg") || blob.contains("simplified") ->
                return "簡體"
            blob.contains("hant") || blob.contains("trad") || blob.contains("繁體") ||
                blob.contains("繁体") || blob.contains("繁中") || blob.contains("big5") ||
                blob.contains("cht") || blob.contains("tc") || blob.contains("zh-tw") ||
                blob.contains("zh_tw") || blob.contains("zh-hk") || blob.contains("taiwan") ||
                blob.contains("traditional") || blob.contains("cantonese") && blob.contains("trad") ->
                return "繁體"
        }
        return when (detectChineseScript(name)) {
            ChineseScript.SIMPLIFIED -> "簡體"
            ChineseScript.TRADITIONAL -> "繁體"
            ChineseScript.UNKNOWN -> null
        }
    }

    fun vlcSpuTrackDisplayName(
        rawName: String,
        trackId: Int,
        chineseTrackIds: List<Int>,
    ): String {
        val localized = localizeVlcTrackName(rawName)
        if (!isChineseSubtitleName("$rawName $localized")) return localized
        if (localized.contains("簡體") || localized.contains("繁體") ||
            localized.contains("简体") || localized.contains("繁体")
        ) {
            return localized
        }
        resolveScriptFromTrackName(rawName)?.let { script ->
            return "$localized ($script)"
        }
        val ord = chineseTrackIds.indexOf(trackId)
        val count = chineseTrackIds.size
        val script =
            when {
                count <= 1 -> "繁體"
                count == 2 -> if (ord == 0) "簡體" else "繁體"
                else ->
                    when (ord) {
                        0 -> "簡體"
                        1 -> "繁體"
                        else -> "繁體"
                    }
            }
        return "$localized ($script)"
    }

    fun resolveScriptFromMetadata(format: androidx.media3.common.Format): String? {
        val blob =
            buildString {
                append(format.label.orEmpty())
                append(' ')
                append(format.id.orEmpty())
                append(' ')
                append(format.language.orEmpty())
                append(' ')
                append(format.codecs.orEmpty())
            }.lowercase()
        when {
            blob.contains("hans") || blob.contains("simp") || blob.contains("简体") ||
                blob.contains("简中") || blob.contains("gb") || blob.contains("chs") ||
                blob.contains("sc") || blob == "zh-cn" || blob.contains("zh-cn") ||
                blob.contains("zh_cn") || blob.contains("zh-sg") ||
                (blob.contains("mandarin") && blob.contains("simp")) ->
                return "簡體"
            blob.contains("hant") || blob.contains("trad") || blob.contains("繁體") ||
                blob.contains("繁体") || blob.contains("繁中") || blob.contains("big5") ||
                blob.contains("cht") || blob.contains("tc") || blob.contains("zh-tw") ||
                blob.contains("zh_tw") || blob.contains("zh-hk") || blob.contains("zh-mo") ||
                (blob.contains("hk") && blob.contains("zh")) || blob.contains("taiwan") ->
                return "繁體"
        }
        format.label?.trim()?.takeIf { it.isNotEmpty() }?.let { raw ->
            val lower = raw.lowercase()
            when {
                lower.contains("simplified") || lower.contains("简体") || lower.contains("简中") -> return "簡體"
                lower.contains("traditional") || lower.contains("繁體") || lower.contains("繁体") || lower.contains("繁中") ->
                    return "繁體"
                else -> Unit
            }
        }
        return null
    }

    private val CHINESE_LANG_CODES = setOf("chi", "zho", "cmn", "yue")

    fun convertSimplifiedToTraditional(text: String): String {
        return runCatching {
            android.icu.text.Transliterator.getInstance("Hans-Hant").transliterate(text)
        }.getOrElse {
            runCatching {
                android.icu.text.Transliterator.getInstance("Simplified-Traditional")
                    .transliterate(text)
            }.getOrDefault(text)
        }
    }
}
