package com.sillytavern.rs

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.webkit.JavascriptInterface
import androidx.core.content.FileProvider
import java.io.File

/**
 * Lets the web UI install the update APK that Rust downloaded into the cache folder, so
 * updating never needs GitHub or a browser. Exposed to the WebView as `SillyInstaller`.
 *
 * Android still checks the signature itself: only an APK signed with this app's key can
 * replace it, whatever ends up in the cache folder.
 */
class ApkInstaller(private val activity: Activity) {
  /** False until the user allows "Install unknown apps" for this app (Android 8+). */
  @JavascriptInterface
  fun canInstall(): Boolean {
    // Before Android 8 there is no per-app switch; the installer itself asks.
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return true
    return activity.packageManager.canRequestPackageInstalls()
  }

  /** Opens this app's "Install unknown apps" switch in the system settings. */
  @JavascriptInterface
  fun openInstallSettings() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val intent = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${activity.packageName}"))
    activity.runOnUiThread { activity.startActivity(intent) }
  }

  /** Hands the downloaded APK to the system installer. False when the file is not the update. */
  @JavascriptInterface
  fun install(path: String): Boolean {
    val file = File(path).canonicalFile
    val cache = activity.cacheDir.canonicalFile
    if (file.name != "update.apk" || !file.isFile || !file.path.startsWith(cache.path + File.separator)) return false
    val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
    val intent = Intent(Intent.ACTION_VIEW)
      .setDataAndType(uri, "application/vnd.android.package-archive")
      .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    activity.runOnUiThread { activity.startActivity(intent) }
    return true
  }
}
