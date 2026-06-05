package com.gentleman.manager.android

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.view.SurfaceView
import android.view.TextureView
import android.view.View
import android.view.ViewGroup
import org.videolan.libvlc.LibVLC
import org.videolan.libvlc.Media
import org.videolan.libvlc.MediaPlayer
import org.videolan.libvlc.interfaces.IMedia
import org.videolan.libvlc.util.VLCVideoLayout
import kotlin.math.roundToInt

/**
 * LibVLC 播放（RMVB / RealMedia 等 ExoPlayer 無法解析的封裝格式）。
 */
class NasVlcPlayerSession(
    context: Context,
    private val videoLayout: VLCVideoLayout,
    private val onEnded: () -> Unit,
    private val onError: (String) -> Unit,
) {
    data class TrackOption(
        val id: Int,
        val name: String,
    )

    private val mainHandler = Handler(Looper.getMainLooper())
    private val libVlc: LibVLC
    private val mediaPlayer: MediaPlayer
    private var viewsAttached = false
    private var playbackSpeed = 1f
    private var userBrightnessBias = 0f
    private var subtitleTextScaleSp = DEFAULT_SUBTITLE_TEXT_SCALE_SP
    private var simpToTradEnabled = false
    private var pendingSeekMs = 0L
    private var seekAttempts = 0
    private var suppressVideoDuringSeek = false
    private var lastUri: Uri? = null
    private var lastSubtitleUris: List<String> = emptyList()
    private var pendingSpuTrackId: Int? = null

    private val seekRetryRunnable =
        Runnable {
            tryApplyPendingSeek()
        }

    init {
        val options =
            arrayListOf(
                "--network-caching=3000",
                "--file-caching=1500",
                "--live-caching=1500",
                "--http-reconnect",
                "--sub-autodetect-file",
                "--vout=android_display,none",
                "--freetype-rel-fontsize=${mapSubtitleSpToFreetype(subtitleTextScaleSp)}",
                "--freetype-bold",
                "--freetype-outline-thickness=4",
                "--freetype-shadow-opacity=128",
                "--stats",
            )
        libVlc = LibVLC(context.applicationContext, options)
        mediaPlayer = MediaPlayer(libVlc)
        mediaPlayer.setEventListener { event ->
            when (event.type) {
                MediaPlayer.Event.EndReached ->
                    mainHandler.post { onEnded() }
                MediaPlayer.Event.EncounteredError ->
                    mainHandler.post { onError("VLC_ENCOUNTERED_ERROR") }
                MediaPlayer.Event.Playing,
                MediaPlayer.Event.Vout,
                MediaPlayer.Event.ESAdded,
                MediaPlayer.Event.MediaChanged,
                -> {
                    mainHandler.post {
                        refreshVideoSurface()
                        tryApplyPendingSeek()
                        tryApplyPendingSpuTrack()
                    }
                }
                MediaPlayer.Event.TimeChanged ->
                    mainHandler.post { tryApplyPendingSeek() }
            }
        }
    }

    fun play(
        uri: Uri,
        startMs: Long,
        subtitleUris: List<String> = emptyList(),
        hideVideoUntilSeeked: Boolean = false,
    ) {
        lastUri = uri
        lastSubtitleUris = subtitleUris
        pendingSeekMs = startMs.coerceAtLeast(0L)
        seekAttempts = 0
        runCatching { mediaPlayer.stop() }
        suppressVideoDuringSeek = hideVideoUntilSeeked && pendingSeekMs > 0L
        if (suppressVideoDuringSeek) {
            videoLayout.alpha = 0f
        }
        ensureViewsAttached()
        refreshVideoSurface()
        val media = Media(libVlc, uri)
        media.setHWDecoderEnabled(true, true)
        applyVideoAdjustMediaOptions(media)
        applySubtitleMediaOptions(media)
        subtitleUris.filter { it.isNotBlank() }.forEachIndexed { index, uriString ->
            media.addSlave(
                IMedia.Slave(
                    IMedia.Slave.Type.Subtitle,
                    index + 1,
                    uriString,
                ),
            )
        }
        media.parseAsync()
        mediaPlayer.media = media
        media.release()
        mediaPlayer.rate = playbackSpeed
        mediaPlayer.play()
        scheduleSeekRetries()
    }

    fun isPlaying(): Boolean = mediaPlayer.isPlaying

    fun currentPositionMs(): Long {
        val t = mediaPlayer.time
        return if (t > 0L) t else pendingSeekMs.coerceAtLeast(0L)
    }

    fun durationMs(): Long = mediaPlayer.length.coerceAtLeast(0L)

    fun playbackSpeed(): Float = playbackSpeed

    fun setPlaybackSpeed(speed: Float) {
        playbackSpeed = speed.coerceIn(0.25f, 4f)
        mediaPlayer.rate = playbackSpeed
    }

    fun setUserBrightnessBias(bias: Float) {
        userBrightnessBias = bias.coerceIn(-1f, 1f)
    }

    fun setSubtitleTextScaleSp(sizeSp: Float) {
        subtitleTextScaleSp = sizeSp.coerceIn(MIN_SUBTITLE_TEXT_SCALE_SP, MAX_SUBTITLE_TEXT_SCALE_SP)
    }

    fun setSimpToTradEnabled(enabled: Boolean) {
        simpToTradEnabled = enabled
    }

    fun hasActiveSubtitles(): Boolean = mediaPlayer.spuTrack >= 0

    fun findSubtitleSurface(): View? {
        val surfaces = mutableListOf<View>()
        collectSurfaceViews(videoLayout, surfaces)
        return if (surfaces.size >= 2) surfaces.last() else null
    }

    fun applySubtitleSurfaceOffset(offsetX: Float, offsetY: Float) {
        videoLayout.post {
            findSubtitleSurface()?.apply {
                scaleX = 1f
                scaleY = 1f
                translationX = offsetX
                translationY = offsetY
            }
        }
    }

    /** 簡轉繁／外掛字幕／字幕尺寸變更時重載（隱藏畫面避免閃爍）。 */
    fun reapplySubtitleOptionsAtCurrentPosition() {
        reloadAtCurrentPosition(preserveSpuTrack = true)
    }

    /** 亮度／字幕選項變更後，於目前位置重載。 */
    fun reapplyVideoAdjustAtCurrentPosition() {
        reloadAtCurrentPosition(preserveSpuTrack = true)
    }

    private fun reloadAtCurrentPosition(preserveSpuTrack: Boolean) {
        val uri = lastUri ?: return
        val pos = mediaPlayer.time.coerceAtLeast(0L)
        val spu = if (preserveSpuTrack) mediaPlayer.spuTrack else -1
        val playing = isPlaying()
        play(uri, pos, lastSubtitleUris, hideVideoUntilSeeked = pos > 0L)
        if (spu >= 0) {
            pendingSpuTrackId = spu
        }
        if (!playing) {
            pause()
        }
    }

    fun addExternalSubtitle(uriString: String): Boolean {
        if (uriString.isBlank()) return false
        val ok =
            runCatching {
                mediaPlayer.addSlave(IMedia.Slave.Type.Subtitle, Uri.parse(uriString), true)
            }.getOrDefault(false)
        if (ok) {
            lastSubtitleUris = lastSubtitleUris + uriString
        }
        return ok
    }

    fun seekTo(positionMs: Long) {
        pendingSeekMs = 0L
        val duration = mediaPlayer.length.coerceAtLeast(0L)
        val target =
            if (duration > 0L) {
                positionMs.coerceIn(0L, duration)
            } else {
                positionMs.coerceAtLeast(0L)
            }
        mediaPlayer.time = target
    }

    fun pause() {
        if (mediaPlayer.isPlaying) {
            mediaPlayer.pause()
        }
    }

    fun resume() {
        mediaPlayer.play()
    }

    fun audioTracks(): List<TrackOption> =
        mediaPlayer.audioTracks?.map { TrackOption(it.id, it.name) }.orEmpty()

    fun currentAudioTrack(): Int = mediaPlayer.audioTrack

    fun setAudioTrack(trackId: Int): Boolean = mediaPlayer.setAudioTrack(trackId)

    fun spuTracks(): List<TrackOption> =
        mediaPlayer.spuTracks
            ?.filter { it.id >= 0 && !it.name.equals("Disable", ignoreCase = true) }
            ?.map { TrackOption(it.id, it.name) }
            .orEmpty()

    fun currentSpuTrack(): Int = mediaPlayer.spuTrack

    fun setSpuTrack(trackId: Int): Boolean {
        pendingSpuTrackId = if (trackId < 0) null else trackId
        return applySpuTrackSelection(trackId)
    }

    fun disableSubtitles(): Boolean = setSpuTrack(-1)

    fun release() {
        mainHandler.removeCallbacks(seekRetryRunnable)
        videoLayout.alpha = 1f
        runCatching {
            mediaPlayer.stop()
            if (viewsAttached) {
                mediaPlayer.detachViews()
                viewsAttached = false
            }
            mediaPlayer.release()
            libVlc.release()
        }
    }

    private fun ensureViewsAttached() {
        if (viewsAttached) return
        mediaPlayer.attachViews(videoLayout, null, true, false)
        viewsAttached = true
    }

    private fun applySpuTrackSelection(trackId: Int): Boolean {
        val ok = mediaPlayer.setSpuTrack(trackId)
        if (ok) {
            pendingSpuTrackId = null
            refreshVideoSurface()
            runCatching { mediaPlayer.updateVideoSurfaces() }
        }
        return ok
    }

    private fun tryApplyPendingSpuTrack() {
        val trackId = pendingSpuTrackId ?: return
        if (spuTracks().none { it.id == trackId }) return
        applySpuTrackSelection(trackId)
    }

    private fun refreshVideoSurface() {
        if (!viewsAttached) return
        val w = videoLayout.width.coerceAtLeast(1)
        val h = videoLayout.height.coerceAtLeast(1)
        runCatching {
            mediaPlayer.vlcVout.setWindowSize(w, h)
        }
    }

    private fun scheduleSeekRetries() {
        mainHandler.removeCallbacks(seekRetryRunnable)
        if (pendingSeekMs <= 0L) return
        mainHandler.postDelayed(seekRetryRunnable, 200)
        mainHandler.postDelayed(seekRetryRunnable, 600)
        mainHandler.postDelayed(seekRetryRunnable, 1200)
        mainHandler.postDelayed(seekRetryRunnable, 2500)
        mainHandler.postDelayed(seekRetryRunnable, 4500)
    }

    private fun tryApplyPendingSeek() {
        if (pendingSeekMs <= 0L) {
            revealVideoIfHidden()
            return
        }
        seekAttempts++
        val duration = mediaPlayer.length.coerceAtLeast(0L)
        val target =
            if (duration > 0L) {
                pendingSeekMs.coerceAtMost(duration)
            } else {
                pendingSeekMs
            }
        mediaPlayer.time = target
        val actual = mediaPlayer.time.coerceAtLeast(0L)
        val reached =
            actual >= target - 1500L ||
                (duration > 0L && actual >= duration - 5000L) ||
                seekAttempts >= 30
        if (reached) {
            pendingSeekMs = 0L
            mainHandler.removeCallbacks(seekRetryRunnable)
            revealVideoIfHidden()
        } else if (seekAttempts < 30) {
            mainHandler.postDelayed(seekRetryRunnable, 350)
        }
    }

    private fun revealVideoIfHidden() {
        if (!suppressVideoDuringSeek) return
        suppressVideoDuringSeek = false
        videoLayout.alpha = 1f
    }

    private fun collectSurfaceViews(root: ViewGroup, out: MutableList<View>) {
        for (i in 0 until root.childCount) {
            when (val child = root.getChildAt(i)) {
                is SurfaceView, is TextureView -> out.add(child)
                is ViewGroup -> collectSurfaceViews(child, out)
            }
        }
    }

    private fun applyVideoAdjustMediaOptions(media: Media) {
        val userGain = 1f + userBrightnessBias * 0.55f
        if (userBrightnessBias != 0f) {
            media.addOption(":video-filter=adjust")
            media.addOption(":brightness=${userGain.coerceIn(0.5f, 2f)}")
        }
    }

    private fun applySubtitleMediaOptions(media: Media) {
        val relSize = mapSubtitleSpToFreetype(subtitleTextScaleSp)
        media.addOption(":freetype-rel-fontsize=$relSize")
        media.addOption(":freetype-bold")
        media.addOption(":subsdec-encoding=utf8")
        if (simpToTradEnabled) {
            media.addOption(":subsdec-formatted=1")
        }
    }

    companion object {
        const val DEFAULT_SUBTITLE_TEXT_SCALE_SP = 16f
        const val MIN_SUBTITLE_TEXT_SCALE_SP = 8f
        const val MAX_SUBTITLE_TEXT_SCALE_SP = 32f

        fun mapSubtitleSpToFreetype(sizeSp: Float): Int {
            val normalized =
                ((sizeSp - MIN_SUBTITLE_TEXT_SCALE_SP) /
                    (MAX_SUBTITLE_TEXT_SCALE_SP - MIN_SUBTITLE_TEXT_SCALE_SP))
                    .coerceIn(0f, 1f)
            return (8 + normalized * 28).roundToInt()
        }
    }
}
