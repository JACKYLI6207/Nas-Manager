package com.gentleman.manager.android

import android.os.SystemClock
import androidx.media3.exoplayer.analytics.AnalyticsListener
import androidx.media3.exoplayer.source.LoadEventInfo
import androidx.media3.exoplayer.source.MediaLoadData
import java.util.ArrayDeque

/** 依 HTTP 載入位元組計算近 1 秒滾動下載速率（類似檔案傳輸速度）。 */
class StreamSpeedTracker : AnalyticsListener {
    private data class Sample(val timeMs: Long, val bytes: Long)

    private val samples = ArrayDeque<Sample>()
    private var lastBps = 0L

    /** 供 UI 定時輪詢的最新速率。 */
    fun currentBps(): Long {
        val now = SystemClock.elapsedRealtime()
        trimOldSamples(now)
        val rolling = computeRollingBps(now)
        return if (rolling > 0L) rolling else lastBps
    }

    override fun onLoadCompleted(
        eventTime: AnalyticsListener.EventTime,
        loadEventInfo: LoadEventInfo,
        mediaLoadData: MediaLoadData,
    ) {
        val uri = loadEventInfo.uri ?: return
        if (uri.scheme?.startsWith("http") != true) return
        recordBytes(loadEventInfo.bytesLoaded)
    }

    private fun recordBytes(bytes: Long) {
        if (bytes <= 0L) return
        val now = SystemClock.elapsedRealtime()
        samples.addLast(Sample(now, bytes))
        trimOldSamples(now)
        val bps = computeRollingBps(now)
        if (bps > 0L) lastBps = bps
    }

    private fun trimOldSamples(now: Long) {
        while (samples.isNotEmpty() && now - samples.first().timeMs > 1000L) {
            samples.removeFirst()
        }
    }

    private fun computeRollingBps(now: Long): Long {
        if (samples.isEmpty()) return 0L
        val totalBytes = samples.sumOf { it.bytes }
        val spanMs = (now - samples.first().timeMs).coerceAtLeast(200L)
        return totalBytes * 1000L * 8L / spanMs
    }

    fun reset() {
        samples.clear()
        lastBps = 0L
    }
}
