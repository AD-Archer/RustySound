package app.adarcher.rustysound.tv;

import android.app.Activity;
import android.os.Bundle;
import android.webkit.JavascriptInterface;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;

/**
 * RustySound for Android TV sets that can't run the native app (32-bit
 * ones: Dioxus 0.7 builds 64-bit Android only). Opens the web build full
 * screen and provides the same `window.RustySoundAndroid` bridge as the
 * native shell (android/MainActivity.kt), so the page turns on TV mode,
 * D-pad navigation and Back handling (src/components/ui/android_shell.*).
 */
public class MainActivity extends Activity {
    private WebView web;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        web = new WebView(this);
        WebSettings settings = web.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setMediaPlaybackRequiresUserGesture(false);
        settings.setMixedContentMode(WebSettings.MIXED_CONTENT_ALWAYS_ALLOW);
        web.setWebViewClient(new WebViewClient());
        web.addJavascriptInterface(new Shell(), "RustySoundAndroid");
        web.setFocusable(true);
        web.setFocusableInTouchMode(true);
        setContentView(web);
        if (savedInstanceState == null) {
            web.loadUrl(BuildConfig.WEB_URL);
        } else {
            web.restoreState(savedInstanceState);
        }
        web.requestFocus();
    }

    @Override
    protected void onSaveInstanceState(Bundle outState) {
        super.onSaveInstanceState(outState);
        web.saveState(outState);
    }

    @Override
    public void onBackPressed() {
        // Same contract as the native shell: the page handles Back, or says
        // "none" and the app goes to the background.
        web.evaluateJavascript(
            "(function(){var b=window.__rsAndroidBack;return b?b():'none';})()",
            result -> { if ("\"none\"".equals(result)) moveTaskToBack(true); });
    }

    /** `window.RustySoundAndroid` in the page. */
    private class Shell {
        @JavascriptInterface public boolean isTv() { return true; }
        @JavascriptInterface public String insets() { return "{\"top\":0,\"right\":0,\"bottom\":0,\"left\":0}"; }
        @JavascriptInterface public void setDarkBars(boolean dark) {}
        @JavascriptInterface public void haptic(String kind) {}
        @JavascriptInterface public void leave() { runOnUiThread(() -> moveTaskToBack(true)); }
    }
}
