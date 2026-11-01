// RustySound's activity (Dioxus.toml: application.android_main_activity).
// dx's default is `class MainActivity : WryActivity()`; this one adds what
// makes the web UI feel at home on Android (docs/ANDROID.md):
// * edge to edge: the UI draws under the status and navigation bars and
//   gets their sizes as --safe-top/right/bottom/left (the same variables the
//   iPhone build fills from env(safe-area-inset-*)); the keyboard shrinks the
//   web view instead of covering it;
// * bar icons that follow the theme's background (light or dark);
// * system Back: closes the open sheet or Now Playing, then goes back a page,
//   then moves the app to the background as Home does. It never finishes
//   the activity: tao ends the process when its window is destroyed, and
//   playback (rustysound-audio's service) lives in this process;
// * haptic ticks for the swipe gestures;
// * Android TV: tells the page it runs on a TV (D-pad navigation).
// Playback lives in the rustysound-audio module (AudioBridge, PlaybackService).
package dev.dioxus.main

import android.app.UiModeManager
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.os.Build
import android.os.Bundle
import android.view.HapticFeedbackConstants
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import app.adarcher.rustysound.audio.AudioBridge

typealias BuildConfig = app.adarcher.rustysound.BuildConfig

class MainActivity : WryActivity() {
    private var webView: WebView? = null

    @Volatile
    private var insetsJson = "{\"top\":0,\"right\":0,\"bottom\":0,\"left\":0}"

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        AudioBridge.init(this)
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                val web = webView ?: return leave()
                web.evaluateJavascript(
                    "(function(){var b=window.__rsAndroidBack;return b?b():'none';})()",
                ) { result -> if (result == "\"none\"") leave() }
            }
        })
    }

    override fun onWebViewCreate(webView: WebView) {
        this.webView = webView
        webView.addJavascriptInterface(Shell(), "RustySoundAndroid")
        ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout(),
            )
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
            val density = resources.displayMetrics.density
            fun css(px: Int) = Math.round(px / density)
            // With the keyboard up the web view ends above it, so nothing
            // sits under the navigation bar any more.
            val bottom = if (ime.bottom > 0) 0 else css(bars.bottom)
            insetsJson = "{\"top\":${css(bars.top)},\"right\":${css(bars.right)}," +
                "\"bottom\":$bottom,\"left\":${css(bars.left)}}"
            (view.parent as? android.view.View)?.setPadding(0, 0, 0, ime.bottom)
            webView.evaluateJavascript("window.__rsApplyInsets&&window.__rsApplyInsets()", null)
            WindowInsetsCompat.CONSUMED
        }
    }

    private fun leave() {
        moveTaskToBack(true)
    }

    /** `window.RustySoundAndroid` in the page (src/android_shell.rs). */
    private inner class Shell {
        @JavascriptInterface
        fun insets(): String = insetsJson

        /** Android TV (or another leanback device): D-pad navigation. */
        @JavascriptInterface
        fun isTv(): Boolean =
            (getSystemService(UI_MODE_SERVICE) as UiModeManager).currentModeType ==
                Configuration.UI_MODE_TYPE_TELEVISION ||
                packageManager.hasSystemFeature(PackageManager.FEATURE_LEANBACK)

        @JavascriptInterface
        fun setDarkBars(dark: Boolean) = runOnUiThread {
            WindowInsetsControllerCompat(window, window.decorView).apply {
                isAppearanceLightStatusBars = !dark
                isAppearanceLightNavigationBars = !dark
            }
        }

        @JavascriptInterface
        fun haptic(kind: String) = runOnUiThread {
            val feedback = when {
                kind == "confirm" && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R ->
                    HapticFeedbackConstants.CONFIRM
                kind == "confirm" -> HapticFeedbackConstants.VIRTUAL_KEY
                else -> HapticFeedbackConstants.CLOCK_TICK
            }
            webView?.performHapticFeedback(feedback)
        }

        @JavascriptInterface
        fun leave() = runOnUiThread { this@MainActivity.leave() }
    }
}
