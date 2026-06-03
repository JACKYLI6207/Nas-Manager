package com.gentleman.manager.android

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.RectF
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import android.util.TypedValue
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import androidx.media3.common.VideoSize
import androidx.media3.common.text.Cue
import androidx.media3.common.Player
import androidx.media3.ui.SubtitleView
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * 字幕拖曳：文字軌對齊 SubtitlePainter.setupTextLayout；PGS 點陣軌對齊 setupBitmapLayout。
 */
class SubtitleDragHelper(
    context: Context,
    private val playerView: View,
    private val subtitleView: SubtitleView,
    private val playerProvider: () -> Player?,
    private val textSizeSpProvider: () -> Float,
    private val isPgsModeProvider: () -> Boolean,
    private val pgsScaleProvider: () -> Float,
    private val videoSizeProvider: () -> VideoSize,
    private val onOffsetChanged: (Float, Float) -> Unit,
) {
    private val outlineOverlay = OutlineOverlayView(context)
    private var dragging = false
    private var lastRawX = 0f
    private var lastRawY = 0f
    private var armed = false
    private var outlineVisible = false
    private var outlineVisibleBeforeDown = false
    private var downX = 0f
    private var downY = 0f
    private var cueBounds: RectF? = null
    private val dragThresholdPx =
        TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP,
            8f,
            context.resources.displayMetrics,
        )
    private val outlinePadPx =
        TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP,
            2f,
            context.resources.displayMetrics,
        )

    init {
        outlineOverlay.layoutParams =
            ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT,
            )
        outlineOverlay.isClickable = false
        outlineOverlay.isFocusable = false
    }

    fun attach(parent: ViewGroup) {
        if (outlineOverlay.parent == null) {
            parent.addView(outlineOverlay, parent.childCount)
        }
        subtitleView.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
            refreshCueBounds()
        }
    }

    fun applySavedOffset(offsetX: Float, offsetY: Float) {
        subtitleView.translationX = offsetX
        subtitleView.translationY = offsetY
        subtitleView.post { refreshCueBounds() }
    }

    fun onCuesChanged() {
        subtitleView.post { refreshCueBounds() }
    }

    fun onTouchEvent(event: MotionEvent): Boolean {
        if (!hasActiveSubtitles()) {
            dragging = false
            armed = false
            hideOutline()
            return false
        }
        refreshCueBounds()
        val bounds = cueBounds ?: return false

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (!bounds.contains(event.x, event.y)) {
                    hideOutline()
                    return false
                }
                armed = true
                downX = event.x
                downY = event.y
                lastRawX = event.rawX
                lastRawY = event.rawY
                outlineVisibleBeforeDown = outlineVisible
                if (!outlineVisible) {
                    showOutline()
                }
                return true
            }
            MotionEvent.ACTION_MOVE -> {
                if (!armed) return false
                val moved = hypot(event.x - downX, event.y - downY)
                if (!dragging && moved >= dragThresholdPx) {
                    dragging = true
                }
                if (dragging && outlineVisible) {
                    val dx = event.rawX - lastRawX
                    val dy = event.rawY - lastRawY
                    subtitleView.translationX += dx
                    subtitleView.translationY += dy
                    lastRawX = event.rawX
                    lastRawY = event.rawY
                    refreshCueBounds()
                    onOffsetChanged(subtitleView.translationX, subtitleView.translationY)
                }
                return true
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (!armed) return false
                val moved = hypot(event.x - downX, event.y - downY)
                if (dragging) {
                    onOffsetChanged(subtitleView.translationX, subtitleView.translationY)
                } else if (outlineVisibleBeforeDown && moved < dragThresholdPx) {
                    hideOutline()
                }
                dragging = false
                armed = false
                return true
            }
        }
        return false
    }

    private fun showOutline() {
        outlineVisible = true
        refreshCueBounds()
    }

    private fun hideOutline() {
        outlineVisible = false
        outlineOverlay.outlineRect = null
        outlineOverlay.invalidate()
    }

    private fun refreshCueBounds() {
        cueBounds = computeCueBounds()
        if (outlineVisible) {
            outlineOverlay.outlineRect = cueBounds
            outlineOverlay.invalidate()
        }
    }

    private fun computeCueBounds(): RectF? {
        val rawCues = playerProvider()?.currentCues?.cues.orEmpty()
        if (rawCues.isEmpty()) return null

        val sv = subtitleView
        val rawHeight = sv.height
        if (sv.width <= 0 || rawHeight <= 0) return null

        val parentLeft = sv.paddingLeft
        val parentTop = sv.paddingTop
        val parentRight = sv.width - sv.paddingRight
        val parentBottom = rawHeight - sv.paddingBottom
        val parentWidth = parentRight - parentLeft
        val parentHeight = parentBottom - parentTop
        if (parentWidth <= 0 || parentHeight <= 0) return null

        val defaultTextSizePx =
            TypedValue.applyDimension(
                TypedValue.COMPLEX_UNIT_SP,
                textSizeSpProvider(),
                sv.resources.displayMetrics,
            )
        if (defaultTextSizePx <= 0f) return null

        val bottomPaddingFraction = SubtitleView.DEFAULT_BOTTOM_PADDING_FRACTION

        val cues =
            if (isPgsModeProvider() && PgsSubtitleHelper.hasBitmapCues(rawCues)) {
                val videoSize = videoSizeProvider()
                val par =
                    if (videoSize.pixelWidthHeightRatio > 0f) {
                        videoSize.pixelWidthHeightRatio
                    } else {
                        1f
                    }
                PgsSubtitleHelper.prepareForDisplay(
                    rawCues,
                    parentWidth,
                    parentHeight,
                    videoSize.width,
                    videoSize.height,
                    par,
                    pgsScaleProvider(),
                )
            } else {
                rawCues
            }

        var union: RectF? = null
        for (cue in cues) {
            val local =
                if (cue.bitmap != null) {
                    computeBitmapCueBounds(
                        cue,
                        parentLeft,
                        parentTop,
                        parentRight,
                        parentBottom,
                        parentWidth,
                        parentHeight,
                    )
                } else {
                    computeSingleTextCueBounds(
                        cue,
                        parentLeft,
                        parentTop,
                        parentRight,
                        parentBottom,
                        parentWidth,
                        parentHeight,
                        defaultTextSizePx,
                        bottomPaddingFraction,
                    )
                } ?: continue
            val mapped = mapLocalRectToPlayerView(local)
            union =
                if (union == null) {
                    mapped
                } else {
                    union.apply { union(mapped) }
                }
        }
        return union
    }

    /** 將 SubtitleView 內座標轉為 playerView 觸控座標（含 translation / scale / 巢狀 parent）。 */
    private fun mapLocalRectToPlayerView(local: RectF): RectF {
        val sv = subtitleView
        val pivotX = if (sv.pivotX > 0f) sv.pivotX else sv.width / 2f
        val pivotY = if (sv.pivotY > 0f) sv.pivotY else sv.height.toFloat()
        val sx = sv.scaleX
        val sy = sv.scaleY

        fun mapPoint(x: Float, y: Float): Pair<Float, Float> {
            val scaledX = (x - pivotX) * sx + pivotX + sv.translationX
            val scaledY = (y - pivotY) * sy + pivotY + sv.translationY
            val (baseX, baseY) = viewOffsetInPlayerView(sv)
            return baseX + scaledX to baseY + scaledY
        }

        val (x1, y1) = mapPoint(local.left, local.top)
        val (x2, y2) = mapPoint(local.right, local.bottom)
        return RectF(
            minOf(x1, x2),
            minOf(y1, y2),
            maxOf(x1, x2),
            maxOf(y1, y2),
        )
    }

    private fun viewOffsetInPlayerView(view: View): Pair<Float, Float> {
        var x = 0f
        var y = 0f
        var current: View? = view
        while (current != null && current !== playerView) {
            x += current.left
            y += current.top
            val parent = current.parent
            current = parent as? View
        }
        return x to y
    }

    /** 對齊 SubtitlePainter.setupBitmapLayout。 */
    private fun computeBitmapCueBounds(
        cue: Cue,
        parentLeft: Int,
        parentTop: Int,
        parentRight: Int,
        parentBottom: Int,
        parentWidth: Int,
        parentHeight: Int,
    ): RectF? {
        val bitmap = cue.bitmap ?: return null
        if (bitmap.isRecycled) return null

        val position = if (cue.position != Cue.DIMEN_UNSET) cue.position else 0.5f
        val line = if (cue.line != Cue.DIMEN_UNSET) cue.line else 0.9f
        val anchorX = parentLeft + parentWidth * position
        val anchorY = parentTop + parentHeight * line

        val cueSize = if (cue.size != Cue.DIMEN_UNSET) cue.size else 1f
        val width = (parentWidth * cueSize).roundToInt()
        val height =
            if (cue.bitmapHeight == Cue.DIMEN_UNSET) {
                (width * (bitmap.height.toFloat() / max(bitmap.width, 1))).roundToInt()
            } else {
                (parentHeight * cue.bitmapHeight).roundToInt()
            }
        if (width <= 0 || height <= 0) return null

        val x =
            when (cue.positionAnchor) {
                Cue.ANCHOR_TYPE_END -> (anchorX - width).roundToInt()
                Cue.ANCHOR_TYPE_MIDDLE -> ((anchorX * 2 - width) / 2).roundToInt()
                else -> anchorX.roundToInt()
            }
        val y =
            when (cue.lineAnchor) {
                Cue.ANCHOR_TYPE_END -> (anchorY - height).roundToInt()
                Cue.ANCHOR_TYPE_MIDDLE -> ((anchorY * 2 - height) / 2).roundToInt()
                else -> anchorY.roundToInt()
            }

        return RectF(
            x - outlinePadPx,
            y - outlinePadPx,
            (x + width) + outlinePadPx,
            (y + height) + outlinePadPx,
        ).apply {
            left = left.coerceAtLeast(parentLeft.toFloat())
            top = top.coerceAtLeast(parentTop.toFloat())
            right = right.coerceAtMost(parentRight.toFloat())
            bottom = bottom.coerceAtMost(parentBottom.toFloat())
        }
    }

    /** 對齊 SubtitlePainter.setupTextLayout。 */
    private fun computeSingleTextCueBounds(
        cue: Cue,
        parentLeft: Int,
        parentTop: Int,
        parentRight: Int,
        parentBottom: Int,
        parentWidth: Int,
        parentHeight: Int,
        defaultTextSizePx: Float,
        bottomPaddingFraction: Float,
    ): RectF? {
        val text = cue.text?.toString()?.trim().orEmpty()
        if (text.isEmpty()) return null

        val textPaint =
            TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
                textSize = defaultTextSizePx
            }
        val textPaddingX = (defaultTextSizePx * INNER_PADDING_RATIO + 0.5f).toInt()

        var availableWidth = parentWidth - textPaddingX * 2
        if (cue.size != Cue.DIMEN_UNSET) {
            availableWidth = (availableWidth * cue.size).toInt()
        }
        if (availableWidth <= 0) return null

        val alignment = cue.textAlignment ?: Layout.Alignment.ALIGN_CENTER
        val textLayout =
            StaticLayout.Builder
                .obtain(text, 0, text.length, textPaint, availableWidth)
                .setAlignment(alignment)
                .setLineSpacing(0f, 1f)
                .setIncludePad(true)
                .build()

        val textHeight = textLayout.height
        var textWidth = 0
        for (i in 0 until textLayout.lineCount) {
            textWidth = max(textWidth, textLayout.getLineWidth(i).roundToInt())
        }
        if (cue.size != Cue.DIMEN_UNSET && textWidth < availableWidth) {
            textWidth = availableWidth
        }
        textWidth += textPaddingX * 2

        val textLeft: Int
        val textRight: Int
        if (cue.position != Cue.DIMEN_UNSET) {
            val anchorPosition = (parentWidth * cue.position).roundToInt() + parentLeft
            textLeft =
                when (cue.positionAnchor) {
                    Cue.ANCHOR_TYPE_END -> anchorPosition - textWidth
                    Cue.ANCHOR_TYPE_MIDDLE -> (anchorPosition * 2 - textWidth) / 2
                    else -> anchorPosition
                }.coerceIn(parentLeft, max(parentLeft, parentRight - textWidth))
            textRight = minOf(textLeft + textWidth, parentRight)
        } else {
            textLeft = (parentWidth - textWidth) / 2 + parentLeft
            textRight = textLeft + textWidth
        }
        if (textRight - textLeft <= 0) return null

        val textTop =
            if (cue.line != Cue.DIMEN_UNSET) {
                val rawTop =
                    if (cue.lineType == Cue.LINE_TYPE_FRACTION) {
                        val anchorPosition = (parentHeight * cue.line).roundToInt() + parentTop
                        when (cue.lineAnchor) {
                            Cue.ANCHOR_TYPE_END -> anchorPosition - textHeight
                            Cue.ANCHOR_TYPE_MIDDLE -> (anchorPosition * 2 - textHeight) / 2
                            else -> anchorPosition
                        }
                    } else {
                        val firstLineHeight = textLayout.getLineBottom(0) - textLayout.getLineTop(0)
                        if (cue.line >= 0) {
                            (cue.line * firstLineHeight).roundToInt() + parentTop
                        } else {
                            ((cue.line + 1) * firstLineHeight).roundToInt() + parentBottom - textHeight
                        }
                    }
                rawTop.coerceIn(parentTop, max(parentTop, parentBottom - textHeight))
            } else {
                parentBottom - textHeight - (parentHeight * bottomPaddingFraction).toInt()
            }

        return RectF(
            textLeft - outlinePadPx,
            textTop - outlinePadPx,
            textRight + outlinePadPx,
            textTop + textHeight + outlinePadPx,
        )
    }

    private fun hasActiveSubtitles(): Boolean {
        return !playerProvider()?.currentCues?.cues.isNullOrEmpty()
    }

    private inner class OutlineOverlayView(context: Context) : View(context) {
        var outlineRect: RectF? = null
        private val paint =
            Paint(Paint.ANTI_ALIAS_FLAG).apply {
                style = Paint.Style.STROKE
                color = Color.parseColor("#FFD54F")
                strokeWidth =
                    TypedValue.applyDimension(
                        TypedValue.COMPLEX_UNIT_DIP,
                        2f,
                        resources.displayMetrics,
                    )
            }

        override fun onTouchEvent(event: MotionEvent): Boolean = false

        override fun onDraw(canvas: Canvas) {
            outlineRect?.let { canvas.drawRect(it, paint) }
        }
    }

    private companion object {
        const val INNER_PADDING_RATIO = 0.125f
    }
}
