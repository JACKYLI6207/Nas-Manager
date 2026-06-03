package com.gentleman.manager.android

import androidx.media3.common.util.UnstableApi
import androidx.media3.effect.RgbMatrix

/**
 * 以 RGB 矩陣調整影片數位亮度（-1 全暗偏移 ～ +1 全亮偏移，0 為原始）。
 */
@UnstableApi
class VideoBrightnessMatrix : RgbMatrix {
    @Volatile
    var brightness: Float = 0f
        set(value) {
            field = value.coerceIn(MIN, MAX)
        }

    override fun getMatrix(
        presentationTimeUs: Long,
        useHdr: Boolean,
    ): FloatArray {
        val offset = brightness * 0.35f
        return floatArrayOf(
            1f, 0f, 0f, 0f,
            0f, 1f, 0f, 0f,
            0f, 0f, 1f, 0f,
            offset, offset, offset, 1f,
        )
    }

    override fun isNoOp(
        inputWidth: Int,
        inputHeight: Int,
    ): Boolean = brightness == 0f

    companion object {
        const val MIN = -1f
        const val MAX = 1f
    }
}
