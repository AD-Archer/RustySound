package {{package}}

import android.annotation.SuppressLint
import android.Manifest
import android.app.Activity
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.MediaPlayer
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.net.Uri
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import org.json.JSONArray
import org.json.JSONObject
import java.util.ArrayDeque
import kotlin.math.max

private data class RustySoundTrackMeta(
    val title: String = "",
    val artist: String = "",
    val album: String = "",
    val artwork: String? = null,
    val duration: Double = 0.0,
    val isLive: Boolean = false,
)

private data class RustySoundQueueItem(
    val songId: String,
    val src: String?,
    val meta: RustySoundTrackMeta,
)

class RustySoundAudioService : Service() {
    override fun onCreate() {
        super.onCreate()
        RustySoundAudioBridge.attachService(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        RustySoundAudioBridge.handleServiceIntent(this, intent)
        return START_STICKY
    }

    override fun onDestroy() {
        RustySoundAudioBridge.detachService(this)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}

object RustySoundAudioBridge {
    private const val CHANNEL_ID = "rustysound_playback"
    private const val NOTIFICATION_ID = 4107

    private const val ACTION_START = "{{package}}.RustySoundAudio.START"
    private const val ACTION_PLAY = "{{package}}.RustySoundAudio.PLAY"
    private const val ACTION_PAUSE = "{{package}}.RustySoundAudio.PAUSE"
    private const val ACTION_TOGGLE = "{{package}}.RustySoundAudio.TOGGLE"
    private const val ACTION_NEXT = "{{package}}.RustySoundAudio.NEXT"
    private const val ACTION_PREVIOUS = "{{package}}.RustySoundAudio.PREVIOUS"
    private const val ACTION_STOP = "{{package}}.RustySoundAudio.STOP"

    private val lock = Any()
    private val remoteActions = ArrayDeque<String>()
    private var player: MediaPlayer? = null
    private var mediaSession: MediaSession? = null
    private var activeService: RustySoundAudioService? = null
    private var currentSongId: String? = null
    private var currentSrc: String? = null
    private var metadata: RustySoundTrackMeta? = null
    private var queue = emptyList<RustySoundQueueItem>()
    private var queueIndex = 0
    private var repeatMode = "off"
    private var shuffleEnabled = false
    private var prepared = false
    private var playWhenReady = false
    private var endedFlag = false
    private var pendingSeekMs = 0L
    private var lastKnownPositionMs = 0L
    private var lastKnownDurationMs = 0L
    private var volume = 1.0f

    @JvmStatic
    fun apply(context: Context, raw: String) {
        val command = runCatching { JSONObject(raw) }.getOrNull() ?: return
        ensure(context)

        when (command.optString("type")) {
            "init" -> {
                updateMediaSession(context)
            }
            "plan" -> {
                updatePlan(command)
                updateMediaSession(context)
                updateNotification(context)
            }
            "load" -> {
                val src = command.optNullableString("src") ?: return
                val songId = command.optNullableString("song_id")
                val position = command.optDouble("position", 0.0)
                val shouldPlay = command.optBoolean("play", false)
                val nextVolume = command.optDouble("volume", volume.toDouble()).toFloat()
                    .coerceIn(0.0f, 1.0f)
                val meta = parseMeta(command.optJSONObject("meta"))
                load(context, src, songId, position, nextVolume, shouldPlay, meta)
            }
            "play" -> {
                play(context)
            }
            "pause" -> {
                pause(context)
            }
            "seek" -> {
                seek(command.optDouble("position", 0.0))
                pushRemoteSeekForNativeOnly(command)
                updateMediaSession(context)
                updateNotification(context)
            }
            "volume" -> {
                volume = command.optDouble("value", volume.toDouble()).toFloat()
                    .coerceIn(0.0f, 1.0f)
                player?.setVolume(volume, volume)
            }
            "loop" -> {
                player?.isLooping = command.optBoolean("enabled", false)
            }
            "metadata" -> {
                metadata = parseMeta(command.optJSONObject("meta"))
                updateMediaSession(context)
                updateNotification(context)
            }
            "clear" -> {
                clear(context)
            }
        }
    }

    @JvmStatic
    fun snapshot(context: Context): String {
        ensure(context)
        val result = JSONObject()
        synchronized(lock) {
            samplePositionLocked()
            val action = remoteActions.pollFirst()
            result.put("current_time", lastKnownPositionMs.toDouble() / 1000.0)
            result.put("duration", lastKnownDurationMs.toDouble() / 1000.0)
            result.put("paused", !(player?.isPlaying == true || playWhenReady))
            result.put("ended", endedFlag)
            result.put("action", action ?: JSONObject.NULL)
            result.put("song_id", currentSongId ?: JSONObject.NULL)
            endedFlag = false
        }
        return result.toString()
    }

    internal fun attachService(service: RustySoundAudioService) {
        activeService = service
        ensure(service)
        updateNotification(service)
    }

    internal fun detachService(service: RustySoundAudioService) {
        if (activeService === service) {
            activeService = null
        }
    }

    internal fun handleServiceIntent(service: RustySoundAudioService, intent: Intent?) {
        ensure(service)
        when (intent?.action) {
            ACTION_PLAY -> {
                play(service)
                pushRemoteAction("play")
            }
            ACTION_PAUSE -> {
                pause(service)
                pushRemoteAction("pause")
            }
            ACTION_TOGGLE -> {
                if (player?.isPlaying == true) {
                    pause(service)
                    pushRemoteAction("pause")
                } else {
                    play(service)
                    pushRemoteAction("play")
                }
            }
            ACTION_NEXT -> {
                if (!tryImmediateTransition(service, "next")) {
                    pushRemoteAction("next")
                }
            }
            ACTION_PREVIOUS -> {
                if (!tryImmediateTransition(service, "previous")) {
                    pushRemoteAction("previous")
                }
            }
            ACTION_STOP -> {
                pause(service)
                pushRemoteAction("pause")
            }
            else -> {
                updateNotification(service)
            }
        }
    }

    private fun ensure(context: Context) {
        ensureMediaSession(context.applicationContext)
        createNotificationChannel(context.applicationContext)
    }

    private fun ensureMediaSession(context: Context) {
        if (mediaSession != null) {
            mediaSession?.isActive = true
            return
        }

        mediaSession = MediaSession(context, "RustySound").apply {
            setFlags(
                MediaSession.FLAG_HANDLES_MEDIA_BUTTONS or
                    MediaSession.FLAG_HANDLES_TRANSPORT_CONTROLS
            )
            setCallback(object : MediaSession.Callback() {
                override fun onPlay() {
                    play(context)
                    pushRemoteAction("play")
                }

                override fun onPause() {
                    pause(context)
                    pushRemoteAction("pause")
                }

                override fun onSkipToNext() {
                    if (!tryImmediateTransition(context, "next")) {
                        pushRemoteAction("next")
                    }
                }

                override fun onSkipToPrevious() {
                    if (!tryImmediateTransition(context, "previous")) {
                        pushRemoteAction("previous")
                    }
                }

                override fun onSeekTo(pos: Long) {
                    val seconds = max(0L, pos).toDouble() / 1000.0
                    seek(seconds)
                    pushRemoteAction("seek:$seconds")
                    updateMediaSession(context)
                    updateNotification(context)
                }

                override fun onStop() {
                    pause(context)
                    pushRemoteAction("pause")
                }
            })
            isActive = true
        }
    }

    private fun load(
        context: Context,
        src: String,
        songId: String?,
        positionSeconds: Double,
        nextVolume: Float,
        shouldPlay: Boolean,
        meta: RustySoundTrackMeta,
    ) {
        releasePlayer()
        ensure(context)
        synchronized(lock) {
            currentSrc = src
            currentSongId = songId
            metadata = meta
            volume = nextVolume
            prepared = false
            playWhenReady = shouldPlay
            endedFlag = false
            pendingSeekMs = secondsToMillis(positionSeconds)
            lastKnownPositionMs = pendingSeekMs
            lastKnownDurationMs = secondsToMillis(meta.duration)
            syncQueueIndexLocked(songId)
        }

        val appContext = context.applicationContext
        val nextPlayer = MediaPlayer()
        player = nextPlayer

        try {
            nextPlayer.setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build()
            )
            nextPlayer.setWakeMode(appContext, PowerManager.PARTIAL_WAKE_LOCK)
            nextPlayer.setVolume(volume, volume)
            nextPlayer.setOnPreparedListener { mediaPlayer ->
                synchronized(lock) {
                    prepared = true
                    val duration = safeDuration(mediaPlayer)
                    if (duration > 0L) {
                        lastKnownDurationMs = duration
                    }
                }
                if (pendingSeekMs > 0L) {
                    seekPrepared(mediaPlayer, pendingSeekMs)
                }
                if (playWhenReady) {
                    startPreparedPlayer(appContext)
                }
                updateMediaSession(appContext)
                updateNotification(appContext)
            }
            nextPlayer.setOnCompletionListener {
                synchronized(lock) {
                    endedFlag = true
                    playWhenReady = false
                    samplePositionLocked()
                }
                if (!tryImmediateTransition(appContext, "ended")) {
                    pushRemoteAction("ended")
                    updateMediaSession(appContext)
                    updateNotification(appContext)
                }
            }
            nextPlayer.setOnErrorListener { _, _, _ ->
                synchronized(lock) {
                    playWhenReady = false
                }
                pushRemoteAction("pause")
                updateMediaSession(appContext)
                updateNotification(appContext)
                true
            }

            val uri = Uri.parse(src)
            if (uri.scheme == "file" || uri.scheme == "content") {
                nextPlayer.setDataSource(appContext, uri)
            } else {
                nextPlayer.setDataSource(src)
            }
            nextPlayer.prepareAsync()
            if (shouldPlay) {
                startPlaybackService(context)
            }
            updateMediaSession(context)
            updateNotification(context)
        } catch (_: Exception) {
            releasePlayer()
            synchronized(lock) {
                playWhenReady = false
            }
            pushRemoteAction("pause")
            updateMediaSession(context)
            updateNotification(context)
        }
    }

    private fun play(context: Context) {
        requestNotificationPermission(context)
        synchronized(lock) {
            playWhenReady = true
            endedFlag = false
        }
        if (prepared) {
            startPreparedPlayer(context.applicationContext)
        } else {
            startPlaybackService(context)
        }
        updateMediaSession(context)
        updateNotification(context)
    }

    private fun pause(context: Context) {
        synchronized(lock) {
            playWhenReady = false
            samplePositionLocked()
        }
        runCatching {
            if (player?.isPlaying == true) {
                player?.pause()
            }
        }
        updateMediaSession(context)
        updateNotification(context)
    }

    private fun seek(positionSeconds: Double) {
        val target = secondsToMillis(positionSeconds)
        synchronized(lock) {
            pendingSeekMs = target
            lastKnownPositionMs = target
            endedFlag = false
        }
        val current = player
        if (current != null && prepared) {
            seekPrepared(current, target)
        }
    }

    private fun clear(context: Context) {
        releasePlayer()
        synchronized(lock) {
            currentSongId = null
            currentSrc = null
            metadata = null
            prepared = false
            playWhenReady = false
            endedFlag = false
            pendingSeekMs = 0L
            lastKnownPositionMs = 0L
            lastKnownDurationMs = 0L
            remoteActions.clear()
        }
        mediaSession?.setPlaybackState(
            PlaybackState.Builder()
                .setState(PlaybackState.STATE_STOPPED, 0L, 0.0f)
                .build()
        )
        mediaSession?.setMetadata(null)
        stopPlaybackService(context, true)
    }

    private fun releasePlayer() {
        runCatching {
            player?.setOnPreparedListener(null)
            player?.setOnCompletionListener(null)
            player?.setOnErrorListener(null)
            player?.reset()
            player?.release()
        }
        player = null
        prepared = false
    }

    private fun startPreparedPlayer(context: Context) {
        requestAudioFocus(context)
        runCatching {
            player?.start()
        }
        startPlaybackService(context)
    }

    @Suppress("DEPRECATION")
    private fun requestAudioFocus(context: Context) {
        val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as? AudioManager ?: return
        audioManager.requestAudioFocus(
            null,
            AudioManager.STREAM_MUSIC,
            AudioManager.AUDIOFOCUS_GAIN
        )
    }

    private fun seekPrepared(mediaPlayer: MediaPlayer, targetMs: Long) {
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                mediaPlayer.seekTo(targetMs, MediaPlayer.SEEK_CLOSEST)
            } else {
                @Suppress("DEPRECATION")
                mediaPlayer.seekTo(targetMs.coerceAtMost(Int.MAX_VALUE.toLong()).toInt())
            }
        }
    }

    private fun updatePlan(command: JSONObject) {
        val nextItems = mutableListOf<RustySoundQueueItem>()
        val rawItems = command.optJSONArray("items") ?: JSONArray()
        for (i in 0 until rawItems.length()) {
            val item = rawItems.optJSONObject(i) ?: continue
            val songId = item.optNullableString("song_id") ?: continue
            nextItems.add(
                RustySoundQueueItem(
                    songId = songId,
                    src = item.optNullableString("src"),
                    meta = parseMeta(item.optJSONObject("meta")),
                )
            )
        }
        synchronized(lock) {
            queue = nextItems
            queueIndex = command.optInt("index", queueIndex)
                .coerceIn(0, queue.size.coerceAtLeast(1) - 1)
            repeatMode = command.optString("repeat", repeatMode)
            shuffleEnabled = command.optBoolean("shuffle", shuffleEnabled)
            syncQueueIndexLocked(currentSongId)
        }
    }

    private fun tryImmediateTransition(context: Context, action: String): Boolean {
        val item = synchronized(lock) {
            if (metadata?.isLive == true) {
                return@synchronized null
            }
            val target = transitionIndexLocked(action) ?: return@synchronized null
            queueIndex = target
            queue.getOrNull(target)
        } ?: return false

        val src = item.src ?: return false
        load(context, src, item.songId, 0.0, volume, true, item.meta)
        return true
    }

    private fun transitionIndexLocked(action: String): Int? {
        if (queue.isEmpty()) {
            return null
        }
        syncQueueIndexLocked(currentSongId)
        val current = queueIndex.coerceIn(0, queue.size - 1)
        return when (action) {
            "next", "ended" -> {
                if (current + 1 < queue.size) {
                    current + 1
                } else if (repeatMode == "all") {
                    0
                } else {
                    null
                }
            }
            "previous" -> {
                if (current > 0) {
                    current - 1
                } else if (repeatMode == "all") {
                    queue.size - 1
                } else {
                    null
                }
            }
            else -> null
        }
    }

    private fun syncQueueIndexLocked(songId: String?) {
        if (songId == null) {
            return
        }
        val index = queue.indexOfFirst { it.songId == songId }
        if (index >= 0) {
            queueIndex = index
        }
    }

    private fun updateMediaSession(context: Context) {
        ensureMediaSession(context.applicationContext)
        synchronized(lock) {
            samplePositionLocked()
            val meta = metadata
            val state = when {
                player?.isPlaying == true || playWhenReady -> PlaybackState.STATE_PLAYING
                currentSongId != null -> PlaybackState.STATE_PAUSED
                else -> PlaybackState.STATE_NONE
            }
            val speed = if (state == PlaybackState.STATE_PLAYING) 1.0f else 0.0f
            val actions = PlaybackState.ACTION_PLAY or
                PlaybackState.ACTION_PAUSE or
                PlaybackState.ACTION_PLAY_PAUSE or
                PlaybackState.ACTION_SKIP_TO_NEXT or
                PlaybackState.ACTION_SKIP_TO_PREVIOUS or
                PlaybackState.ACTION_SEEK_TO or
                PlaybackState.ACTION_STOP
            mediaSession?.setPlaybackState(
                PlaybackState.Builder()
                    .setActions(actions)
                    .setState(state, lastKnownPositionMs, speed)
                    .build()
            )

            if (meta == null) {
                mediaSession?.setMetadata(null)
            } else {
                val builder = MediaMetadata.Builder()
                    .putString(MediaMetadata.METADATA_KEY_TITLE, meta.title)
                    .putString(MediaMetadata.METADATA_KEY_ARTIST, meta.artist)
                    .putString(MediaMetadata.METADATA_KEY_ALBUM, meta.album)
                if (!meta.isLive && lastKnownDurationMs > 0L) {
                    builder.putLong(MediaMetadata.METADATA_KEY_DURATION, lastKnownDurationMs)
                }
                if (!meta.artwork.isNullOrBlank()) {
                    builder.putString(MediaMetadata.METADATA_KEY_DISPLAY_ICON_URI, meta.artwork)
                }
                mediaSession?.setMetadata(builder.build())
            }
            mediaSession?.isActive = currentSongId != null
        }
    }

    private fun updateNotification(context: Context) {
        if (currentSongId == null) {
            return
        }
        val service = activeService
        if (service != null) {
            promoteForeground(service)
            return
        }
        notifyOnly(context.applicationContext)
    }

    private fun promoteForeground(service: RustySoundAudioService) {
        val notification = buildNotification(service)
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                service.startForeground(
                    NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
                )
            } else {
                service.startForeground(NOTIFICATION_ID, notification)
            }
        }.onFailure {
            notifyOnly(service)
        }
    }

    private fun notifyOnly(context: Context) {
        runCatching {
            val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            manager.notify(NOTIFICATION_ID, buildNotification(context))
        }
    }

    private fun buildNotification(context: Context): Notification {
        createNotificationChannel(context)
        val meta = metadata
        val playing = player?.isPlaying == true || playWhenReady
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(context, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(context)
        }

        val playPauseAction = if (playing) {
            Notification.Action(
                android.R.drawable.ic_media_pause,
                "Pause",
                serviceIntent(context, ACTION_PAUSE, 2)
            )
        } else {
            Notification.Action(
                android.R.drawable.ic_media_play,
                "Play",
                serviceIntent(context, ACTION_PLAY, 2)
            )
        }

        builder
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle(meta?.title?.takeIf { it.isNotBlank() } ?: "RustySound")
            .setContentText(meta?.artist?.takeIf { it.isNotBlank() } ?: meta?.album.orEmpty())
            .setOngoing(playing)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .addAction(
                Notification.Action(
                    android.R.drawable.ic_media_previous,
                    "Previous",
                    serviceIntent(context, ACTION_PREVIOUS, 1)
                )
            )
            .addAction(playPauseAction)
            .addAction(
                Notification.Action(
                    android.R.drawable.ic_media_next,
                    "Next",
                    serviceIntent(context, ACTION_NEXT, 3)
                )
            )
            .setDeleteIntent(serviceIntent(context, ACTION_STOP, 4))

        launchIntent(context)?.let { builder.setContentIntent(it) }

        mediaSession?.sessionToken?.let { token ->
            builder.setStyle(
                Notification.MediaStyle()
                    .setMediaSession(token)
                    .setShowActionsInCompactView(0, 1, 2)
            )
        }

        return builder.build()
    }

    private fun launchIntent(context: Context): PendingIntent? {
        val intent = context.packageManager.getLaunchIntentForPackage(context.packageName)
            ?: return null
        return PendingIntent.getActivity(context, 0, intent, pendingIntentFlags())
    }

    private fun serviceIntent(context: Context, action: String, requestCode: Int): PendingIntent {
        val intent = Intent(context, RustySoundAudioService::class.java).setAction(action)
        return PendingIntent.getService(context, requestCode, intent, pendingIntentFlags())
    }

    private fun pendingIntentFlags(): Int {
        val immutable = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            PendingIntent.FLAG_IMMUTABLE
        } else {
            0
        }
        return PendingIntent.FLAG_UPDATE_CURRENT or immutable
    }

    private fun startPlaybackService(context: Context) {
        requestNotificationPermission(context)
        val intent = Intent(context, RustySoundAudioService::class.java).setAction(ACTION_START)
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.applicationContext.startForegroundService(intent)
            } else {
                context.applicationContext.startService(intent)
            }
        }
    }

    private fun requestNotificationPermission(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            return
        }
        val activity = context as? Activity ?: return
        if (activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        activity.requestPermissions(
            arrayOf(Manifest.permission.POST_NOTIFICATIONS),
            NOTIFICATION_ID
        )
    }

    private fun stopPlaybackService(context: Context, removeNotification: Boolean) {
        activeService?.let { service ->
            runCatching {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
                    service.stopForeground(
                        if (removeNotification) {
                            Service.STOP_FOREGROUND_REMOVE
                        } else {
                            Service.STOP_FOREGROUND_DETACH
                        }
                    )
                } else {
                    @Suppress("DEPRECATION")
                    service.stopForeground(removeNotification)
                }
                if (removeNotification) {
                    service.stopSelf()
                }
            }
        }
        if (removeNotification) {
            runCatching {
                val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
                manager.cancel(NOTIFICATION_ID)
            }
        }
    }

    private fun createNotificationChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return
        }
        val manager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (manager.getNotificationChannel(CHANNEL_ID) != null) {
            return
        }
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                "Playback",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                setShowBadge(false)
                lockscreenVisibility = Notification.VISIBILITY_PUBLIC
            }
        )
    }

    private fun samplePositionLocked() {
        val current = player ?: return
        runCatching {
            if (prepared) {
                lastKnownPositionMs = current.currentPosition.toLong().coerceAtLeast(0L)
                val duration = safeDuration(current)
                if (duration > 0L) {
                    lastKnownDurationMs = duration
                }
            }
        }
    }

    private fun safeDuration(mediaPlayer: MediaPlayer): Long {
        return runCatching { mediaPlayer.duration.toLong().coerceAtLeast(0L) }.getOrDefault(0L)
    }

    private fun secondsToMillis(seconds: Double): Long {
        return (seconds.coerceAtLeast(0.0) * 1000.0).toLong()
    }

    private fun pushRemoteAction(action: String) {
        synchronized(lock) {
            when {
                action == "play" || action == "pause" -> {
                    remoteActions.removeAll { it == "play" || it == "pause" }
                }
                action.startsWith("seek:") -> {
                    remoteActions.removeAll { it.startsWith("seek:") }
                }
            }
            remoteActions.add(action)
        }
    }

    private fun pushRemoteSeekForNativeOnly(command: JSONObject) {
        if (command.optBoolean("from_remote", false)) {
            pushRemoteAction("seek:${command.optDouble("position", 0.0).coerceAtLeast(0.0)}")
        }
    }

    private fun parseMeta(value: JSONObject?): RustySoundTrackMeta {
        if (value == null) {
            return RustySoundTrackMeta()
        }
        return RustySoundTrackMeta(
            title = value.optString("title", ""),
            artist = value.optString("artist", ""),
            album = value.optString("album", ""),
            artwork = value.optNullableString("artwork"),
            duration = value.optDouble("duration", 0.0),
            isLive = value.optBoolean("is_live", false),
        )
    }

    private fun JSONObject.optNullableString(name: String): String? {
        if (!has(name) || isNull(name)) {
            return null
        }
        return optString(name, "").takeIf { it.isNotBlank() }
    }
}
