package com.gentleman.manager.android

import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import kotlin.math.abs

/**
 * 雙指捏合縮放影片畫面；放大後可單指拖移。
 */
class VideoPinchZoomHelper(
    private val zoomTarget: View,
) {
    private var scaleFactor = 1f
    private var translateX = 0f
    private var translateY = 0f
    private var lastPanX = 0f
    private var lastPanY = 0f
    private var panning = false
    private var panMoved = false

    private val scaleGestureDetector =
        ScaleGestureDetector(
            zoomTarget.context,
            object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
                override fun onScaleBegin(detector: ScaleGestureDetector): Boolean = true

                override fun onScale(detector: ScaleGestureDetector): Boolean {
                    scaleFactor =
                        (scaleFactor * detector.scaleFactor).coerceIn(MIN_SCALE, MAX_SCALE)
                    if (scaleFactor <= MIN_SCALE + 0.01f) {
                        resetTransform()
                    } else {
                        clampTranslation()
                        applyTransform()
                    }
                    return true
                }
            },
        )

    fun shouldHandle(event: MotionEvent): Boolean {
        if (event.pointerCount >= 2) return true
        if (scaleFactor > MIN_SCALE + 0.01f && event.pointerCount == 1) {
            return when (event.actionMasked) {
                MotionEvent.ACTION_DOWN, MotionEvent.ACTION_MOVE, MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> true
                else -> false
            }
        }
        return false
    }

    fun onTouchEvent(event: MotionEvent): Boolean {
        var handled = scaleGestureDetector.onTouchEvent(event)
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (event.pointerCount == 1 && scaleFactor > MIN_SCALE + 0.01f) {
                    panning = true
                    panMoved = false
                    lastPanX = event.x
                    lastPanY = event.y
                    handled = true
                }
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                panning = false
                handled = true
            }
            MotionEvent.ACTION_MOVE -> {
                if (panning && event.pointerCount == 1 && scaleFactor > MIN_SCALE + 0.01f) {
                    val dx = event.x - lastPanX
                    val dy = event.y - lastPanY
                    if (abs(dx) > 2f || abs(dy) > 2f) {
                        panMoved = true
                    }
                    translateX += dx
                    translateY += dy
                    lastPanX = event.x
                    lastPanY = event.y
                    clampTranslation()
                    applyTransform()
                    handled = true
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (panning && !panMoved && event.pointerCount <= 1) {
                    panning = false
                    return false
                }
                panning = false
            }
        }
        return handled || event.pointerCount >= 2
    }

    private fun applyTransform() {
        zoomTarget.scaleX = scaleFactor
        zoomTarget.scaleY = scaleFactor
        zoomTarget.translationX = translateX
        zoomTarget.translationY = translateY
    }

    private fun resetTransform() {
        scaleFactor = MIN_SCALE
        translateX = 0f
        translateY = 0f
        applyTransform()
    }

    private fun clampTranslation() {
        val w = zoomTarget.width.toFloat()
        val h = zoomTarget.height.toFloat()
        if (w <= 0f || h <= 0f) return
        val maxX = (w * (scaleFactor - 1f)) / 2f
        val maxY = (h * (scaleFactor - 1f)) / 2f
        translateX = translateX.coerceIn(-maxX, maxX)
        translateY = translateY.coerceIn(-maxY, maxY)
    }

    companion object {
        private const val MIN_SCALE = 1f
        private const val MAX_SCALE = 4f
    }
}
