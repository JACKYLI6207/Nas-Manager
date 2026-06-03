package com.gentleman.manager.android

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
    }

    override fun onResume() {
        super.onResume()
        if (
            VideoPlaybackForegroundService.isRunning &&
            !VideoPlaybackForegroundService.userBrowsingApp
        ) {
            bringVideoPlayerToFront()
        }
    }

    private fun bringVideoPlayerToFront() {
        startActivity(
            Intent(this, LocalVideoPlayerActivity::class.java).apply {
                addFlags(
                    Intent.FLAG_ACTIVITY_NEW_TASK or
                        Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or
                        Intent.FLAG_ACTIVITY_SINGLE_TOP,
                )
            },
        )
    }
}
