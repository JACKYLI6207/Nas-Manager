package com.gentleman.manager.android

import android.os.Handler
import android.os.Looper
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackParameters
import androidx.media3.common.Player
import androidx.media3.common.SimpleBasePlayer
import androidx.media3.common.util.UnstableApi
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture

/**
 * 將 LibVLC 播放狀態橋接至 Media3 控制列（與 ExoPlayer 相同 UI）。
 */
@UnstableApi
class VlcBindingPlayer(
    looper: Looper,
    private val session: NasVlcPlayerSession,
    private var mediaItem: MediaItem,
) : SimpleBasePlayer(looper) {
    private val pollHandler = Handler(looper)
    private val pollRunnable =
        object : Runnable {
            override fun run() {
                invalidateState()
                pollHandler.postDelayed(this, POLL_MS)
            }
        }

    fun startPolling() {
        pollHandler.removeCallbacks(pollRunnable)
        pollHandler.post(pollRunnable)
    }

    fun stopPolling() {
        pollHandler.removeCallbacks(pollRunnable)
    }

    fun updateMediaItem(item: MediaItem) {
        mediaItem = item
        invalidateState()
    }

    override fun getState(): State {
        val duration = session.durationMs()
        val position = session.currentPositionMs()
        val playing = session.isPlaying()
        val commands =
            Player.Commands.Builder()
                .addAll(
                    Player.COMMAND_PLAY_PAUSE,
                    Player.COMMAND_SEEK_IN_CURRENT_MEDIA_ITEM,
                    Player.COMMAND_SEEK_BACK,
                    Player.COMMAND_SEEK_FORWARD,
                    Player.COMMAND_SEEK_TO_PREVIOUS_MEDIA_ITEM,
                    Player.COMMAND_SEEK_TO_NEXT_MEDIA_ITEM,
                    Player.COMMAND_GET_CURRENT_MEDIA_ITEM,
                    Player.COMMAND_GET_TIMELINE,
                    Player.COMMAND_GET_METADATA,
                    Player.COMMAND_SET_SPEED_AND_PITCH,
                )
                .build()
        val playbackState =
            when {
                duration <= 0L && position <= 0L -> Player.STATE_BUFFERING
                else -> Player.STATE_READY
            }
        val durationUs = if (duration > 0L) duration * 1000L else C.TIME_UNSET
        val itemData =
            MediaItemData.Builder(PLAYLIST_UID)
                .setMediaItem(mediaItem)
                .setIsSeekable(true)
                .setDurationUs(durationUs)
                .build()
        val bufferedMs = if (duration > 0L) position.coerceAtMost(duration) else position
        return State.Builder()
            .setAvailableCommands(commands)
            .setPlaylist(listOf(itemData))
            .setCurrentMediaItemIndex(0)
            .setPlaybackState(playbackState)
            .setPlayWhenReady(playing, Player.PLAY_WHEN_READY_CHANGE_REASON_USER_REQUEST)
            .setContentPositionMs(position)
            .setContentBufferedPositionMs(PositionSupplier.getConstant(bufferedMs))
            .setSeekBackIncrementMs(SEEK_MS)
            .setSeekForwardIncrementMs(SEEK_MS)
            .setPlaybackParameters(PlaybackParameters(session.playbackSpeed()))
            .build()
    }

    override fun handleSetPlayWhenReady(playWhenReady: Boolean): ListenableFuture<*> {
        if (playWhenReady) {
            session.resume()
        } else {
            session.pause()
        }
        invalidateState()
        return Futures.immediateVoidFuture()
    }

    override fun handleSeek(
        mediaItemIndex: Int,
        positionMs: Long,
        seekCommand: Int,
    ): ListenableFuture<*> {
        val duration = session.durationMs()
        val current = session.currentPositionMs()
        val target =
            when (seekCommand) {
                Player.COMMAND_SEEK_BACK -> (current - SEEK_MS).coerceAtLeast(0L)
                Player.COMMAND_SEEK_FORWARD ->
                    if (duration > 0L) {
                        (current + SEEK_MS).coerceAtMost(duration)
                    } else {
                        current + SEEK_MS
                    }
                else -> positionMs
            }
        session.seekTo(target)
        invalidateState()
        return Futures.immediateVoidFuture()
    }

    override fun handleSetPlaybackParameters(
        playbackParameters: PlaybackParameters,
    ): ListenableFuture<*> {
        session.setPlaybackSpeed(playbackParameters.speed)
        invalidateState()
        return Futures.immediateVoidFuture()
    }

    override fun handleRelease(): ListenableFuture<*> {
        stopPolling()
        return Futures.immediateVoidFuture()
    }

    companion object {
        private const val POLL_MS = 400L
        private const val PLAYLIST_UID = "vlc-media-0"
        private const val SEEK_MS = 10_000L
    }
}
