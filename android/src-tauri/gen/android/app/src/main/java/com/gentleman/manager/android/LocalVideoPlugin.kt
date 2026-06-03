package com.gentleman.manager.android

import android.app.Activity
import android.content.Intent
import android.net.Uri
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class StreamPlaylistJobArgs {
    var host: String = ""
    var port: Int = 0
    var relPath: String = ""
    var title: String = ""
}

@InvokeArg
class SyncStreamPlaylistArgs {
    var jobs: Array<StreamPlaylistJobArgs>? = null
    var currentRelPath: String? = null
}

@InvokeArg
class PlayLocalVideoArgs {
    var uri: String = ""
    var title: String? = null
    var subtitleUris: Array<String>? = null
    var pcHost: String? = null
    var pcPort: Int = 0
    var pcRelPath: String? = null
    var startPositionMs: Long? = 0L
    var resumeOnly: Boolean? = false
}

@TauriPlugin
class LocalVideoPlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun pickLocalVideo(invoke: Invoke) {
        val intent =
            Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "video/*"
                putExtra(
                    Intent.EXTRA_MIME_TYPES,
                    arrayOf(
                        "video/*",
                        "application/x-matroska",
                        "application/mp4",
                        "application/octet-stream",
                    ),
                )
                addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION,
                )
            }
        startActivityForResult(invoke, intent, "pickLocalVideoResult")
    }

    @Command
    fun pickLocalSubtitle(invoke: Invoke) {
        val intent =
            Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "*/*"
                putExtra(
                    Intent.EXTRA_MIME_TYPES,
                    arrayOf(
                        "application/x-subrip",
                        "text/*",
                        "application/octet-stream",
                    ),
                )
                addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION,
                )
            }
        startActivityForResult(invoke, intent, "pickLocalSubtitleResult")
    }

    @Command
    fun playLocalVideo(invoke: Invoke) {
        val args = invoke.parseArgs(PlayLocalVideoArgs::class.java)
        val uriString = args.uri.trim()
        if (uriString.isEmpty()) {
            invoke.reject("缺少影片 URI")
            return
        }

        if (args.resumeOnly == true) {
            if (LocalVideoPlayerActivity.isPlayerAlive()) {
                try {
                    activity.startActivity(
                        buildBringPlayerIntent().apply {
                            putExtra(LocalVideoPlayerActivity.EXTRA_RESUME_PLAY, true)
                        },
                    )
                    VideoPlaybackCoordinator.resolveImmediately(invoke)
                } catch (e: Exception) {
                    invoke.reject("無法回到播放器：${e.message}")
                }
                return
            }
            if (VideoPlaybackSessionStore.isActive(activity)) {
                invoke.reject("播放器已關閉，請重新選擇影片")
                return
            }
            invoke.reject("沒有可接續的播放")
            return
        }

        val uri = Uri.parse(uriString)
        val progressKey =
            VideoProgressStore.key(
                args.pcHost?.trim()?.takeIf { it.isNotEmpty() },
                args.pcPort,
                args.pcRelPath?.trim()?.takeIf { it.isNotEmpty() },
                uriString,
            )
        val startMs =
            when {
                (args.startPositionMs ?: 0L) > 0 -> args.startPositionMs ?: 0L
                else -> VideoProgressStore.load(activity, progressKey)
            }

        try {
            val playIntent = buildPlayIntent(args, uri, uriString, startMs)
            VideoPlaybackCoordinator.launch(activity, playIntent, invoke)
        } catch (e: Exception) {
            invoke.reject("無法開啟播放器：${e.message}")
        }
    }

    private fun buildBringPlayerIntent(): Intent =
        Intent(activity, LocalVideoPlayerActivity::class.java).apply {
            addFlags(
                Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or
                    Intent.FLAG_ACTIVITY_SINGLE_TOP,
            )
        }

    private fun buildPlayIntent(
        args: PlayLocalVideoArgs,
        uri: Uri,
        uriString: String,
        startMs: Long,
    ): Intent =
        Intent(activity, LocalVideoPlayerActivity::class.java).apply {
            if (uri.scheme == "content") {
                setDataAndType(
                    uri,
                    activity.contentResolver.getType(uri) ?: "video/*",
                )
            } else {
                putExtra(LocalVideoPlayerActivity.EXTRA_URI, uriString)
            }
            putExtra(LocalVideoPlayerActivity.EXTRA_TITLE, args.title ?: "")
            if (startMs > 0L) {
                putExtra(LocalVideoPlayerActivity.EXTRA_START_POSITION_MS, startMs)
            }
            args.subtitleUris?.let {
                if (it.isNotEmpty()) {
                    putExtra(LocalVideoPlayerActivity.EXTRA_SUBTITLE_URIS, it)
                }
            }
            args.pcHost?.trim()?.takeIf { it.isNotEmpty() }?.let {
                putExtra(LocalVideoPlayerActivity.EXTRA_PC_HOST, it)
            }
            if (args.pcPort > 0) {
                putExtra(LocalVideoPlayerActivity.EXTRA_PC_PORT, args.pcPort)
            }
            args.pcRelPath?.trim()?.takeIf { it.isNotEmpty() }?.let {
                putExtra(LocalVideoPlayerActivity.EXTRA_PC_REL_PATH, it)
            }
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_ACTIVITY_NEW_TASK or
                    Intent.FLAG_ACTIVITY_SINGLE_TOP or
                    Intent.FLAG_ACTIVITY_REORDER_TO_FRONT,
            )
        }

    @Command
    fun syncStreamPlaylist(invoke: Invoke) {
        val args = invoke.parseArgs(SyncStreamPlaylistArgs::class.java)
        val arr = org.json.JSONArray()
        args.jobs?.forEach { job ->
            arr.put(
                org.json.JSONObject()
                    .put("host", job.host.trim())
                    .put("port", job.port)
                    .put("relPath", job.relPath.trim())
                    .put("title", job.title.trim()),
            )
        }
        VideoStreamPlaylistStore.setFromJson(arr.toString(), args.currentRelPath)
        LocalVideoPlayerActivity.notifyPlaylistSynced()
        val ret = JSObject()
        ret.put("ok", true)
        invoke.resolve(ret)
    }

    @Command
    fun stopVideoPlayback(invoke: Invoke) {
        LocalVideoPlayerActivity.requestStopPlayback(activity)
        val ret = JSObject()
        ret.put("ok", true)
        invoke.resolve(ret)
    }

    @Command
    fun getBackgroundPlaybackSession(invoke: Invoke) {
        val ret = JSObject()
        ret.put("sessionJson", VideoPlaybackSessionStore.toJson(activity).orEmpty())
        invoke.resolve(ret)
    }

    @ActivityCallback
    fun playLocalVideoResult(
        invoke: Invoke,
        result: androidx.activity.result.ActivityResult,
    ) {
        activity.runOnUiThread {
            val ret = JSObject()
            if (result.resultCode != Activity.RESULT_OK) {
                ret.put("cancelled", true)
                ret.put("error", "")
            } else {
                val err = result.data?.getStringExtra(LocalVideoPlayerActivity.EXTRA_PLAYBACK_ERROR)
                ret.put("cancelled", false)
                ret.put("error", err ?: "")
            }
            invoke.resolve(ret)
        }
    }

    @ActivityCallback
    fun pickLocalVideoResult(
        invoke: Invoke,
        result: androidx.activity.result.ActivityResult,
    ) {
        activity.runOnUiThread { resolveUriPick(invoke, result) }
    }

    @ActivityCallback
    fun pickLocalSubtitleResult(
        invoke: Invoke,
        result: androidx.activity.result.ActivityResult,
    ) {
        activity.runOnUiThread { resolveUriPick(invoke, result) }
    }

    private fun resolveUriPick(
        invoke: Invoke,
        result: androidx.activity.result.ActivityResult,
    ) {
        if (result.resultCode != Activity.RESULT_OK) {
            val ret = JSObject()
            ret.put("cancelled", true)
            ret.put("uri", "")
            invoke.resolve(ret)
            return
        }
        val uri = result.data?.data
        if (uri == null) {
            invoke.reject("未取得檔案 URI")
            return
        }
        try {
            activity.contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION,
            )
        } catch (_: SecurityException) {
        }
        val ret = JSObject()
        ret.put("cancelled", false)
        ret.put("uri", uri.toString())
        invoke.resolve(ret)
    }
}
