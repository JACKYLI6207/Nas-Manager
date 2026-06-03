package com.gentleman.manager.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

/** 背景串流時維持播放，通知列可點擊回到播放器。 */
class VideoPlaybackForegroundService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        isRunning = true
        val title = intent?.getStringExtra(EXTRA_TITLE)?.trim().orEmpty().ifBlank { "影片播放中" }
        ensureChannel()
        val openPlayer =
            PendingIntent.getActivity(
                this,
                0,
                Intent(this, LocalVideoPlayerActivity::class.java).apply {
                    addFlags(
                        Intent.FLAG_ACTIVITY_NEW_TASK or
                            Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or
                            Intent.FLAG_ACTIVITY_SINGLE_TOP,
                    )
                },
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        val notification =
            NotificationCompat.Builder(this, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.ic_media_play)
                .setContentTitle("Nas Manager 串流播放")
                .setContentText(title)
                .setContentIntent(openPlayer)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setCategory(Notification.CATEGORY_TRANSPORT)
                .build()
        startForeground(NOTIFICATION_ID, notification)
        return START_STICKY
    }

    override fun onDestroy() {
        isRunning = false
        stopForeground(STOP_FOREGROUND_REMOVE)
        super.onDestroy()
    }

    private fun ensureChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val mgr = getSystemService(NotificationManager::class.java)
        mgr.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                "影片播放",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "背景串流播放"
                setShowBadge(false)
            },
        )
    }

    companion object {
        private const val CHANNEL_ID = "nas_video_playback"
        private const val NOTIFICATION_ID = 9101
        private const val EXTRA_TITLE = "title"

        @Volatile
        var isRunning: Boolean = false
            private set

        /** 使用者刻意回到 App 瀏覽時，MainActivity 不要自動跳回播放器。 */
        @Volatile
        var userBrowsingApp: Boolean = false
            private set

        fun setUserBrowsingApp(browsing: Boolean) {
            userBrowsingApp = browsing
        }

        fun start(context: Context, title: String) {
            userBrowsingApp = false
            val intent =
                Intent(context, VideoPlaybackForegroundService::class.java).apply {
                    putExtra(EXTRA_TITLE, title)
                }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            userBrowsingApp = false
            context.stopService(Intent(context, VideoPlaybackForegroundService::class.java))
        }
    }
}
