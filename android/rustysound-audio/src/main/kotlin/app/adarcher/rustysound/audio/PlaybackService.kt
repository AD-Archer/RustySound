package app.adarcher.rustysound.audio

import android.app.PendingIntent
import android.content.Intent
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService

/**
 * Keeps playback alive while the app is in the background. Media3 shows the
 * media notification (artwork, title, play/pause, next/previous, seek bar),
 * lock-screen and Bluetooth controls from the session, and makes this a
 * foreground service while the player plays.
 *
 * The player itself belongs to [AudioBridge], so Rust can drive it before
 * the service exists and after it stops.
 */
class PlaybackService : MediaSessionService() {
    private var session: MediaSession? = null

    override fun onCreate() {
        super.onCreate()
        running = true
        val launch = packageManager.getLaunchIntentForPackage(packageName)?.apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_NEW_TASK
        }
        val builder = MediaSession.Builder(this, AudioBridge.player(this))
        if (launch != null) {
            builder.setSessionActivity(
                PendingIntent.getActivity(
                    this,
                    0,
                    launch,
                    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
        }
        session = builder.build().also { addSession(it) }
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    /**
     * Not sticky: the service shares the app's process, which ends when the
     * activity is destroyed (tao exits), so a restarted service would have
     * nothing to play.
     */
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        super.onStartCommand(intent, flags, startId)
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        running = false
        session?.release()
        session = null
        super.onDestroy()
    }

    companion object {
        @Volatile
        var running = false
            private set
    }
}
