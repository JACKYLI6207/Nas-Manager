package com.gentleman.manager.android

import android.util.TypedValue
import android.view.MotionEvent
import android.view.View
import org.videolan.libvlc.util.VLCVideoLayout
import kotlin.math.hypot

/**
 * LibVLC 內嵌字幕拖曳：只平移字幕 Surface（不縮放，避免壓扁）。
 */
class VlcSubtitleDragHelper(
    private val videoLayout: VLCVideoLayout,
    private val subtitleSurfaceProvider: () -> View?,
    private val hasActiveSubtitles: () -> Boolean,
    private val onOffsetChanged: (Float, Float) -> Unit,
) {
    private var dragging = false
    private var armed = false
    private var lastRawX = 0f
    private var lastRawY = 0f
    private var downX = 0f
    private var downY = 0f
    private val dragThresholdPx =
        TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP,
            8f,
            videoLayout.resources.displayMetrics,
        )
    private val controlsExcludePx =
        TypedValue.applyDimension(
            TypedValue.COMPLEX_UNIT_DIP,
            132f,
            videoLayout.resources.displayMetrics,
        )

    fun applySavedOffset(offsetX: Float, offsetY: Float) {
        subtitleSurfaceProvider()?.apply {
            scaleX = 1f
            scaleY = 1f
            translationX = offsetX
            translationY = offsetY
        }
    }

    fun onTouchEvent(
        event: MotionEvent,
        uiChromeVisible: Boolean,
    ): Boolean {
        if (!hasActiveSubtitles()) {
            dragging = false
            armed = false
            return false
        }
        val target = subtitleSurfaceProvider() ?: return false
        val layoutH = videoLayout.height
        val layoutW = videoLayout.width
        if (layoutH <= 0 || layoutW <= 0) return false
        val excludeBottom = if (uiChromeVisible) controlsExcludePx else 0f
        val zoneTop = layoutH * 0.18f
        val zoneBottom = layoutH - excludeBottom
        if (zoneBottom <= zoneTop) return false

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (event.y < zoneTop || event.y > zoneBottom) return false
                downX = event.x
                downY = event.y
                lastRawX = event.rawX
                lastRawY = event.rawY
                armed = true
                dragging = false
                return true
            }
            MotionEvent.ACTION_MOVE -> {
                if (!armed) return false
                val moved = hypot(event.x - downX, event.y - downY)
                if (!dragging && moved < dragThresholdPx) return true
                dragging = true
                target.scaleX = 1f
                target.scaleY = 1f
                target.translationX += event.rawX - lastRawX
                target.translationY += event.rawY - lastRawY
                lastRawX = event.rawX
                lastRawY = event.rawY
                onOffsetChanged(target.translationX, target.translationY)
                return true
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (!armed) return false
                if (dragging) {
                    onOffsetChanged(target.translationX, target.translationY)
                }
                val handled = armed
                dragging = false
                armed = false
                return handled
            }
        }
        return false
    }
}
