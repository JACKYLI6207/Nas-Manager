# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.

# Keep entry / Tauri shell (release minify 可能導致啟動閃退)
-keep class com.gentleman.manager.android.MainActivity { *; }
-keep class com.gentleman.manager.android.TauriActivity { *; }
-keep class com.gentleman.manager.android.WryActivity { *; }
-keep class com.gentleman.manager.android.Rust { *; }
-keep class com.gentleman.manager.android.RustWebView { *; }

# Keep custom Android plugin classes used by Rust register_android_plugin.
-keep class com.gentleman.manager.android.FolderPickerPlugin { *; }
-keep class com.gentleman.manager.android.LanDiscoveryPlugin { *; }
-keep class com.gentleman.manager.android.LanDiscoveryPlugin$* { *; }
-keep class com.gentleman.manager.android.LocalVideoPlugin { *; }
-keep class com.gentleman.manager.android.LocalVideoPlugin$* { *; }
-keep class com.gentleman.manager.android.ClipboardPlugin { *; }
-keep class com.gentleman.manager.android.ClipboardPlugin$* { *; }
-keep class com.gentleman.manager.android.LocalVideoPlayerActivity { *; }
-keep class com.gentleman.manager.android.PlayLocalVideoArgs { *; }

-dontwarn androidx.media3.**
-keep class androidx.media3.** { *; }
-keep class com.gentleman.manager.android.ReadArgs { *; }
-keep class com.gentleman.manager.android.TreeArgs { *; }
-keep class com.gentleman.manager.android.CopyToTreeArgs { *; }
-keep class com.gentleman.manager.android.AppendLineArgs { *; }
