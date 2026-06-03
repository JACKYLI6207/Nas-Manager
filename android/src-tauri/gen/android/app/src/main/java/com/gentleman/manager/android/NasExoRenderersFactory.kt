package com.gentleman.manager.android

import android.content.Context
import android.os.Handler
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.decoder.ffmpeg.ExperimentalFfmpegVideoRenderer
import androidx.media3.decoder.ffmpeg.FfmpegLibrary
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.Renderer
import androidx.media3.exoplayer.mediacodec.MediaCodecSelector
import androidx.media3.exoplayer.video.VideoRendererEventListener

/**
 * 優先註冊 FFmpeg 視訊解碼（HEVC/x265/10bit MKV），再 fallback MediaCodec。
 */
@OptIn(UnstableApi::class)
class NasExoRenderersFactory(context: Context) : DefaultRenderersFactory(context) {
    init {
        setExtensionRendererMode(EXTENSION_RENDERER_MODE_PREFER)
        setEnableDecoderFallback(true)
    }

    override fun buildVideoRenderers(
        context: Context,
        extensionRendererMode: Int,
        mediaCodecSelector: MediaCodecSelector,
        enableDecoderFallback: Boolean,
        eventHandler: Handler,
        eventListener: VideoRendererEventListener,
        allowedVideoJoiningTimeMs: Long,
        out: ArrayList<Renderer>,
    ) {
        if (FfmpegLibrary.isAvailable()) {
            out.add(
                ExperimentalFfmpegVideoRenderer(
                    allowedVideoJoiningTimeMs,
                    eventHandler,
                    eventListener,
                    MAX_DROPPED_VIDEO_FRAME_COUNT_TO_NOTIFY,
                ),
            )
        }
        super.buildVideoRenderers(
            context,
            extensionRendererMode,
            mediaCodecSelector,
            enableDecoderFallback,
            eventHandler,
            eventListener,
            allowedVideoJoiningTimeMs,
            out,
        )
    }
}
