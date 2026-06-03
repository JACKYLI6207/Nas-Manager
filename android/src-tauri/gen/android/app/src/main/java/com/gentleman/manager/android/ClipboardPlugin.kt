package com.gentleman.manager.android

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg
class CopyTextArgs {
    var text: String = ""
}

@TauriPlugin
class ClipboardPlugin(private val activity: Activity) : Plugin(activity) {

    @Command
    fun copyText(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(CopyTextArgs::class.java)
            val text = args.text
            if (text.isEmpty()) {
                invoke.reject("empty text")
                return
            }
            val clipboard =
                activity.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            clipboard.setPrimaryClip(ClipData.newPlainText("nas-manager-log", text))
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject(e.message ?: "copy failed")
        }
    }
}
