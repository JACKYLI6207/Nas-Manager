package com.gentleman.manager.android

import android.app.Activity
import android.content.Intent
import android.os.Handler
import android.os.Looper
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject

/** 以 startActivity 開啟播放器，避免 ActivityResultLauncher 未註冊導致無法播放。 */
object VideoPlaybackCoordinator {
    private val mainHandler = Handler(Looper.getMainLooper())
    private var pendingInvoke: Invoke? = null

    @Synchronized
    fun launch(
        activity: Activity,
        intent: Intent,
        invoke: Invoke,
    ) {
        pendingInvoke = invoke
        activity.startActivity(intent)
    }

    @Synchronized
    fun resolveImmediately(invoke: Invoke, error: String? = null) {
        mainHandler.post {
            val ret = JSObject()
            ret.put("cancelled", false)
            ret.put("error", error ?: "")
            ret.put("background", false)
            invoke.resolve(ret)
        }
    }

    @Synchronized
    fun complete(error: String?) {
        val invoke = pendingInvoke ?: return
        pendingInvoke = null
        mainHandler.post {
            val ret = JSObject()
            ret.put("cancelled", false)
            ret.put("error", error ?: "")
            ret.put("background", false)
            invoke.resolve(ret)
        }
    }

    /** 使用者選擇背景繼續播放：先解除前端 await，播放器 Activity 仍保留。 */
    @Synchronized
    fun releaseToBackground() {
        val invoke = pendingInvoke ?: return
        pendingInvoke = null
        mainHandler.post {
            val ret = JSObject()
            ret.put("cancelled", false)
            ret.put("error", "")
            ret.put("background", true)
            invoke.resolve(ret)
        }
    }
}
