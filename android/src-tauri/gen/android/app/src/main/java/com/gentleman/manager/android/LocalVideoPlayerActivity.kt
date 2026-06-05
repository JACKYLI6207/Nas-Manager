package com.gentleman.manager.android

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Base64
import android.util.TypedValue
import android.view.SurfaceView
import android.view.TextureView
import android.view.View
import android.view.ViewGroup
import android.view.ViewTreeObserver
import android.view.WindowManager
import android.widget.Button
import android.widget.CheckBox
import android.widget.HorizontalScrollView
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.TextView
import android.widget.Toast
import android.widget.FrameLayout
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AlertDialog
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.media3.common.C
import androidx.media3.common.Effect
import androidx.media3.common.Format
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.PlaybackException
import androidx.media3.common.ForwardingPlayer
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionOverride
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.Tracks
import androidx.media3.common.VideoSize
import androidx.media3.common.text.CueGroup
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.decoder.ffmpeg.FfmpegLibrary
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.trackselection.DefaultTrackSelector
import androidx.media3.exoplayer.upstream.DefaultBandwidthMeter
import androidx.media3.extractor.DefaultExtractorsFactory
import androidx.media3.ui.DefaultTrackNameProvider
import androidx.media3.ui.PlayerControlView
import androidx.media3.ui.PlayerView
import androidx.media3.ui.SubtitleView
import androidx.media3.ui.R as Media3UiR
import androidx.media3.ui.TrackSelectionDialogBuilder
import org.videolan.libvlc.util.VLCVideoLayout
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors
import java.lang.ref.WeakReference
import kotlin.math.roundToInt

/**
 * 本地／網路／遠端串流播放（ExoPlayer / Media3 + FFmpeg 軟解）。
 */
@UnstableApi
class LocalVideoPlayerActivity : AppCompatActivity() {

    private var player: ExoPlayer? = null
    private var playlistPlayer: Player? = null
    private var playbackUri: Uri? = null
    private var titleHint: String = ""
    private var bandwidthMeter: DefaultBandwidthMeter? = null
    private var playbackError: String? = null
    private val extraSubtitleUris = mutableListOf<String>()
    private val ioExecutor = Executors.newSingleThreadExecutor()

    private var pcHost: String? = null
    private var pcPort: Int = 0
    private var pcRelPath: String? = null
    private var hasRemotePc = false
    private var subtitleBusy = false
    private var topChromeScrollRef: HorizontalScrollView? = null
    private var statusBarInsetTop = 0
    private var uiChromeVisible = false
    private var playerViewRef: PlayerView? = null
    private var controlsRootRef: View? = null
    private var vlcControlRef: PlayerControlView? = null
    private var vlcTapCatcherRef: View? = null
    private val videoBrightnessMatrix = VideoBrightnessMatrix()
    private var pinchZoomHelper: VideoPinchZoomHelper? = null
    private var subtitleDragHelper: SubtitleDragHelper? = null
    private var vlcSubtitleDragHelper: VlcSubtitleDragHelper? = null
    private var isRemoteStreamPlayback = false
    private var simpToTradEnabled = false
    private val progressSaveHandler = Handler(Looper.getMainLooper())
    private var progressStorageKey: String = "unknown"
    private var subtitleTextSizeSp = DEFAULT_SUBTITLE_TEXT_SIZE_SP
    private var subtitlePgsScale = DEFAULT_PGS_SCALE
    private var embeddedSubtitleIsPgs = false
    private var contentFrameRef: ViewGroup? = null
    private var contentFrameLayoutListener: View.OnLayoutChangeListener? = null
    private var displaySubtitleView: SubtitleView? = null
    private var lastVideoSize: VideoSize = VideoSize.UNKNOWN
    private var trackSelectorRef: DefaultTrackSelector? = null
    private var subtitleSelectionRestored = false
    private var leaveDialogShowing = false
    private var embeddedCueConverter: Player.Listener? = null
    private var streamPlaylistPanel: ScrollView? = null
    private var streamPlaylistListHost: LinearLayout? = null
    private var streamPlaylistOpen = false
    private var vlcSession: NasVlcPlayerSession? = null
    private var vlcBindingPlayer: VlcBindingPlayer? = null
    private var vlcVideoLayoutRef: VLCVideoLayout? = null

    private val pickLocalSubtitleLauncher =
        registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            uri?.let { appendLocalSubtitle(it) }
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        activeInstance = WeakReference(this)
        setupWindowForVideo()
        setContentView(R.layout.activity_local_video_player)

        val videoUri = resolvePlaybackUri()
        if (videoUri == null) {
            Toast.makeText(this, "無法讀取影片路徑", Toast.LENGTH_SHORT).show()
            finishWithResult(null)
            return
        }
        playbackUri = videoUri
        titleHint = intent.getStringExtra(EXTRA_TITLE).orEmpty()

        pcHost = intent.getStringExtra(EXTRA_PC_HOST)?.trim()?.takeIf { it.isNotEmpty() }
        pcPort = intent.getIntExtra(EXTRA_PC_PORT, 0)
        pcRelPath = intent.getStringExtra(EXTRA_PC_REL_PATH)?.trim()?.takeIf { it.isNotEmpty() }
        progressStorageKey =
            VideoProgressStore.key(pcHost, pcPort, pcRelPath, videoUri.toString())

        subtitleTextSizeSp = VideoSubtitlePrefStore.loadTextSizeSp(this)
        subtitlePgsScale = VideoSubtitlePrefStore.loadPgsScale(this)
        simpToTradEnabled = VideoSubtitlePrefStore.loadSimpToTrad(this)
        intent.getStringArrayExtra(EXTRA_SUBTITLE_URIS)?.filter { it.isNotBlank() }?.let {
            extraSubtitleUris.addAll(it)
        }
        VideoSubtitlePrefStore.loadExternalUris(this, progressStorageKey).forEach { uri ->
            if (!extraSubtitleUris.contains(uri)) {
                extraSubtitleUris.add(uri)
            }
        }

        if (!FfmpegLibrary.isAvailable()) {
            Toast.makeText(this, "FFmpeg 解碼庫未載入，x265/部分 MKV 可能無法播放", Toast.LENGTH_LONG).show()
        }

        val playerView = findViewById<PlayerView>(R.id.player_view)
        playerViewRef = playerView
        val topChromeScroll = findViewById<HorizontalScrollView>(R.id.top_chrome_scroll)
        topChromeScrollRef = topChromeScroll
        val btnExit = findViewById<Button>(R.id.btn_exit)
        setupWindowInsets()

        val isRemoteStream = videoUri.scheme?.startsWith("http") == true
        isRemoteStreamPlayback = isRemoteStream
        hasRemotePc = !pcHost.isNullOrEmpty() && pcPort > 0 && !pcRelPath.isNullOrEmpty()
        scaleTopChrome()

        btnExit.setOnClickListener { showLeavePlaybackDialog() }
        onBackPressedDispatcher.addCallback(
            this,
            object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() {
                    showLeavePlaybackDialog()
                }
            },
        )

        if (shouldUseVlcEngine(videoUri, titleHint)) {
            setupVlcPlayback(videoUri, savedInstanceState)
            return
        }

        findViewById<VLCVideoLayout>(R.id.vlc_video_layout).visibility = View.GONE
        findViewById<PlayerControlView>(R.id.vlc_player_control).apply {
            visibility = View.GONE
            player = null
        }
        vlcControlRef = null
        playerView.visibility = View.VISIBLE
        playerViewRef = playerView
        controlsRootRef = playerView

        playerView.setControllerVisibilityListener(
            PlayerView.ControllerVisibilityListener { visibility ->
                uiChromeVisible = visibility == View.VISIBLE
                topChromeScroll.visibility = visibility
                applyImmersiveMode(!uiChromeVisible)
                updateTopChromePadding()
                if (visibility == View.VISIBLE) {
                    playerView.post {
                        alignPlaylistCurrentFromPlayback()
                        ensureSubtitleButtonInteractive(playerView)
                    }
                }
            },
        )

        val meter = DefaultBandwidthMeter.Builder(this).build()
        bandwidthMeter = meter

        val trackSelector = DefaultTrackSelector(this)
        trackSelectorRef = trackSelector
        trackSelector.parameters =
            trackSelector
                .buildUponParameters()
                .setPreferredTextLanguage("zh")
                .setSelectUndeterminedTextLanguage(true)
                .setAllowInvalidateSelectionsOnRendererCapabilitiesChange(true)
                .build()

        val httpFactory =
            DefaultHttpDataSource.Factory()
                .setAllowCrossProtocolRedirects(true)
                .setConnectTimeoutMs(30_000)
                .setReadTimeoutMs(120_000)
        val dataSourceFactory = DefaultDataSource.Factory(this, httpFactory)
        val extractorsFactory = DefaultExtractorsFactory()
        val mediaSourceFactory = DefaultMediaSourceFactory(dataSourceFactory, extractorsFactory)

        val renderersFactory = NasExoRenderersFactory(this)

        val exo =
            ExoPlayer.Builder(this)
                .setRenderersFactory(renderersFactory)
                .setTrackSelector(trackSelector)
                .setBandwidthMeter(meter)
                .setMediaSourceFactory(mediaSourceFactory)
                .build()
                .also { player = it }
        exo.setWakeMode(C.WAKE_MODE_NETWORK)
        refreshVideoEffects()
        val wrapped = PlaylistNavPlayer(exo)
        playlistPlayer = wrapped
        playerView.player = wrapped
        playerView.setControllerShowTimeoutMs(0)
        setupPlayerViewInteractions(playerView)
        setupEmbeddedCueConverter(exo, playerView)

        exo.addListener(
            object : Player.Listener {
                override fun onPlaybackStateChanged(playbackState: Int) {
                    if (playbackState == Player.STATE_READY) {
                        playerView.post { alignPlaylistCurrentFromPlayback() }
                    }
                    if (playbackState == Player.STATE_ENDED) {
                        VideoProgressStore.clear(this@LocalVideoPlayerActivity, progressStorageKey)
                        finishWithResult(null)
                    }
                }

                override fun onPlayerError(error: PlaybackException) {
                    playbackError = error.errorCodeName
                    handlePlaybackError(videoUri, error)
                }

                override fun onTracksChanged(tracks: Tracks) {
                    playerView.post {
                        ensureSubtitleButtonInteractive(playerView)
                        restoreSavedTextTrackSelection(tracks)
                        updateEmbeddedSubtitleLayout()
                    }
                }

                override fun onTrackSelectionParametersChanged(parameters: TrackSelectionParameters) {
                    persistTextTrackSelection(parameters)
                    playerView.post { updateEmbeddedSubtitleLayout() }
                }

                override fun onVideoSizeChanged(videoSize: VideoSize) {
                    lastVideoSize = videoSize
                    playerView.post {
                        updateEmbeddedSubtitleLayout()
                        refreshSubtitleCueDisplay()
                    }
                }

                override fun onEvents(
                    player: Player,
                    events: Player.Events,
                ) {
                    if (
                        events.contains(Player.EVENT_TRACKS_CHANGED) ||
                        events.contains(Player.EVENT_AVAILABLE_COMMANDS_CHANGED)
                    ) {
                        playerView.post { ensureSubtitleButtonInteractive(playerView) }
                    }
                }
            },
        )

        Toast.makeText(this, "正在準備字幕…", Toast.LENGTH_SHORT).show()
        ioExecutor.execute {
            runCatching { convertAllSubtitleUris() }
                .onFailure { e ->
                    runOnUiThread {
                        Toast.makeText(this, "字幕轉碼失敗：${e.message}", Toast.LENGTH_LONG).show()
                    }
                }
            runOnUiThread {
                val startMs = resolveInitialPosition(savedInstanceState)
                applyMediaItem(exo, videoUri, startMs, true)
                if (startMs > 0L) {
                    Toast.makeText(
                        this,
                        "已從 ${formatResumeTime(startMs)} 接續播放",
                        Toast.LENGTH_SHORT,
                    ).show()
                }
                VideoPlaybackForegroundService.start(this, titleHint)
                alignPlaylistCurrentFromPlayback()
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent.action == ACTION_FORCE_STOP) {
            finishWithResult(null)
            return
        }
        VideoPlaybackForegroundService.setUserBrowsingApp(false)
        if (intent.getBooleanExtra(EXTRA_RESUME_PLAY, false)) {
            vlcSession?.resume()
            player?.playWhenReady = true
            if (isRemoteStreamPlayback) {
                VideoPlaybackForegroundService.start(this, titleHint)
            }
        }
    }

    override fun onResume() {
        super.onResume()
        vlcSession?.resume()
        player?.playWhenReady = true
        applyImmersiveMode(!uiChromeVisible)
        updateTopChromePadding()
        startProgressSaving()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) {
            applyImmersiveMode(!uiChromeVisible)
            updateTopChromePadding()
        }
    }

    private fun resolveInitialPosition(savedInstanceState: Bundle?): Long {
        savedInstanceState?.getLong(KEY_PLAYBACK_POSITION, 0L)?.takeIf { it > 0L }?.let {
            return it
        }
        val fromIntent = intent.getLongExtra(EXTRA_START_POSITION_MS, -1L)
        if (fromIntent >= 0L) return fromIntent
        return VideoProgressStore.load(this, progressStorageKey)
    }

    private fun formatResumeTime(positionMs: Long): String {
        val totalSec = (positionMs / 1000).toInt()
        val h = totalSec / 3600
        val m = (totalSec % 3600) / 60
        val s = totalSec % 60
        return if (h > 0) {
            String.format("%d:%02d:%02d", h, m, s)
        } else {
            String.format("%02d:%02d", m, s)
        }
    }

    private fun savedPlaybackPosition(savedInstanceState: Bundle?): Long {
        if (savedInstanceState == null) return 0L
        return savedInstanceState.getLong(KEY_PLAYBACK_POSITION, 0L)
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        vlcSession?.let { outState.putLong(KEY_PLAYBACK_POSITION, it.currentPositionMs()) }
        player?.let { outState.putLong(KEY_PLAYBACK_POSITION, it.currentPosition) }
    }

    private fun setupVlcPlayback(
        videoUri: Uri,
        savedInstanceState: Bundle?,
        startMsOverride: Long? = null,
    ) {
        val exoPlayerView = findViewById<PlayerView>(R.id.player_view)
        exoPlayerView.visibility = View.GONE
        exoPlayerView.player = null
        playerViewRef = null

        val vlcLayout = findViewById<VLCVideoLayout>(R.id.vlc_video_layout)
        vlcVideoLayoutRef = vlcLayout
        vlcLayout.visibility = View.VISIBLE

        val vlcControl = findViewById<PlayerControlView>(R.id.vlc_player_control)
        vlcControl.visibility = View.VISIBLE
        vlcControlRef = vlcControl
        controlsRootRef = vlcControl

        val startMs = startMsOverride ?: resolveInitialPosition(savedInstanceState)
        vlcBindingPlayer?.stopPolling()
        vlcBindingPlayer?.release()
        vlcBindingPlayer = null
        vlcControl.player = null
        vlcSession?.release()
        vlcSession = null
        val mediaItem = MediaItem.Builder().setUri(videoUri).build()
        val session =
            try {
                NasVlcPlayerSession(
                    context = this,
                    videoLayout = vlcLayout,
                    onEnded = {
                        VideoProgressStore.clear(this, progressStorageKey)
                        finishWithResult(null)
                    },
                    onError = { code ->
                        playbackError = code
                        Toast.makeText(this, "播放失敗，可嘗試外部 VLC", Toast.LENGTH_LONG).show()
                        finishWithResult(code)
                    },
                )
            } catch (e: Exception) {
                playbackError = e.message ?: "VLC_INIT_FAILED"
                Toast.makeText(this, "VLC 引擎初始化失敗：${e.message}", Toast.LENGTH_LONG).show()
                finishWithResult(playbackError)
                return
            }
        vlcSession = session.also {
            it.setUserBrightnessBias(videoBrightnessMatrix.brightness)
            it.setSubtitleTextScaleSp(subtitleTextSizeSp)
            it.setSimpToTradEnabled(simpToTradEnabled)
        }
        vlcBindingPlayer =
            VlcBindingPlayer(mainLooper, session, mediaItem).also {
                it.startPolling()
            }
        val wrapped = PlaylistNavPlayer(vlcBindingPlayer!!)
        playlistPlayer = wrapped
        vlcControl.player = wrapped

        vlcTapCatcherRef = findViewById(R.id.vlc_tap_catcher)
        vlcTapCatcherRef?.setOnClickListener { setVlcChromeVisible(true) }

        vlcControl.addVisibilityListener(
            PlayerControlView.VisibilityListener { visibility ->
                val visible = visibility == View.VISIBLE
                if (visible != uiChromeVisible) {
                    syncVlcTapCatcher(visible)
                }
                if (visible) {
                    vlcControl.post {
                        alignPlaylistCurrentFromPlayback()
                        ensureSubtitleButtonInteractive(vlcControl)
                    }
                }
            },
        )
        setupVlcControlInteractions(vlcControl, vlcLayout)
        setVlcChromeVisible(true)
        applyVlcWindowBrightness()

        ioExecutor.execute {
            val subs = convertSubtitleUriList(extraSubtitleUris.toList())
            runOnUiThread {
                startVlcPlayWhenReady(vlcLayout) {
                    vlcSession?.play(videoUri, startMs, subs)
                    val (ox, oy) = VideoSubtitlePrefStore.loadSubtitleOffset(this)
                    vlcSession?.applySubtitleSurfaceOffset(ox, oy)
                    if (startMs > 0L) {
                        Toast.makeText(
                            this,
                            "已從 ${formatResumeTime(startMs)} 接續播放",
                            Toast.LENGTH_SHORT,
                        ).show()
                    }
                    VideoPlaybackForegroundService.start(this, titleHint)
                    startProgressSaving()
                    alignPlaylistCurrentFromPlayback()
                }
            }
        }
    }

    private fun setVlcChromeVisible(visible: Boolean) {
        val control = vlcControlRef ?: return
        uiChromeVisible = visible
        topChromeScrollRef?.visibility = if (visible) View.VISIBLE else View.GONE
        if (visible) {
            vlcTapCatcherRef?.visibility = View.GONE
            control.visibility = View.VISIBLE
            control.show()
        } else {
            control.hide()
            vlcTapCatcherRef?.visibility = View.VISIBLE
            vlcTapCatcherRef?.bringToFront()
        }
        applyImmersiveMode(!visible)
        updateTopChromePadding()
        if (visible) {
            control.post {
                alignPlaylistCurrentFromPlayback()
                ensureSubtitleButtonInteractive(control)
            }
        }
    }

    private fun syncVlcTapCatcher(visible: Boolean) {
        uiChromeVisible = visible
        topChromeScrollRef?.visibility = if (visible) View.VISIBLE else View.GONE
        vlcTapCatcherRef?.visibility = if (visible) View.GONE else View.VISIBLE
        if (!visible) {
            vlcTapCatcherRef?.bringToFront()
        }
        applyImmersiveMode(!visible)
        updateTopChromePadding()
    }

    private fun setupVlcControlInteractions(
        control: PlayerControlView,
        touchTarget: View,
    ) {
        control.setShowTimeoutMs(0)
        control.setShowRewindButton(true)
        control.setShowFastForwardButton(true)
        control.setShowPreviousButton(true)
        control.setShowNextButton(true)
        control.setShowSubtitleButton(true)
        control.post {
            control.findViewById<View>(Media3UiR.id.exo_controls_background)?.apply {
                visibility = View.GONE
                background = null
            }
            vlcVideoLayoutRef?.let { layout ->
                pinchZoomHelper = VideoPinchZoomHelper(layout)
                val (offsetX, offsetY) = VideoSubtitlePrefStore.loadSubtitleOffset(this@LocalVideoPlayerActivity)
                vlcSubtitleDragHelper =
                    VlcSubtitleDragHelper(
                        videoLayout = layout,
                        subtitleSurfaceProvider = { vlcSession?.findSubtitleSurface() },
                        hasActiveSubtitles = { vlcSession?.hasActiveSubtitles() == true },
                    ) { x, y ->
                        VideoSubtitlePrefStore.saveSubtitleOffset(this@LocalVideoPlayerActivity, x, y)
                    }.also {
                        layout.post { it.applySavedOffset(offsetX, offsetY) }
                    }
            }
            val routeVlcTouch =
                View.OnTouchListener { _, event ->
                    vlcSubtitleDragHelper?.onTouchEvent(event, uiChromeVisible)?.let { if (it) return@OnTouchListener true }
                    val helper = pinchZoomHelper
                    if (helper != null && helper.shouldHandle(event)) {
                        helper.onTouchEvent(event)
                    } else {
                        false
                    }
                }
            control.isClickable = true
            control.setOnClickListener {
                if (uiChromeVisible) {
                    setVlcChromeVisible(false)
                }
            }
            control.setOnTouchListener { _, event ->
                if (vlcSubtitleDragHelper?.onTouchEvent(event, uiChromeVisible) == true) {
                    return@setOnTouchListener true
                }
                false
            }
            touchTarget.isClickable = true
            touchTarget.setOnClickListener {
                if (uiChromeVisible) {
                    setVlcChromeVisible(false)
                } else {
                    setVlcChromeVisible(true)
                }
            }
            touchTarget.setOnTouchListener(routeVlcTouch)
            control.findViewById<ImageButton>(Media3UiR.id.exo_settings)?.setOnClickListener {
                showVideoSettingsMenu()
            }
            ensureSubtitleButtonInteractive(control)
            setupStreamPlaylistButton(control)
            setupPlaylistSkipButtons(control)
        }
    }

    private fun startVlcPlayWhenReady(
        vlcLayout: VLCVideoLayout,
        onReady: () -> Unit,
    ) {
        fun launchIfSized(): Boolean {
            if (vlcLayout.width > 0 && vlcLayout.height > 0) {
                onReady()
                return true
            }
            return false
        }
        vlcLayout.post {
            if (launchIfSized()) return@post
            val observer = vlcLayout.viewTreeObserver
            observer.addOnGlobalLayoutListener(
                object : ViewTreeObserver.OnGlobalLayoutListener {
                    override fun onGlobalLayout() {
                        if (!launchIfSized()) return
                        vlcLayout.viewTreeObserver.removeOnGlobalLayoutListener(this)
                    }
                },
            )
        }
    }

    private fun shouldUseVlcEngine(uri: Uri, title: String): Boolean {
        val name = title.ifBlank { uri.lastPathSegment.orEmpty() }.lowercase()
        return name.endsWith(".rmvb") ||
            name.endsWith(".rm") ||
            name.contains(".rmvb") ||
            name.contains(".rm")
    }

    private fun setupWindowForVideo() {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            window.attributes =
                window.attributes.apply {
                    layoutInDisplayCutoutMode =
                        WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
                }
        }
        applyImmersiveMode(true)
    }

    private fun refreshVideoEffects() {
        vlcSession?.let {
            it.setUserBrightnessBias(videoBrightnessMatrix.brightness)
            applyVlcWindowBrightness()
            return
        }
        val effects = mutableListOf<Effect>()
        if (videoBrightnessMatrix.brightness != 0f) {
            effects.add(videoBrightnessMatrix)
        }
        player?.setVideoEffects(effects)
    }

    private fun applyVlcWindowBrightness() {
        if (vlcSession == null) return
        val bias = videoBrightnessMatrix.brightness
        val lp = window.attributes
        lp.screenBrightness = (0.55f + bias * 0.4f).coerceIn(0.15f, 1.0f)
        window.attributes = lp
    }

    private fun resetWindowScreenBrightness() {
        val lp = window.attributes
        lp.screenBrightness = WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE
        window.attributes = lp
    }

    private fun setupWindowInsets() {
        ViewCompat.setOnApplyWindowInsetsListener(window.decorView) { _, insets ->
            statusBarInsetTop = insets.getInsets(WindowInsetsCompat.Type.statusBars()).top
            updateTopChromePadding()
            insets
        }
        ViewCompat.requestApplyInsets(window.decorView)
    }

    private fun updateTopChromePadding() {
        val topChrome = topChromeScrollRef ?: return
        val extraTop =
            if (uiChromeVisible) {
                statusBarInsetTop + dpToPx(6)
            } else {
                0
            }
        topChrome.setPadding(
            topChrome.paddingLeft,
            extraTop,
            topChrome.paddingRight,
            topChrome.paddingBottom,
        )
    }

    private fun dpToPx(dp: Int): Int =
        TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP,
            dp.toFloat(),
            resources.displayMetrics,
        ).toInt()

    private fun applyImmersiveMode(hideSystemBars: Boolean) {
        val controller = WindowInsetsControllerCompat(window, window.decorView)
        if (hideSystemBars) {
            controller.hide(WindowInsetsCompat.Type.systemBars())
            controller.systemBarsBehavior =
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        } else {
            controller.show(WindowInsetsCompat.Type.systemBars())
        }
    }

    private fun setupPlayerViewInteractions(playerView: PlayerView) {
        playerView.post {
            playerView.findViewById<View>(Media3UiR.id.exo_controls_background)?.apply {
                visibility = View.GONE
                background = null
            }
            val contentFrame = playerView.findViewById<ViewGroup>(Media3UiR.id.exo_content_frame)
            contentFrameRef = contentFrame
            installContentFrameLayoutListener(contentFrame)
            val subtitleView = installDisplaySubtitleOverlay(contentFrame)
            val zoomTarget =
                if (vlcSession != null) {
                    vlcVideoLayoutRef
                } else {
                    contentFrame?.let { findVideoSurface(it) }
                }
            if (zoomTarget != null) {
                pinchZoomHelper = VideoPinchZoomHelper(zoomTarget)
            }
            if (subtitleView != null) {
                val (offsetX, offsetY) = VideoSubtitlePrefStore.loadSubtitleOffset(this)
                subtitleDragHelper =
                    SubtitleDragHelper(
                        context = this@LocalVideoPlayerActivity,
                        playerView = playerView,
                        subtitleView = subtitleView,
                        playerProvider = { playlistPlayer },
                        textSizeSpProvider = { subtitleTextSizeSp },
                        isPgsModeProvider = { embeddedSubtitleIsPgs },
                        pgsScaleProvider = { subtitlePgsScale },
                        videoSizeProvider = { lastVideoSize },
                    ) { x, y ->
                        VideoSubtitlePrefStore.saveSubtitleOffset(this, x, y)
                    }.also {
                        it.attach(playerView)
                        it.applySavedOffset(offsetX, offsetY)
                    }
            }
            playerView.setOnTouchListener { _, event ->
                subtitleDragHelper?.let {
                    if (it.onTouchEvent(event)) return@setOnTouchListener true
                }
                val helper = pinchZoomHelper
                if (helper != null && helper.shouldHandle(event)) {
                    helper.onTouchEvent(event)
                } else {
                    false
                }
            }
            playerView.findViewById<ImageButton>(Media3UiR.id.exo_settings)?.setOnClickListener {
                showVideoSettingsMenu()
            }
            ensureSubtitleButtonInteractive(playerView)
            setupStreamPlaylistButton(playerView)
            setupPlaylistSkipButtons(playerView)
            applySubtitleTextSize(subtitleTextSizeSp)
            updateEmbeddedSubtitleLayout()
        }
    }

    private fun setupStreamPlaylistButton(root: View) {
        val btn = root.findViewById<ImageButton>(R.id.nas_exo_playlist) ?: return
        if (!VideoStreamPlaylistStore.hasPlaylist()) {
            btn.visibility = View.GONE
            return
        }
        btn.visibility = View.VISIBLE
        btn.setImageResource(android.R.drawable.ic_menu_sort_by_size)
        btn.setOnClickListener { toggleStreamPlaylistPanel() }
        ensureStreamPlaylistPanel()
    }

    private fun setupPlaylistSkipButtons(root: View) {
        val prevBtn = root.findViewById<ImageButton>(Media3UiR.id.exo_prev)
        val nextBtn = root.findViewById<ImageButton>(Media3UiR.id.exo_next)
        prevBtn?.setOnClickListener {
            if (VideoStreamPlaylistStore.isPlaylistMode() && VideoStreamPlaylistStore.hasPrevious()) {
                VideoStreamPlaylistStore.getPrevious()?.let { playStreamPlaylistItem(it) }
            }
        }
        nextBtn?.setOnClickListener {
            if (VideoStreamPlaylistStore.isPlaylistMode() && VideoStreamPlaylistStore.hasNext()) {
                VideoStreamPlaylistStore.getNext()?.let { playStreamPlaylistItem(it) }
            }
        }
        refreshPlaylistSkipButtons(root)
    }

    private fun alignPlaylistCurrentFromPlayback() {
        if (!VideoStreamPlaylistStore.isPlaylistMode()) return
        pcRelPath?.let { VideoStreamPlaylistStore.alignCurrentToPath(it) }
        refreshPlaylistSkipButtons()
    }

    private fun refreshPlaylistSkipButtons(root: View? = controlsRootRef) {
        val pv = root ?: return
        val prevBtn = pv.findViewById<ImageButton>(Media3UiR.id.exo_prev) ?: return
        val nextBtn = pv.findViewById<ImageButton>(Media3UiR.id.exo_next) ?: return
        val mode = VideoStreamPlaylistStore.isPlaylistMode()
        val hasPrev = mode && VideoStreamPlaylistStore.hasPrevious()
        val hasNext = mode && VideoStreamPlaylistStore.hasNext()
        // 維持可點擊，由 listener 判斷；避免 ExoPlayer 單曲模式覆寫 enabled
        prevBtn.isEnabled = true
        nextBtn.isEnabled = true
        prevBtn.alpha = if (hasPrev) 1f else 0.35f
        nextBtn.alpha = if (hasNext) 1f else 0.35f
    }

    /** 前端 sync 播放列表或切曲後更新播放器 UI。 */
    fun refreshStreamPlaylistUi() {
        val root = controlsRootRef ?: return
        alignPlaylistCurrentFromPlayback()
        setupStreamPlaylistButton(root)
        refreshPlaylistSkipButtons(root)
        if (streamPlaylistOpen) {
            refreshStreamPlaylistPanel()
        }
    }

    private fun ensureStreamPlaylistPanel() {
        if (streamPlaylistPanel != null) return
        val root = findViewById<FrameLayout>(android.R.id.content) ?: return
        val panel =
            ScrollView(this).apply {
                visibility = View.GONE
                isFillViewport = false
                setBackgroundColor(0xE6000000.toInt())
                layoutParams =
                    FrameLayout.LayoutParams(
                        (resources.displayMetrics.widthPixels * 0.72f).toInt(),
                        FrameLayout.LayoutParams.WRAP_CONTENT,
                        android.view.Gravity.BOTTOM or android.view.Gravity.END,
                    ).apply {
                        val margin =
                            TypedValue.applyDimension(
                                TypedValue.COMPLEX_UNIT_DIP,
                                72f,
                                resources.displayMetrics,
                            ).toInt()
                        setMargins(margin / 4, 0, margin / 4, margin)
                    }
            }
        val listHost =
            LinearLayout(this).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(12, 12, 12, 12)
            }
        panel.addView(
            listHost,
            FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.WRAP_CONTENT,
            ),
        )
        root.addView(panel)
        streamPlaylistPanel = panel
        streamPlaylistListHost = listHost
        refreshStreamPlaylistPanel()
    }

    private fun toggleStreamPlaylistPanel() {
        ensureStreamPlaylistPanel()
        streamPlaylistOpen = !streamPlaylistOpen
        streamPlaylistPanel?.visibility = if (streamPlaylistOpen) View.VISIBLE else View.GONE
        if (streamPlaylistOpen) {
            refreshStreamPlaylistPanel()
        }
    }

    private fun refreshStreamPlaylistPanel() {
        val host = streamPlaylistListHost ?: return
        host.removeAllViews()
        val items = VideoStreamPlaylistStore.getItems()
        val current = VideoStreamPlaylistStore.getCurrentRelPath()
        if (items.isEmpty()) {
            host.addView(
                TextView(this).apply {
                    text = "播放列表為空"
                    setTextColor(0xFFCCCCCC.toInt())
                },
            )
            return
        }
        for (item in items) {
            val playing = current != null && VideoStreamPlaylistStore.pathsEqual(current, item.relPath)
            val row =
                Button(this).apply {
                    text = if (playing) "▶ ${item.title}" else item.title
                    isAllCaps = false
                    textAlignment = View.TEXT_ALIGNMENT_TEXT_START
                    setOnClickListener { playStreamPlaylistItem(item) }
                }
            host.addView(
                row,
                LinearLayout.LayoutParams(
                    LinearLayout.LayoutParams.MATCH_PARENT,
                    LinearLayout.LayoutParams.WRAP_CONTENT,
                ).apply { bottomMargin = 6 },
            )
        }
    }

    private fun playStreamPlaylistItem(item: VideoStreamPlaylistStore.Item) {
        streamPlaylistOpen = false
        streamPlaylistPanel?.visibility = View.GONE
        val streamUrl = buildPcStreamUrl(item.host, item.port, item.relPath)
        pcHost = item.host
        pcPort = item.port
        pcRelPath = item.relPath
        titleHint = item.title
        playbackUri = Uri.parse(streamUrl)
        VideoStreamPlaylistStore.setCurrentRelPath(item.relPath)
        progressStorageKey =
            VideoProgressStore.key(pcHost, pcPort, pcRelPath, playbackUri.toString())
        val vlc = vlcSession
        if (vlc != null) {
            if (!shouldUseVlcEngine(playbackUri!!, titleHint)) {
                Toast.makeText(this, "播放清單已切換至不支援格式，請返回後重新選擇", Toast.LENGTH_LONG).show()
                return
            }
            vlcBindingPlayer?.updateMediaItem(MediaItem.Builder().setUri(playbackUri!!).build())
            ioExecutor.execute {
                val subs = convertSubtitleUriList(extraSubtitleUris.toList())
                runOnUiThread {
                    val startMs = VideoProgressStore.load(this, progressStorageKey)
                    vlc.play(playbackUri!!, startMs, subs)
                    refreshPlaylistSkipButtons()
                    Toast.makeText(this, "播放：${item.title}", Toast.LENGTH_SHORT).show()
                }
            }
            return
        }
        val exo = player ?: return
        applyMediaItem(exo, playbackUri!!, 0L, true)
        refreshPlaylistSkipButtons()
        if (streamPlaylistOpen) {
            refreshStreamPlaylistPanel()
        }
        Toast.makeText(this, "播放：${item.title}", Toast.LENGTH_SHORT).show()
    }

    /** 隱藏 PlayerView 內建字幕（避免顯示未修正的 PGS），改用獨立 SubtitleView 疊加。 */
    private fun installDisplaySubtitleOverlay(contentFrame: ViewGroup?): SubtitleView? {
        val frame = contentFrame ?: return null
        playerViewRef?.findViewById<SubtitleView>(Media3UiR.id.exo_subtitles)?.visibility = View.GONE

        displaySubtitleView?.let { existing ->
            (existing.parent as? ViewGroup)?.removeView(existing)
        }

        val overlay =
            SubtitleView(this).apply {
                setApplyEmbeddedFontSizes(false)
                setFixedTextSize(TypedValue.COMPLEX_UNIT_SP, subtitleTextSizeSp)
                layoutParams =
                    FrameLayout.LayoutParams(
                        FrameLayout.LayoutParams.MATCH_PARENT,
                        FrameLayout.LayoutParams.MATCH_PARENT,
                    )
            }
        overlay.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
            if (embeddedSubtitleIsPgs) {
                refreshSubtitleCueDisplay()
            }
        }
        frame.addView(overlay)
        displaySubtitleView = overlay
        return overlay
    }

    private fun activeSubtitleView(): SubtitleView? = displaySubtitleView ?: playerViewRef?.subtitleView

    /** PGS / 點陣：疊在 contentFrame；文字字幕：改掛 PlayerView 全螢幕以便拖曳。 */
    private fun updateEmbeddedSubtitleLayout() {
        val playerView = playerViewRef ?: return
        val contentFrame = contentFrameRef ?: return
        val subtitleView = displaySubtitleView ?: return

        val exo = player ?: return
        embeddedSubtitleIsPgs =
            isEmbeddedPgsTrackActive() || PgsSubtitleHelper.hasBitmapCues(exo.currentCues.cues)
        if (embeddedSubtitleIsPgs) {
            attachSubtitleToContentFrame(contentFrame, subtitleView)
        } else {
            detachSubtitleFromContentFrame(contentFrame, subtitleView, playerView)
        }
        applySubtitleTransform()
        refreshSubtitleCueDisplay()
        subtitleDragHelper?.onCuesChanged()
    }

    private fun installContentFrameLayoutListener(contentFrame: ViewGroup?) {
        contentFrame ?: return
        contentFrameLayoutListener?.let { contentFrame.removeOnLayoutChangeListener(it) }
        val listener =
            View.OnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
                if (embeddedSubtitleIsPgs) {
                    refreshSubtitleCueDisplay()
                    subtitleDragHelper?.onCuesChanged()
                }
            }
        contentFrameLayoutListener = listener
        contentFrame.addOnLayoutChangeListener(listener)
    }

    private fun isEmbeddedPgsTrackActive(): Boolean {
        val exo = player ?: return false
        for (group in exo.currentTracks.groups) {
            if (group.type != C.TRACK_TYPE_TEXT) continue
            for (trackIndex in 0 until group.length) {
                if (!group.isTrackSelected(trackIndex)) continue
                val format = group.getTrackFormat(trackIndex)
                val mime = format.sampleMimeType
                if (mime != null && isPgsMimeType(mime)) return true
                val codecs = format.codecs
                if (codecs != null && codecs.contains("pgs", ignoreCase = true)) return true
            }
        }
        return false
    }

    private fun isPgsMimeType(mime: String): Boolean {
        val lower = mime.lowercase()
        return MimeTypes.APPLICATION_PGS == mime ||
            lower == "application/pgs" ||
            lower.contains("pgs")
    }

    private fun attachSubtitleToContentFrame(
        contentFrame: ViewGroup,
        subtitleView: SubtitleView,
    ) {
        if (subtitleView.parent == contentFrame) return
        (subtitleView.parent as? ViewGroup)?.removeView(subtitleView)
        contentFrame.addView(
            subtitleView,
            FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT,
            ),
        )
    }

    private fun applySubtitleTransform() {
        val sv = activeSubtitleView() ?: return
        val (offsetX, offsetY) = VideoSubtitlePrefStore.loadSubtitleOffset(this)
        val apply = {
            sv.pivotX = sv.width / 2f
            sv.pivotY = sv.height.toFloat()
            sv.translationX = offsetX
            sv.translationY = offsetY
            sv.scaleX = 1f
            sv.scaleY = 1f
            subtitleDragHelper?.onCuesChanged()
        }
        if (sv.width > 0) {
            apply()
        } else {
            sv.post { apply() }
        }
    }

    private fun applyPgsScale(scale: Float) {
        subtitlePgsScale = scale.coerceIn(MIN_PGS_SCALE, MAX_PGS_SCALE)
        VideoSubtitlePrefStore.savePgsScale(this, subtitlePgsScale)
        refreshSubtitleCueDisplay()
        subtitleDragHelper?.onCuesChanged()
    }

    private fun detachSubtitleFromContentFrame(
        contentFrame: ViewGroup,
        subtitleView: SubtitleView,
        playerView: PlayerView,
    ) {
        if (subtitleView.parent != contentFrame) return
        contentFrame.removeView(subtitleView)
        val insertIndex = (playerView.indexOfChild(contentFrame) + 1).coerceAtMost(playerView.childCount)
        playerView.addView(
            subtitleView,
            insertIndex,
            FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT,
            ),
        )
    }

    private fun findVideoSurface(root: ViewGroup): View? {
        for (i in 0 until root.childCount) {
            when (val child = root.getChildAt(i)) {
                is SurfaceView, is TextureView -> return child
                is ViewGroup -> findVideoSurface(child)?.let { return it }
            }
        }
        return null
    }

    private fun prepareSubtitleUri(sourceUri: Uri, labelHint: String): String {
        return SubtitleCharsetHelper.prepareUtf8SubtitleUri(
            this,
            sourceUri,
            labelHint,
            simpToTradEnabled,
        ).toString()
    }

    private fun reloadSubtitlesAfterPrefChange() {
        vlcSession?.let { vlc ->
            vlc.setSimpToTradEnabled(simpToTradEnabled)
            if (extraSubtitleUris.isNotEmpty()) {
                val video = playbackUri ?: return
                ioExecutor.execute {
                    val converted = convertSubtitleUriList(extraSubtitleUris.toList())
                    runOnUiThread {
                        extraSubtitleUris.clear()
                        extraSubtitleUris.addAll(converted)
                        persistExternalSubtitles()
                        reloadVlcAtCurrentPosition()
                    }
                }
            } else {
                vlc.reapplySubtitleOptionsAtCurrentPosition()
                if (simpToTradEnabled) {
                    Toast.makeText(
                        this,
                        "簡轉繁僅對外掛字幕有效；內嵌字幕請改用「本地字幕」載入",
                        Toast.LENGTH_LONG,
                    ).show()
                }
            }
            return
        }
        if (extraSubtitleUris.isEmpty()) return
        val video = playbackUri ?: return
        val exo = player ?: return
        val pos = exo.currentPosition
        val playing = exo.isPlaying
        ioExecutor.execute {
            val converted = convertSubtitleUriList(extraSubtitleUris.toList())
            runOnUiThread {
                extraSubtitleUris.clear()
                extraSubtitleUris.addAll(converted)
                persistExternalSubtitles()
                applyMediaItem(exo, video, pos, playing)
            }
        }
    }

    private fun resolveVlcResumePosition(): Long {
        val vlc = vlcSession ?: return 0L
        val live = vlc.currentPositionMs()
        if (live >= 1000L) return live
        val saved = VideoProgressStore.load(this, progressStorageKey)
        if (saved >= 1000L) return saved
        return live.coerceAtLeast(0L)
    }

    private fun reloadVlcAtCurrentPosition() {
        val uri = playbackUri ?: return
        val vlc = vlcSession ?: return
        val pos = resolveVlcResumePosition()
        val playing = vlc.isPlaying()
        vlc.setUserBrightnessBias(videoBrightnessMatrix.brightness)
        ioExecutor.execute {
            val subs = convertSubtitleUriList(extraSubtitleUris.toList())
            runOnUiThread {
                vlc.play(uri, pos, subs, hideVideoUntilSeeked = pos > 0L)
                if (!playing) {
                    vlc.pause()
                }
            }
        }
    }

    private fun ensureSubtitleButtonInteractive(root: View) {
        root.findViewById<ImageButton>(Media3UiR.id.exo_subtitle)?.apply {
            isEnabled = true
            alpha = 1f
            setOnClickListener { showSubtitleSourceMenu() }
        }
    }

    private fun showSubtitleSourceMenu() {
        AlertDialog.Builder(this)
            .setTitle("字幕")
            .setItems(arrayOf("內嵌字幕", "本地字幕", "遠端字幕")) { _, which ->
                when (which) {
                    0 -> showEmbeddedSubtitleDialog()
                    1 -> pickLocalSubtitleFile()
                    2 -> {
                        if (!hasRemotePc) {
                            Toast.makeText(this, "僅遠端 PC 串流時可用", Toast.LENGTH_SHORT).show()
                        } else {
                            pickRemoteSubtitle()
                        }
                    }
                }
            }
            .show()
    }

    private fun showEmbeddedSubtitleDialog() {
        if (vlcSession != null) {
            showVlcSpuTrackDialog()
            return
        }
        val exo = player ?: return
        val selector = trackSelectorRef ?: return
        val nameProvider = DefaultTrackNameProvider(resources)
        val textGroups =
            exo.currentTracks.groups.filter { it.type == C.TRACK_TYPE_TEXT && it.length > 0 }
        if (textGroups.isEmpty()) {
            Toast.makeText(this, "此影片沒有內嵌字幕", Toast.LENGTH_SHORT).show()
            return
        }

        data class TrackEntry(
            val group: Tracks.Group,
            val trackIndex: Int,
            val format: Format,
            val baseName: String,
            val selected: Boolean,
        )

        val entries = mutableListOf<TrackEntry>()
        for (group in textGroups) {
            for (trackIndex in 0 until group.length) {
                if (!group.isTrackSupported(trackIndex)) continue
                val format = group.getTrackFormat(trackIndex)
                val baseName = nameProvider.getTrackName(format)
                entries.add(
                    TrackEntry(
                        group = group,
                        trackIndex = trackIndex,
                        format = format,
                        baseName = baseName,
                        selected = group.isTrackSelected(trackIndex),
                    ),
                )
            }
        }

        val chineseEntries =
            entries.filter { SubtitleCharsetHelper.isChineseSubtitleTrack(it.format, it.baseName) }
        val chineseCount = chineseEntries.size

        val labels = mutableListOf<String>()
        val actions = mutableListOf<() -> Unit>()

        labels.add("關閉字幕")
        actions.add {
            selector.parameters =
                selector.buildUponParameters()
                    .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true)
                    .build()
        }

        var chineseOrdinal = 0
        for (entry in entries) {
            val isChinese =
                SubtitleCharsetHelper.isChineseSubtitleTrack(entry.format, entry.baseName)
            val label =
                if (isChinese) {
                    val ordinal = chineseOrdinal++
                    SubtitleCharsetHelper.embeddedSubtitleDisplayName(
                        entry.format,
                        entry.baseName,
                        ordinal,
                        chineseCount,
                    )
                } else {
                    entry.baseName
                } + if (entry.selected) " ✓" else ""
            labels.add(label)
            val capturedGroup = entry.group
            val capturedIndex = entry.trackIndex
            actions.add {
                selectEmbeddedTextTrack(capturedGroup, capturedIndex)
            }
        }

        AlertDialog.Builder(this)
            .setTitle("內嵌字幕")
            .setItems(labels.toTypedArray()) { _, which -> actions[which]() }
            .show()
    }

    private fun selectEmbeddedTextTrack(
        group: Tracks.Group,
        trackIndex: Int,
    ) {
        val selector = trackSelectorRef ?: return
        val override = TrackSelectionOverride(group.mediaTrackGroup, trackIndex)
        selector.parameters =
            selector.buildUponParameters()
                .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, false)
                .clearOverridesOfType(C.TRACK_TYPE_TEXT)
                .addOverride(override)
                .build()
    }

    private fun setupEmbeddedCueConverter(exo: ExoPlayer, playerView: PlayerView) {
        embeddedCueConverter?.let { exo.removeListener(it) }
        val listener =
            object : Player.Listener {
                override fun onCues(cueGroup: CueGroup) {
                    subtitleDragHelper?.onCuesChanged()
                    playerView.post { refreshSubtitleCueDisplay() }
                }
            }
        embeddedCueConverter = listener
        exo.addListener(listener)
    }

    private fun refreshSubtitleCueDisplay() {
        val exo = player ?: return
        val subtitleView = activeSubtitleView() ?: return
        val videoSize = exo.videoSize
        if (videoSize.width > 0 && videoSize.height > 0) {
            lastVideoSize = videoSize
        }
        val rawCues = exo.currentCues.cues
        if (rawCues.isEmpty()) {
            subtitleView.setCues(rawCues)
            return
        }

        embeddedSubtitleIsPgs =
            isEmbeddedPgsTrackActive() || PgsSubtitleHelper.hasBitmapCues(rawCues)

        val textAdjusted =
            if (!simpToTradEnabled) {
                rawCues
            } else {
                rawCues.map { cue ->
                    val text = cue.text?.toString() ?: return@map cue
                    val convertedText = SubtitleCharsetHelper.convertSimplifiedToTraditional(text)
                    if (convertedText == text) {
                        cue
                    } else {
                        cue.buildUpon().setText(convertedText).build()
                    }
                }
            }

        val viewportWidth =
            (subtitleView.width - subtitleView.paddingLeft - subtitleView.paddingRight)
                .coerceAtLeast(0)
        val viewportHeight =
            (subtitleView.height - subtitleView.paddingTop - subtitleView.paddingBottom)
                .coerceAtLeast(0)
        val displayVideoSize = lastVideoSize
        val par =
            if (displayVideoSize.pixelWidthHeightRatio > 0f) {
                displayVideoSize.pixelWidthHeightRatio
            } else {
                1f
            }

        val displayCues =
            if (
                embeddedSubtitleIsPgs &&
                viewportWidth > 0 &&
                viewportHeight > 0 &&
                PgsSubtitleHelper.hasBitmapCues(textAdjusted)
            ) {
                PgsSubtitleHelper.prepareForDisplay(
                    textAdjusted,
                    viewportWidth,
                    viewportHeight,
                    displayVideoSize.width,
                    displayVideoSize.height,
                    par,
                    subtitlePgsScale,
                )
            } else {
                textAdjusted
            }
        subtitleView.setCues(displayCues)
    }

    private fun pickLocalSubtitleFile() {
        if (subtitleBusy) return
        pickLocalSubtitleLauncher.launch(
            arrayOf(
                "application/x-subrip",
                "text/*",
                "application/octet-stream",
                "*/*",
            ),
        )
    }

    private fun appendLocalSubtitle(uri: Uri) {
        val name = uri.lastPathSegment ?: "local_sub.srt"
        if (extraSubtitleUris.contains(uri.toString())) {
            Toast.makeText(this, "此字幕已加入", Toast.LENGTH_SHORT).show()
            return
        }
        subtitleBusy = true
        ioExecutor.execute {
            val utf8Uri =
                runCatching {
                    SubtitleCharsetHelper.prepareUtf8SubtitleUri(this, uri, name, simpToTradEnabled).toString()
                }
            runOnUiThread {
                subtitleBusy = false
                utf8Uri
                    .onSuccess { converted ->
                        appendConvertedSubtitleUri(converted, "已加入本地外掛字幕")
                    }.onFailure { e ->
                        Toast.makeText(this, "字幕轉碼失敗：${e.message}", Toast.LENGTH_LONG).show()
                    }
            }
        }
    }

    private fun appendConvertedSubtitleUri(
        uri: String,
        toastMessage: String,
    ) {
        if (extraSubtitleUris.contains(uri)) {
            Toast.makeText(this, "此字幕已加入", Toast.LENGTH_SHORT).show()
            return
        }
        extraSubtitleUris.add(uri)
        persistExternalSubtitles()
        if (vlcSession != null) {
            val vlc = vlcSession!!
            if (vlc.addExternalSubtitle(uri)) {
                Toast.makeText(this, toastMessage, Toast.LENGTH_SHORT).show()
            } else {
                reloadVlcAtCurrentPosition()
                Toast.makeText(this, toastMessage, Toast.LENGTH_SHORT).show()
            }
            return
        }
        val exo = player ?: return
        val video = playbackUri ?: return
        val pos = exo.currentPosition
        val playing = exo.isPlaying
        applyMediaItem(exo, video, pos, playing)
        Toast.makeText(this, toastMessage, Toast.LENGTH_SHORT).show()
    }

    private fun applySubtitleTextSize(sizeSp: Float) {
        subtitleTextSizeSp = sizeSp.coerceIn(MIN_SUBTITLE_TEXT_SIZE_SP, MAX_SUBTITLE_TEXT_SIZE_SP)
        VideoSubtitlePrefStore.saveTextSizeSp(this, subtitleTextSizeSp)
        activeSubtitleView()?.apply {
            setApplyEmbeddedFontSizes(false)
            setFixedTextSize(TypedValue.COMPLEX_UNIT_SP, subtitleTextSizeSp)
        }
        subtitleDragHelper?.onCuesChanged()
    }

    private fun persistExternalSubtitles() {
        VideoSubtitlePrefStore.saveExternalUris(
            this,
            progressStorageKey,
            extraSubtitleUris,
        )
    }

    private fun persistTextTrackSelection(parameters: TrackSelectionParameters) {
        if (progressStorageKey == "unknown") return
        if (parameters.disabledTrackTypes.contains(C.TRACK_TYPE_TEXT)) {
            VideoSubtitlePrefStore.saveTextTrack(
                this,
                progressStorageKey,
                VideoSubtitlePrefStore.TextTrackSelection(
                    disabled = true,
                    groupId = null,
                    trackIndex = -1,
                ),
            )
            return
        }
        val tracks = player?.currentTracks ?: return
        for (group in tracks.groups) {
            if (group.type != C.TRACK_TYPE_TEXT) continue
            val override = parameters.overrides[group.mediaTrackGroup] ?: continue
            val trackIndex = override.trackIndices.firstOrNull() ?: continue
            VideoSubtitlePrefStore.saveTextTrack(
                this,
                progressStorageKey,
                VideoSubtitlePrefStore.TextTrackSelection(
                    disabled = false,
                    groupId = group.mediaTrackGroup.id,
                    trackIndex = trackIndex,
                ),
            )
            return
        }
    }

    private fun restoreSavedTextTrackSelection(tracks: Tracks) {
        if (subtitleSelectionRestored) return
        val selector = trackSelectorRef ?: return
        val saved = VideoSubtitlePrefStore.loadTextTrack(this, progressStorageKey) ?: return
        if (saved.disabled) {
            selector.parameters =
                selector.buildUponParameters()
                    .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true)
                    .build()
            subtitleSelectionRestored = true
            return
        }
        val groupId = saved.groupId ?: return
        for (group in tracks.groups) {
            if (group.type != C.TRACK_TYPE_TEXT) continue
            if (group.mediaTrackGroup.id != groupId) continue
            if (saved.trackIndex !in 0 until group.length) continue
            if (!group.isTrackSupported(saved.trackIndex)) continue
            selector.parameters =
                selector.buildUponParameters()
                    .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, false)
                    .addOverride(
                        TrackSelectionOverride(
                            group.mediaTrackGroup,
                            listOf(saved.trackIndex),
                        ),
                    )
                    .build()
            subtitleSelectionRestored = true
            return
        }
    }

    private fun showLeavePlaybackDialog() {
        if (leaveDialogShowing) return
        leaveDialogShowing = true
        AlertDialog.Builder(this)
            .setTitle("離開播放器")
            .setMessage("要暫停播放回到「影片播放」分頁，還是徹底結束播放？")
            .setPositiveButton("暫停播放") { _, _ ->
                leaveDialogShowing = false
                pausePlaybackAndLeave()
            }
            .setNegativeButton("結束播放") { _, _ ->
                leaveDialogShowing = false
                finishWithResult(null)
            }
            .setOnCancelListener { leaveDialogShowing = false }
            .show()
    }

    private fun pausePlaybackAndLeave() {
        persistPlaybackProgress()
        persistExternalSubtitles()
        player?.trackSelectionParameters?.let { persistTextTrackSelection(it) }
        vlcSession?.pause()
        player?.pause()
        player?.playWhenReady = false
        val uri = playbackUri?.toString().orEmpty()
        if (uri.isNotEmpty()) {
            VideoPlaybackSessionStore.save(
                this,
                uri = uri,
                title = titleHint,
                subtitleUris = extraSubtitleUris.toList(),
                pcHost = pcHost,
                pcPort = pcPort,
                pcRelPath = pcRelPath,
            )
        }
        VideoPlaybackForegroundService.stop(this)
        VideoPlaybackForegroundService.setUserBrowsingApp(true)
        VideoPlaybackCoordinator.releaseToBackground()
        startActivity(
            Intent(this, MainActivity::class.java).apply {
                addFlags(
                    Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or
                        Intent.FLAG_ACTIVITY_SINGLE_TOP or
                        Intent.FLAG_ACTIVITY_NEW_TASK,
                )
            },
        )
        moveTaskToBack(true)
    }

    private fun showVideoSettingsMenu() {
        AlertDialog.Builder(this)
            .setTitle("播放設定")
            .setItems(arrayOf("播放速度", "音訊音軌", "數位亮度", "字幕尺寸")) { _, which ->
                when (which) {
                    0 -> showPlaybackSpeedDialog()
                    1 -> showAudioTrackDialog()
                    2 -> showBrightnessDialog()
                    3 -> showSubtitleSizeDialog()
                }
            }
            .show()
    }

    private fun showPlaybackSpeedDialog() {
        val speeds = floatArrayOf(0.5f, 0.75f, 1f, 1.25f, 1.5f, 2f)
        val labels =
            speeds.map { speed ->
                if (speed == 1f) "正常 (1.0x)" else "${speed}x"
            }.toTypedArray()
        AlertDialog.Builder(this)
            .setTitle("播放速度")
            .setItems(labels) { _, which ->
                val speed = speeds[which]
                vlcSession?.setPlaybackSpeed(speed)
                    ?: player?.setPlaybackSpeed(speed)
            }
            .show()
    }

    private fun showVlcSpuTrackDialog(retry: Int = 0) {
        val vlc = vlcSession ?: return
        val tracks = vlc.spuTracks()
        if (tracks.isEmpty()) {
            if (retry < 12) {
                if (retry == 0) {
                    Toast.makeText(this, "正在載入字幕軌…", Toast.LENGTH_SHORT).show()
                }
                vlcControlRef?.postDelayed({ showVlcSpuTrackDialog(retry + 1) }, 450)
                return
            }
            Toast.makeText(this, "此影片沒有內嵌字幕", Toast.LENGTH_SHORT).show()
            return
        }
        val current = vlc.currentSpuTrack()
        val labels = mutableListOf<String>()
        val actions = mutableListOf<() -> Boolean>()
        labels.add("關閉字幕" + if (current < 0) " ✓" else "")
        actions.add { vlc.disableSubtitles() }
        val chineseTrackIds =
            tracks
                .filter { SubtitleCharsetHelper.isChineseSubtitleName(it.name) }
                .map { it.id }
        for (track in tracks) {
            val name =
                SubtitleCharsetHelper.vlcSpuTrackDisplayName(
                    track.name.ifBlank { "字幕 ${track.id}" },
                    track.id,
                    chineseTrackIds,
                )
            labels.add(name + if (track.id == current) " ✓" else "")
            val trackId = track.id
            actions.add {
                val ok = vlc.setSpuTrack(trackId)
                if (ok) {
                    val (ox, oy) = VideoSubtitlePrefStore.loadSubtitleOffset(this)
                    vlc.applySubtitleSurfaceOffset(ox, oy)
                    vlcSubtitleDragHelper?.applySavedOffset(ox, oy)
                }
                ok
            }
        }
        AlertDialog.Builder(this)
            .setTitle("內嵌字幕")
            .setItems(labels.toTypedArray()) { _, which ->
                val ok = actions[which]()
                Toast.makeText(
                    this,
                    if (ok) "字幕已切換" else "字幕切換失敗",
                    Toast.LENGTH_SHORT,
                ).show()
            }
            .show()
    }

    private fun showVlcAudioTrackDialog(retry: Int = 0) {
        val vlc = vlcSession ?: return
        val tracks = vlc.audioTracks()
        if (tracks.isEmpty()) {
            if (retry < 12) {
                if (retry == 0) {
                    Toast.makeText(this, "正在載入音訊軌…", Toast.LENGTH_SHORT).show()
                }
                vlcControlRef?.postDelayed({ showVlcAudioTrackDialog(retry + 1) }, 450)
                return
            }
            Toast.makeText(this, "沒有可切換的音訊音軌", Toast.LENGTH_SHORT).show()
            return
        }
        val current = vlc.currentAudioTrack()
        val labels =
            tracks
                .map { track ->
                    val name = track.name.ifBlank { "音軌 ${track.id}" }
                    name + if (track.id == current) " ✓" else ""
                }.toTypedArray()
        AlertDialog.Builder(this)
            .setTitle("音訊音軌")
            .setItems(labels) { _, which ->
                val ok = vlc.setAudioTrack(tracks[which].id)
                Toast.makeText(
                    this,
                    if (ok) "音軌已切換" else "音軌切換失敗",
                    Toast.LENGTH_SHORT,
                ).show()
            }
            .show()
    }

    private fun showAudioTrackDialog() {
        if (vlcSession != null) {
            showVlcAudioTrackDialog()
            return
        }
        val exo = player ?: return
        TrackSelectionDialogBuilder(this, "音訊音軌", exo, C.TRACK_TYPE_AUDIO)
            .build()
            .show()
    }

    private fun showSubtitleSizeDialog() {
        val dialogView = layoutInflater.inflate(R.layout.dialog_video_subtitle_size, null)
        val valueLabel = dialogView.findViewById<TextView>(R.id.txt_subtitle_size_value)
        val seekBar = dialogView.findViewById<SeekBar>(R.id.seek_subtitle_size)
        val simpToTradCheck = dialogView.findViewById<CheckBox>(R.id.chk_simp_to_trad)
        val dialogRoot = dialogView as ViewGroup
        val hintLabel = dialogRoot.getChildAt(0) as? TextView
        val scaleHintLabel = dialogRoot.getChildAt(3) as? TextView

        if (embeddedSubtitleIsPgs) {
            hintLabel?.text = "調整點陣字幕（PGS）縮放比例"
            scaleHintLabel?.text = "← 50%　　　200% →"
            simpToTradCheck.visibility = View.GONE
            seekBar.max = 150
            seekBar.progress =
                ((subtitlePgsScale - MIN_PGS_SCALE) * 100f).roundToInt().coerceIn(0, seekBar.max)
            valueLabel.text = "${(subtitlePgsScale * 100f).roundToInt()} %"
            seekBar.setOnSeekBarChangeListener(
                object : SeekBar.OnSeekBarChangeListener {
                    override fun onProgressChanged(
                        seekBar: SeekBar?,
                        progress: Int,
                        fromUser: Boolean,
                    ) {
                        val scale = MIN_PGS_SCALE + progress / 100f
                        applyPgsScale(scale)
                        valueLabel.text = "${(scale * 100f).roundToInt()} %"
                    }

                    override fun onStartTrackingTouch(seekBar: SeekBar?) = Unit

                    override fun onStopTrackingTouch(seekBar: SeekBar?) = Unit
                },
            )
            AlertDialog.Builder(this)
                .setTitle("點陣字幕縮放")
                .setView(dialogView)
                .setPositiveButton("完成", null)
                .setNeutralButton("重設") { _, _ ->
                    applyPgsScale(DEFAULT_PGS_SCALE)
                }
                .show()
            return
        }

        hintLabel?.text = "調整字幕顯示大小"
        scaleHintLabel?.text = "← 較小　　　較大 →"
        simpToTradCheck.visibility = View.VISIBLE
        seekBar.max = (MAX_SUBTITLE_TEXT_SIZE_SP - MIN_SUBTITLE_TEXT_SIZE_SP).toInt()
        seekBar.progress =
            (subtitleTextSizeSp - MIN_SUBTITLE_TEXT_SIZE_SP).toInt().coerceIn(0, seekBar.max)
        valueLabel.text = "${subtitleTextSizeSp.toInt()} sp"
        simpToTradCheck.isChecked = simpToTradEnabled

        val vlcMode = vlcSession != null
        if (vlcMode) {
            hintLabel?.text = "調整字幕顯示大小（鬆手後套用，不影響播放進度）"
        }
        seekBar.setOnSeekBarChangeListener(
            object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(
                    seekBar: SeekBar?,
                    progress: Int,
                    fromUser: Boolean,
                ) {
                    val sizeSp = MIN_SUBTITLE_TEXT_SIZE_SP + progress
                    if (!vlcMode) {
                        applySubtitleTextSize(sizeSp)
                    } else {
                        subtitleTextSizeSp = sizeSp.coerceIn(MIN_SUBTITLE_TEXT_SIZE_SP, MAX_SUBTITLE_TEXT_SIZE_SP)
                        VideoSubtitlePrefStore.saveTextSizeSp(this@LocalVideoPlayerActivity, subtitleTextSizeSp)
                        vlcSession?.setSubtitleTextScaleSp(subtitleTextSizeSp)
                    }
                    valueLabel.text = "${sizeSp.toInt()} sp"
                }

                override fun onStartTrackingTouch(seekBar: SeekBar?) = Unit

                override fun onStopTrackingTouch(seekBar: SeekBar?) {
                    if (vlcMode) {
                        vlcSession?.reapplySubtitleOptionsAtCurrentPosition()
                        val (ox, oy) = VideoSubtitlePrefStore.loadSubtitleOffset(this@LocalVideoPlayerActivity)
                        vlcSession?.applySubtitleSurfaceOffset(ox, oy)
                    }
                }
            },
        )

        simpToTradCheck.setOnCheckedChangeListener { _, checked ->
            if (simpToTradEnabled == checked) return@setOnCheckedChangeListener
            simpToTradEnabled = checked
            VideoSubtitlePrefStore.saveSimpToTrad(this, checked)
            reloadSubtitlesAfterPrefChange()
            refreshSubtitleCueDisplay()
        }

        AlertDialog.Builder(this)
            .setTitle("字幕尺寸")
            .setView(dialogView)
            .setPositiveButton("完成", null)
            .setNeutralButton("重設") { _, _ ->
                if (vlcMode) {
                    subtitleTextSizeSp = DEFAULT_SUBTITLE_TEXT_SIZE_SP
                    VideoSubtitlePrefStore.saveTextSizeSp(this, subtitleTextSizeSp)
                    vlcSession?.setSubtitleTextScaleSp(subtitleTextSizeSp)
                    vlcSession?.reapplySubtitleOptionsAtCurrentPosition()
                    valueLabel.text = "${subtitleTextSizeSp.toInt()} sp"
                    seekBar.progress =
                        (subtitleTextSizeSp - MIN_SUBTITLE_TEXT_SIZE_SP).toInt().coerceIn(0, seekBar.max)
                } else {
                    applySubtitleTextSize(DEFAULT_SUBTITLE_TEXT_SIZE_SP)
                }
                if (simpToTradEnabled) {
                    simpToTradCheck.isChecked = false
                }
            }
            .show()
    }

    private fun showBrightnessDialog() {
        val dialogView = layoutInflater.inflate(R.layout.dialog_video_brightness, null)
        val valueLabel = dialogView.findViewById<TextView>(R.id.txt_brightness_value)
        val seekBar = dialogView.findViewById<SeekBar>(R.id.seek_brightness)
        seekBar.progress = ((videoBrightnessMatrix.brightness + 1f) * 100f).toInt()
        valueLabel.text = formatBrightnessLabel(videoBrightnessMatrix.brightness)

        seekBar.setOnSeekBarChangeListener(
            object : SeekBar.OnSeekBarChangeListener {
                override fun onProgressChanged(
                    seekBar: SeekBar?,
                    progress: Int,
                    fromUser: Boolean,
                ) {
                    val brightness = (progress / 100f) - 1f
                    videoBrightnessMatrix.brightness = brightness
                    refreshVideoEffects()
                    valueLabel.text = formatBrightnessLabel(brightness)
                }

                override fun onStartTrackingTouch(seekBar: SeekBar?) = Unit

                override fun onStopTrackingTouch(seekBar: SeekBar?) {
                    vlcSession?.reapplyVideoAdjustAtCurrentPosition()
                }
            },
        )

        AlertDialog.Builder(this)
            .setTitle("影片亮度")
            .setView(dialogView)
            .setPositiveButton("完成", null)
            .setNeutralButton("重設亮度") { _, _ ->
                videoBrightnessMatrix.brightness = 0f
                refreshVideoEffects()
                vlcSession?.reapplyVideoAdjustAtCurrentPosition()
            }
            .show()
    }

    private fun formatBrightnessLabel(brightness: Float): String {
        val percent = (brightness * 100f).toInt()
        return when {
            percent > 0 -> "+$percent"
            percent < 0 -> "$percent"
            else -> "0（原始）"
        }
    }

    private fun convertAllSubtitleUris() {
        if (extraSubtitleUris.isEmpty()) return
        val converted = convertSubtitleUriList(extraSubtitleUris.toList())
        extraSubtitleUris.clear()
        extraSubtitleUris.addAll(converted)
    }

    private fun convertSubtitleUriList(uris: List<String>): List<String> {
        return uris.mapIndexed { index, uriString ->
            val uri = Uri.parse(uriString)
            val name = uri.lastPathSegment ?: "sub$index.srt"
            SubtitleCharsetHelper.prepareUtf8SubtitleUri(this, uri, name, simpToTradEnabled).toString()
        }
    }

    private fun scaleTopChrome() {
        val widthPx = resources.displayMetrics.widthPixels
        val textSp =
            when {
                widthPx < 640 -> 9f
                widthPx < 800 -> 10f
                widthPx < 1080 -> 11f
                else -> 12f
            }
        val btnHeight =
            TypedValue.applyDimension(
                TypedValue.COMPLEX_UNIT_DIP,
                if (widthPx < 720) 32f else 36f,
                resources.displayMetrics,
            ).toInt()

        findViewById<Button>(R.id.btn_exit)?.apply {
            setTextSize(TypedValue.COMPLEX_UNIT_SP, textSp)
            layoutParams.height = btnHeight
        }
    }

    private fun applyMediaItem(exo: ExoPlayer, videoUri: Uri, positionMs: Long, playWhenReady: Boolean) {
        subtitleSelectionRestored = false
        val mediaItem = buildMediaItem(videoUri, titleHint, extraSubtitleUris.toTypedArray())
        exo.setMediaItem(mediaItem, positionMs)
        exo.prepare()
        exo.playWhenReady = playWhenReady
    }

    private fun pickRemoteSubtitle() {
        val host = pcHost ?: return
        val port = pcPort
        val rel = pcRelPath ?: return
        if (subtitleBusy) return
        val dir =
            rel.replace('\\', '/').substringBeforeLast('/', missingDelimiterValue = "")
        subtitleBusy = true
        Toast.makeText(this, "正在讀取遠端字幕列表…", Toast.LENGTH_SHORT).show()
        ioExecutor.execute {
            val result =
                runCatching {
                    listRemoteSubtitleCandidates(host, port, dir, rel)
                }
            runOnUiThread {
                subtitleBusy = false
                result
                    .onSuccess { items ->
                        if (items.isEmpty()) {
                            Toast.makeText(this, "同資料夾內找不到字幕檔", Toast.LENGTH_LONG).show()
                            return@onSuccess
                        }
                        val labels = items.map { it.first }.toTypedArray()
                        AlertDialog.Builder(this)
                            .setTitle("選擇遠端字幕")
                            .setItems(labels) { _, which ->
                                val (_, streamUrl) = items[which]
                                appendRemoteSubtitle(streamUrl, labels[which])
                            }
                            .setNegativeButton("取消", null)
                            .show()
                    }.onFailure { e ->
                        Toast.makeText(this, "讀取失敗：${e.message}", Toast.LENGTH_LONG).show()
                    }
            }
        }
    }

    private fun appendRemoteSubtitle(streamUrl: String, fileName: String) {
        if (extraSubtitleUris.contains(streamUrl)) {
            Toast.makeText(this, "此字幕已加入", Toast.LENGTH_SHORT).show()
            return
        }
        if (subtitleBusy) return
        subtitleBusy = true
        ioExecutor.execute {
            val utf8Uri =
                runCatching {
                    SubtitleCharsetHelper.prepareUtf8SubtitleUri(
                        this,
                        Uri.parse(streamUrl),
                        fileName,
                        simpToTradEnabled,
                    ).toString()
                }
            runOnUiThread {
                subtitleBusy = false
                utf8Uri
                    .onSuccess { uri ->
                        appendConvertedSubtitleUri(uri, "已加入遠端外掛字幕")
                    }.onFailure { e ->
                        Toast.makeText(this, "字幕轉碼失敗：${e.message}", Toast.LENGTH_LONG).show()
                    }
            }
        }
    }

    private fun listRemoteSubtitleCandidates(
        host: String,
        port: Int,
        browsePath: String,
        videoRelPath: String,
    ): List<Pair<String, String>> {
        val videoBase =
            videoRelPath
                .replace('\\', '/')
                .substringAfterLast('/')
                .substringBeforeLast('.', videoRelPath)
                .lowercase()
        val entries = browsePcDirectory(host, port, browsePath)
        val subs =
            entries
                .filter { !it.second }
                .filter { isSubtitleName(it.first) }
                .map { entry ->
                    val apiPath =
                        if (browsePath.isEmpty()) entry.first
                        else "${browsePath.replace('\\', '/')}/${entry.first}"
                    val label = entry.first
                    val streamUrl = buildPcStreamUrl(host, port, apiPath)
                    val score = if (entry.first.lowercase().startsWith(videoBase)) 0 else 1
                    Triple(score, label, streamUrl)
                }
                .sortedWith(compareBy({ it.first }, { it.second.lowercase() }))
                .map { it.second to it.third }
        return subs
    }

    private fun browsePcDirectory(host: String, port: Int, path: String): List<Pair<String, Boolean>> {
        val url = URL("http://$host:$port/api/v1/browse")
        val conn = (url.openConnection() as HttpURLConnection).apply {
            requestMethod = "POST"
            connectTimeout = 30_000
            readTimeout = 60_000
            doOutput = true
            setRequestProperty("Content-Type", "application/json; charset=utf-8")
        }
        conn.outputStream.use { os ->
            os.write("""{"path":${JSONObject.quote(path)}}""".toByteArray(Charsets.UTF_8))
        }
        val code = conn.responseCode
        val body =
            (if (code in 200..299) conn.inputStream else conn.errorStream)?.use { stream ->
                BufferedReader(InputStreamReader(stream, Charsets.UTF_8)).readText()
            } ?: ""
        if (code !in 200..299) {
            throw IllegalStateException("HTTP $code${if (body.isBlank()) "" else "：$body"}")
        }
        val json = JSONObject(body)
        if (!json.optBoolean("ok", false)) {
            throw IllegalStateException("PC 回應異常")
        }
        val arr = json.optJSONArray("entries") ?: JSONArray()
        val out = ArrayList<Pair<String, Boolean>>()
        for (i in 0 until arr.length()) {
            val item = arr.optJSONObject(i) ?: continue
            val name = item.optString("name", "").trim()
            if (name.isEmpty()) continue
            out.add(name to item.optBoolean("isDir", false))
        }
        return out
    }

    private fun isSubtitleName(name: String): Boolean {
        val ext = name.substringAfterLast('.', "").lowercase()
        return ext in SUBTITLE_EXTENSIONS
    }

    private fun buildPcStreamUrl(host: String, port: Int, relPath: String): String {
        val b64 =
            Base64.encodeToString(
                relPath.toByteArray(Charsets.UTF_8),
                Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING,
            )
        return "http://$host:$port/api/v1/stream?path_b64=$b64"
    }

    private fun handlePlaybackError(uri: Uri, error: PlaybackException) {
        val code = error.errorCodeName
        val name =
            titleHint.ifBlank { uri.lastPathSegment.orEmpty() }.lowercase()
        val isRmvb =
            name.endsWith(".rmvb") ||
                name.endsWith(".rm") ||
                name.contains(".rmvb") ||
                name.contains(".rm")
        val isDecode =
            error.errorCode == PlaybackException.ERROR_CODE_DECODING_FAILED ||
                error.errorCode == PlaybackException.ERROR_CODE_DECODER_INIT_FAILED
        val isParseUnsupported =
            error.errorCode == PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED ||
                error.errorCode == PlaybackException.ERROR_CODE_PARSING_CONTAINER_MALFORMED

        if (isDecode || isParseUnsupported) {
            val title =
                when {
                    isRmvb || isParseUnsupported -> "內建播放器不支援此格式"
                    else -> "內建播放器無法解碼"
                }
            val rmvbHint =
                if (isRmvb || isParseUnsupported) {
                    "\n\nRMVB / RealMedia 為舊式封裝，ExoPlayer 無法直接解析，與 PC 配套端版本無關。"
                } else {
                    ""
                }
            val bodyHint =
                if (isDecode) {
                    "此檔可能為 x265 / 10bit MKV，裝置硬解不支援。"
                } else {
                    "此影片封裝格式內建播放器無法處理。"
                }
            AlertDialog.Builder(this)
                .setTitle(title)
                .setMessage(
                    "$bodyHint$rmvbHint\n\n" +
                        "可改用外部播放器（如 VLC）開啟同一串流網址，或在 PC 轉為 MP4 / H.264。\n\n" +
                        "錯誤：$code",
                )
                .setPositiveButton("外部播放") { _, _ ->
                    openExternalPlayer(uri)
                    finishWithResult(code)
                }
                .setNegativeButton("返回") { _, _ -> finishWithResult(code) }
                .setOnCancelListener { finishWithResult(code) }
                .show()
            return
        }

        Toast.makeText(this, "播放失敗：$code", Toast.LENGTH_LONG).show()
        finishWithResult(code)
    }

    private fun openExternalPlayer(uri: Uri) {
        try {
            val intent =
                Intent(Intent.ACTION_VIEW).apply {
                    setDataAndType(uri, "video/*")
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
            startActivity(Intent.createChooser(intent, "選擇播放器"))
        } catch (e: Exception) {
            Toast.makeText(this, "無法開啟外部播放器：${e.message}", Toast.LENGTH_LONG).show()
        }
    }

    private fun buildMediaItem(
        videoUri: Uri,
        titleHint: String,
        subtitleUris: Array<String>?,
    ): MediaItem {
        val builder = MediaItem.Builder().setUri(videoUri)
        guessVideoMime(videoUri, titleHint)?.let { builder.setMimeType(it) }
        val subs = subtitleUris?.filter { it.isNotBlank() } ?: emptyList()
        if (subs.isNotEmpty()) {
            val configs =
                subs.mapIndexed { index, uriString ->
                    val uri = Uri.parse(uriString)
                    val name = uri.lastPathSegment ?: "外掛字幕"
                    MediaItem.SubtitleConfiguration.Builder(uri)
                        .setMimeType(guessSubtitleMime(name))
                        .setLanguage("zh")
                        .setLabel("外掛 ${index + 1}")
                        .setSelectionFlags(C.SELECTION_FLAG_DEFAULT)
                        .build()
                }
            builder.setSubtitleConfigurations(configs)
        }
        return builder.build()
    }

    private fun guessVideoMime(uri: Uri, titleHint: String): String? {
        val name =
            titleHint.ifBlank { uri.lastPathSegment.orEmpty() }.lowercase()
        return when {
            name.endsWith(".mkv") -> MimeTypes.VIDEO_MATROSKA
            name.endsWith(".webm") -> MimeTypes.VIDEO_WEBM
            name.endsWith(".mp4") || name.endsWith(".m4v") -> MimeTypes.VIDEO_MP4
            name.endsWith(".ts") || name.endsWith(".m2ts") -> MimeTypes.VIDEO_MP2T
            name.endsWith(".rmvb") || name.endsWith(".rm") -> "application/vnd.rn-realmedia-vbr"
            uri.scheme?.startsWith("http") == true && name.contains(".mkv") -> MimeTypes.VIDEO_MATROSKA
            uri.scheme?.startsWith("http") == true &&
                (name.contains(".rmvb") || name.contains(".rm")) -> "application/vnd.rn-realmedia-vbr"
            else -> null
        }
    }

    private fun guessSubtitleMime(fileName: String): String {
        return when (fileName.substringAfterLast('.', "").lowercase()) {
            "ass", "ssa" -> MimeTypes.TEXT_SSA
            "vtt", "webvtt" -> MimeTypes.TEXT_VTT
            "ttml", "xml" -> MimeTypes.APPLICATION_TTML
            else -> MimeTypes.APPLICATION_SUBRIP
        }
    }

    private fun resolvePlaybackUri(): Uri? {
        intent.data?.let { return it }
        val extra = intent.getStringExtra(EXTRA_URI)?.trim().orEmpty()
        if (extra.isNotEmpty()) {
            return Uri.parse(extra)
        }
        return null
    }

    private fun finishWithResult(error: String?) {
        persistPlaybackProgress()
        persistExternalSubtitles()
        player?.trackSelectionParameters?.let { persistTextTrackSelection(it) }
        VideoPlaybackSessionStore.clear(this)
        VideoPlaybackForegroundService.setUserBrowsingApp(false)
        VideoPlaybackForegroundService.stop(this)
        vlcBindingPlayer?.stopPolling()
        vlcBindingPlayer?.release()
        vlcBindingPlayer = null
        vlcSession?.release()
        vlcSession = null
        embeddedCueConverter?.let { player?.removeListener(it) }
        embeddedCueConverter = null
        player?.run {
            playWhenReady = false
            stop()
            release()
        }
        player = null
        VideoPlaybackCoordinator.complete(error)
        val data = Intent()
        if (!error.isNullOrBlank()) {
            data.putExtra(EXTRA_PLAYBACK_ERROR, error)
        }
        setResult(RESULT_OK, data)
        if (!isFinishing) {
            finishAndRemoveTask()
        }
    }

    private fun persistPlaybackProgress() {
        if (progressStorageKey == "unknown") return
        val vlc = vlcSession
        if (vlc != null) {
            VideoProgressStore.save(
                this,
                progressStorageKey,
                vlc.currentPositionMs(),
                vlc.durationMs(),
            )
            return
        }
        val exo = player ?: return
        VideoProgressStore.save(
            this,
            progressStorageKey,
            exo.currentPosition,
            exo.duration,
        )
    }

    private fun startProgressSaving() {
        progressSaveHandler.removeCallbacks(progressSaveRunnable)
        progressSaveHandler.postDelayed(progressSaveRunnable, 3000)
    }

    private fun stopProgressSaving() {
        progressSaveHandler.removeCallbacks(progressSaveRunnable)
    }

    private val progressSaveRunnable =
        object : Runnable {
            override fun run() {
                persistPlaybackProgress()
                progressSaveHandler.postDelayed(this, 3000)
            }
        }

    override fun onPause() {
        super.onPause()
        persistPlaybackProgress()
        stopProgressSaving()
        vlcSession?.pause()
        if (isFinishing) {
            player?.pause()
        }
    }

    override fun onStop() {
        super.onStop()
        stopProgressSaving()
        persistPlaybackProgress()
    }

    override fun onDestroy() {
        if (activeInstance?.get() === this) {
            activeInstance = null
        }
        persistPlaybackProgress()
        resetWindowScreenBrightness()
        VideoPlaybackForegroundService.stop(this)
        stopProgressSaving()
        embeddedCueConverter?.let { player?.removeListener(it) }
        embeddedCueConverter = null
        ioExecutor.shutdownNow()
        vlcBindingPlayer?.stopPolling()
        vlcBindingPlayer?.release()
        vlcBindingPlayer = null
        vlcSession?.release()
        vlcSession = null
        vlcVideoLayoutRef = null
        vlcTapCatcherRef = null
        vlcSubtitleDragHelper = null
        vlcControlRef?.player = null
        vlcControlRef = null
        controlsRootRef = null
        player?.release()
        player = null
        bandwidthMeter = null
        playerViewRef = null
        playlistPlayer = null
        displaySubtitleView = null
        contentFrameRef = null
        super.onDestroy()
    }

    private inner class PlaylistNavPlayer(
        delegate: Player,
    ) : ForwardingPlayer(delegate) {
        override fun isCommandAvailable(command: Int): Boolean {
            if (!VideoStreamPlaylistStore.isPlaylistMode()) {
                when (command) {
                    Player.COMMAND_SEEK_TO_PREVIOUS,
                    Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
                    Player.COMMAND_SEEK_TO_NEXT,
                    Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM -> return false
                }
            } else {
                when (command) {
                    Player.COMMAND_SEEK_TO_PREVIOUS,
                    Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM ->
                        return VideoStreamPlaylistStore.hasPrevious()
                    Player.COMMAND_SEEK_TO_NEXT,
                    Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM ->
                        return VideoStreamPlaylistStore.hasNext()
                }
            }
            return super.isCommandAvailable(command)
        }

        override fun hasPreviousMediaItem(): Boolean =
            VideoStreamPlaylistStore.isPlaylistMode() && VideoStreamPlaylistStore.hasPrevious()

        override fun hasNextMediaItem(): Boolean =
            VideoStreamPlaylistStore.isPlaylistMode() && VideoStreamPlaylistStore.hasNext()

        override fun seekToPreviousMediaItem() {
            VideoStreamPlaylistStore.getPrevious()?.let { playStreamPlaylistItem(it) }
                ?: super.seekToPreviousMediaItem()
        }

        override fun seekToNextMediaItem() {
            VideoStreamPlaylistStore.getNext()?.let { playStreamPlaylistItem(it) }
                ?: super.seekToNextMediaItem()
        }
    }

    companion object {
        private var activeInstance: WeakReference<LocalVideoPlayerActivity>? = null

        const val ACTION_FORCE_STOP = "com.gentleman.manager.android.action.FORCE_STOP_PLAYBACK"
        const val EXTRA_RESUME_PLAY = "resume_play"

        fun isPlayerAlive(): Boolean = activeInstance?.get() != null

        fun notifyPlaylistSynced() {
            val activity = activeInstance?.get() ?: return
            activity.runOnUiThread { activity.refreshStreamPlaylistUi() }
        }

        fun requestStopPlayback(context: android.content.Context) {
            val inst = activeInstance?.get()
            if (inst != null) {
                inst.runOnUiThread { inst.finishWithResult(null) }
                return
            }
            VideoPlaybackSessionStore.clear(context)
            VideoPlaybackForegroundService.setUserBrowsingApp(false)
            VideoPlaybackForegroundService.stop(context)
        }

        private val SUBTITLE_EXTENSIONS = setOf("srt", "ass", "ssa", "vtt", "sub", "sup")
        private const val KEY_PLAYBACK_POSITION = "playback_position"
        private const val DEFAULT_SUBTITLE_TEXT_SIZE_SP = 16f
        private const val MIN_SUBTITLE_TEXT_SIZE_SP = 8f
        private const val MAX_SUBTITLE_TEXT_SIZE_SP = 48f
        private const val DEFAULT_PGS_SCALE = 1f
        private const val MIN_PGS_SCALE = 0.5f
        private const val MAX_PGS_SCALE = 2f

        const val EXTRA_URI = "uri"
        const val EXTRA_TITLE = "title"
        const val EXTRA_SUBTITLE_URIS = "subtitleUris"
        const val EXTRA_PC_HOST = "pcHost"
        const val EXTRA_PC_PORT = "pcPort"
        const val EXTRA_PC_REL_PATH = "pcRelPath"
        const val EXTRA_START_POSITION_MS = "startPositionMs"
        const val EXTRA_PLAYBACK_ERROR = "playback_error"
    }
}
