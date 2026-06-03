package com.gentleman.manager.android

import androidx.media3.common.text.Cue
import kotlin.math.roundToInt

/**
 * PGS 點陣字幕顯示修正。
 *
 * Blu-ray PGS 座標通常綁在固定合成平面（常見 1920×1080），與實際影片顯示比例
 * （如 2.39:1）不一致時，Media3 會用 [Cue.size] / [Cue.bitmapHeight] 分別縮放寬高而壓扁。
 * 此處改為：映射到影片有效區域 + 清除 bitmapHeight 以等比縮放。
 */
object PgsSubtitleHelper {
    fun isBitmapCue(cue: Cue): Boolean {
        val bitmap = cue.bitmap
        return bitmap != null && !bitmap.isRecycled
    }

    fun hasBitmapCues(cues: List<Cue>): Boolean = cues.any(::isBitmapCue)

    fun prepareForDisplay(
        cues: List<Cue>,
        viewportWidth: Int,
        viewportHeight: Int,
        videoWidth: Int,
        videoHeight: Int,
        videoPixelAspectRatio: Float,
        userScale: Float,
    ): List<Cue> {
        if (viewportWidth <= 0 || viewportHeight <= 0 || !hasBitmapCues(cues)) {
            return cues
        }
        val scale = userScale.coerceIn(MIN_PGS_SCALE, MAX_PGS_SCALE)
        return cues.map { cue ->
            if (!isBitmapCue(cue)) {
                cue
            } else {
                remapBitmapCue(
                    cue,
                    videoWidth,
                    videoHeight,
                    videoPixelAspectRatio,
                    scale,
                )
            }
        }
    }

    private fun remapBitmapCue(
        cue: Cue,
        videoWidth: Int,
        videoHeight: Int,
        videoPixelAspectRatio: Float,
        userScale: Float,
    ): Cue {
        val bitmap = cue.bitmap ?: return cue
        val planeSize = cue.size
        if (planeSize == Cue.DIMEN_UNSET || planeSize <= 0f) {
            return cue.buildUpon().setBitmapHeight(Cue.DIMEN_UNSET).build()
        }

        val planeWidth = (bitmap.width / planeSize).roundToInt().coerceAtLeast(1)
        val planeHeight =
            if (cue.bitmapHeight != Cue.DIMEN_UNSET && cue.bitmapHeight > 0f) {
                (bitmap.height / cue.bitmapHeight).roundToInt().coerceAtLeast(1)
            } else {
                (planeWidth * 9f / 16f).roundToInt().coerceAtLeast(1)
            }

        val planeAspect = planeWidth.toFloat() / planeHeight
        val videoAspect =
            if (videoWidth > 0 && videoHeight > 0) {
                videoWidth * videoPixelAspectRatio.coerceAtLeast(0.01f) / videoHeight
            } else {
                planeAspect
            }

        val region = mapPlaneToVideoRegion(planeAspect, videoAspect)

        val position = if (cue.position != Cue.DIMEN_UNSET) cue.position else 0.5f
        val line = if (cue.line != Cue.DIMEN_UNSET) cue.line else 0.9f

        val mappedPosition =
            ((position - region.offsetX) / region.activeWidthFraction)
                .coerceIn(0f, 1f)
        val mappedLine =
            if (cue.lineType == Cue.LINE_TYPE_FRACTION) {
                ((line - region.offsetY) / region.activeHeightFraction)
                    .coerceIn(0f, 1f)
            } else {
                line
            }

        val mappedSize =
            (planeSize / region.activeWidthFraction * userScale)
                .coerceIn(MIN_CUE_SIZE, MAX_CUE_SIZE)

        return cue.buildUpon()
            .setBitmapHeight(Cue.DIMEN_UNSET)
            .setPosition(mappedPosition)
            .setLine(mappedLine, cue.lineType)
            .setSize(mappedSize)
            .build()
    }

    /**
     * PGS 平面與影片顯示比例不一致時，有效畫面在平面上的比例區域。
     * 例：2.39:1 影片印在 16:9 PGS 平面 → 左右黑邊（pillarbox）。
     */
    private fun mapPlaneToVideoRegion(
        planeAspect: Float,
        videoAspect: Float,
    ): PlaneVideoRegion {
        if (planeAspect <= 0f || videoAspect <= 0f) {
            return PlaneVideoRegion(0f, 0f, 1f, 1f)
        }
        val delta = videoAspect - planeAspect
        return if (delta > 0.02f) {
            val activeWidth = planeAspect / videoAspect
            val offsetX = (1f - activeWidth) / 2f
            PlaneVideoRegion(offsetX, 0f, activeWidth, 1f)
        } else if (delta < -0.02f) {
            val activeHeight = videoAspect / planeAspect
            val offsetY = (1f - activeHeight) / 2f
            PlaneVideoRegion(0f, offsetY, 1f, activeHeight)
        } else {
            PlaneVideoRegion(0f, 0f, 1f, 1f)
        }
    }

    private data class PlaneVideoRegion(
        val offsetX: Float,
        val offsetY: Float,
        val activeWidthFraction: Float,
        val activeHeightFraction: Float,
    )

    private const val MIN_PGS_SCALE = 0.5f
    private const val MAX_PGS_SCALE = 2f
    private const val MIN_CUE_SIZE = 0.01f
    private const val MAX_CUE_SIZE = 2f
}
