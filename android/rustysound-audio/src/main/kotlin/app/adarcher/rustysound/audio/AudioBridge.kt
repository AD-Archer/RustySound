package app.adarcher.rustysound.audio

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import org.json.JSONObject
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicInteger

/**
 * Rust's handle on native playback (native_android_backend.rs calls these
 * statics over JNI).
 *
 * [apply] takes the JSON commands every native backend takes (load, play,
 * pause, seek, volume, rate, metadata, clear) plus "plan" (the queue around
 * the current song) and "stop_after_current".
 * Commands run on the main thread, where ExoPlayer lives. [snapshot] answers
 * at once from state the player's events keep up to date, with no calls into
 * the player and no polling here.
 *
 * ExoPlayer holds the queue as a playlist, so it moves on to the next song
 * by itself (gapless), and the notification's next/previous work without
 * the Rust UI, which can be suspended in the background. The UI hears about
 * every change through the snapshot: the song id and "gapless:a|b"
 * when a song ended into the next, "play"/"pause" when something other than
 * Rust (the notification, a headset, audio focus) changed play state, and
 * "ended" at the end of the queue or of the song the sleep timer stops on.
 */
object AudioBridge {
    private const val TAG = "RustySoundAudio"
    private const val GAIN = "rustysound.gain"
    private const val DURATION = "rustysound.duration_ms"

    private val main = Handler(Looper.getMainLooper())
    private val actions = ConcurrentLinkedQueue<String>()
    private val pendingLoads = AtomicInteger(0)

    @Volatile private var appContext: Context? = null
    @Volatile private var songId: String? = null
    @Volatile private var timing = Timing()

    // Main thread only.
    private var exo: ExoPlayer? = null
    private var plan = Plan(emptyList(), 0, false)
    private var rustWantsPlaying = false
    private var userVolume = 1f
    private var lastItemId: String? = null

    private class Timing(
        val positionMs: Long = 0,
        val at: Long = 0,
        val playing: Boolean = false,
        val speed: Float = 1f,
        val durationMs: Long = 0,
        val playWhenReady: Boolean = false,
        val state: Int = Player.STATE_IDLE,
        val hasMedia: Boolean = false,
    )

    private class Track(
        val songId: String,
        val src: String,
        val meta: JSONObject?,
        val gain: Float,
    ) {
        fun mediaItem(): MediaItem {
            val extras = Bundle().apply {
                putFloat(GAIN, gain)
                putLong(DURATION, ((meta?.optDouble("duration", 0.0) ?: 0.0) * 1000).toLong())
            }
            val metadata = MediaMetadata.Builder()
                .setTitle(meta?.optString("title"))
                .setArtist(meta?.optString("artist"))
                .setAlbumTitle(meta?.optString("album"))
                .setArtworkUri(meta?.optString("artwork")?.takeIf { it.isNotEmpty() && it != "null" }?.let(Uri::parse))
                .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
                .setIsPlayable(true)
                .setIsBrowsable(false)
                .setExtras(extras)
                .build()
            return MediaItem.Builder()
                .setMediaId(songId)
                .setUri(src)
                .setMediaMetadata(metadata)
                .build()
        }
    }

    /** The songs ExoPlayer may play in order, and which one is current. */
    private class Plan(val tracks: List<Track>, val index: Int, val repeatAll: Boolean) {
        fun indexOf(id: String): Int {
            if (tracks.getOrNull(index)?.songId == id) return index
            return tracks.indexOfFirst { it.songId == id }
        }
    }

    @JvmStatic
    fun init(context: Context) {
        if (appContext == null) appContext = context.applicationContext
    }

    @JvmStatic
    fun apply(json: String) {
        val cmd = try {
            JSONObject(json)
        } catch (e: Exception) {
            return
        }
        val type = cmd.optString("type")
        // The song id changes as soon as Rust asks, so its next snapshot
        // doesn't report the old song and pull the UI back.
        when (type) {
            "load" -> {
                pendingLoads.incrementAndGet()
                songId = cmd.optString("song_id").ifEmpty { null }
            }
            "clear" -> {
                songId = null
                actions.clear()
            }
        }
        main.post {
            try {
                applyOnMain(type, cmd)
            } catch (e: Exception) {
                Log.w(TAG, "command $type failed", e)
            } finally {
                if (type == "load") pendingLoads.decrementAndGet()
            }
        }
    }

    @JvmStatic
    fun snapshot(): String {
        val t = timing
        var position = t.positionMs
        if (t.playing) position += ((SystemClock.elapsedRealtime() - t.at) * t.speed).toLong()
        if (t.durationMs > 0) position = position.coerceAtMost(t.durationMs)
        val out = JSONObject()
        out.put("current_time", position.coerceAtLeast(0) / 1000.0)
        out.put("duration", t.durationMs.coerceAtLeast(0) / 1000.0)
        out.put("paused", !t.playWhenReady || t.state == Player.STATE_IDLE || t.state == Player.STATE_ENDED)
        out.put("ended", t.hasMedia && t.state == Player.STATE_ENDED)
        actions.poll()?.let { out.put("action", it) }
        songId?.let { out.put("song_id", it) }
        return out.toString()
    }

    /** The player, created on first use. Main thread only. */
    fun player(context: Context? = null): ExoPlayer {
        exo?.let { return it }
        val ctx = appContext ?: context!!.applicationContext.also { appContext = it }
        val attributes = AudioAttributes.Builder()
            .setUsage(C.USAGE_MEDIA)
            .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
            .build()
        val player = ExoPlayer.Builder(ctx)
            // Pauses for calls and other apps, ducks for notifications.
            .setAudioAttributes(attributes, true)
            // Pauses when headphones are unplugged.
            .setHandleAudioBecomingNoisy(true)
            .setWakeMode(C.WAKE_MODE_NETWORK)
            .build()
        player.addListener(listener)
        exo = player
        return player
    }

    private fun applyOnMain(type: String, cmd: JSONObject) {
        val p = player()
        when (type) {
            "plan" -> {
                plan = parsePlan(cmd)
                syncPlaylist(p)
            }
            "load" -> load(p, cmd)
            "play" -> {
                rustWantsPlaying = true
                ensureService()
                when (p.playbackState) {
                    Player.STATE_IDLE -> if (p.mediaItemCount > 0) p.prepare()
                    Player.STATE_ENDED -> p.seekToDefaultPosition(p.currentMediaItemIndex)
                }
                p.play()
            }
            "pause" -> {
                rustWantsPlaying = false
                p.pause()
            }
            "seek" -> p.seekTo(seconds(cmd.optDouble("position", 0.0)))
            "volume" -> setVolume(p, cmd)
            "rate" -> p.setPlaybackSpeed(cmd.optDouble("value", 1.0).toFloat().coerceIn(0.25f, 4f))
            "metadata" -> updateMetadata(p, cmd.optJSONObject("meta"))
            "stop_after_current" -> p.pauseAtEndOfMediaItems = cmd.optBoolean("on")
            "clear" -> {
                rustWantsPlaying = false
                plan = Plan(emptyList(), 0, false)
                p.stop()
                p.clearMediaItems()
            }
        }
        refreshTiming(p)
    }

    private fun load(p: ExoPlayer, cmd: JSONObject) {
        val id = cmd.optString("song_id")
        val src = cmd.optString("src")
        if (id.isEmpty() || src.isEmpty()) return
        val play = cmd.optBoolean("play")
        val position = seconds(cmd.optDouble("position", 0.0))
        val user = cmd.optDouble("user", 1.0).toFloat()
        val value = cmd.optDouble("volume", user.toDouble()).toFloat()
        val track = Track(id, src, cmd.optJSONObject("meta"), if (user > 0f) value / user else 1f)
        val current = p.currentMediaItem
        val planIndex = plan.indexOf(id)
        when {
            current?.mediaId == id &&
                current.localConfiguration?.uri?.toString() == src &&
                p.playbackState != Player.STATE_IDLE -> p.seekTo(position)
            planIndex >= 0 && playlistMatchesPlan(p) -> p.seekTo(planIndex, position)
            planIndex >= 0 -> {
                val items = plan.tracks.mapIndexed { i, t -> if (i == planIndex) track.mediaItem() else t.mediaItem() }
                p.setMediaItems(items, planIndex, position)
            }
            else -> p.setMediaItem(track.mediaItem(), position)
        }
        p.repeatMode = if (planIndex >= 0 && plan.repeatAll) Player.REPEAT_MODE_ALL else Player.REPEAT_MODE_OFF
        p.prepare()
        songId = id
        userVolume = user
        p.volume = value.coerceIn(0f, 1f)
        rustWantsPlaying = play
        p.playWhenReady = play
        if (play) ensureService()
    }

    /**
     * Rebuilds the playlist around the playing song to match the plan,
     * without touching the playing song itself (no gap, no reload).
     */
    private fun syncPlaylist(p: ExoPlayer) {
        val current = p.currentMediaItem ?: return
        if (plan.tracks.isEmpty()) {
            // Only this song (repeat one, radio, a song outside the queue).
            trimToCurrent(p)
            p.repeatMode = Player.REPEAT_MODE_OFF
            return
        }
        // Until the load that follows makes the plan's song current.
        if (plan.tracks.getOrNull(plan.index)?.songId != current.mediaId) return
        val repeat = if (plan.repeatAll) Player.REPEAT_MODE_ALL else Player.REPEAT_MODE_OFF
        if (p.currentMediaItemIndex == plan.index && playlistMatchesPlan(p)) {
            p.repeatMode = repeat
            return
        }
        trimToCurrent(p)
        val after = plan.tracks.subList(plan.index + 1, plan.tracks.size).map { it.mediaItem() }
        val before = plan.tracks.subList(0, plan.index).map { it.mediaItem() }
        if (after.isNotEmpty()) p.addMediaItems(1, after)
        if (before.isNotEmpty()) p.addMediaItems(0, before)
        p.repeatMode = repeat
    }

    private fun trimToCurrent(p: ExoPlayer) {
        val index = p.currentMediaItemIndex
        if (index + 1 < p.mediaItemCount) p.removeMediaItems(index + 1, p.mediaItemCount)
        if (index > 0) p.removeMediaItems(0, index)
    }

    private fun playlistMatchesPlan(p: ExoPlayer): Boolean {
        if (p.mediaItemCount != plan.tracks.size) return false
        for (i in plan.tracks.indices) {
            val item = p.getMediaItemAt(i)
            val track = plan.tracks[i]
            if (item.mediaId != track.songId || item.localConfiguration?.uri?.toString() != track.src) return false
        }
        return true
    }

    private fun parsePlan(cmd: JSONObject): Plan {
        val raw = cmd.optJSONArray("items") ?: return Plan(emptyList(), 0, false)
        val tracks = ArrayList<Track>(raw.length())
        for (i in 0 until raw.length()) {
            val item = raw.optJSONObject(i) ?: continue
            val id = item.optString("song_id")
            val src = item.optString("src")
            if (id.isEmpty() || src.isEmpty() || src == "null") continue
            tracks.add(Track(id, src, item.optJSONObject("meta"), item.optDouble("gain", 1.0).toFloat()))
        }
        val index = cmd.optInt("index", 0).coerceIn(0, (tracks.size - 1).coerceAtLeast(0))
        return Plan(tracks, index, cmd.optBoolean("repeat_all"))
    }

    /** New lock-screen text for the playing song, only when it changed. */
    private fun updateMetadata(p: ExoPlayer, meta: JSONObject?) {
        val current = p.currentMediaItem ?: return
        meta ?: return
        val old = current.mediaMetadata
        val artwork = meta.optString("artwork").takeIf { it.isNotEmpty() && it != "null" }
        if (old.title?.toString() == meta.optString("title") &&
            old.artist?.toString() == meta.optString("artist") &&
            old.albumTitle?.toString() == meta.optString("album") &&
            old.artworkUri?.toString() == artwork
        ) {
            return
        }
        val src = current.localConfiguration?.uri?.toString() ?: return
        val gain = old.extras?.getFloat(GAIN, 1f) ?: 1f
        p.replaceMediaItem(p.currentMediaItemIndex, Track(current.mediaId, src, meta, gain).mediaItem())
    }

    private fun setVolume(p: ExoPlayer, cmd: JSONObject) {
        val value = cmd.optDouble("value", 1.0).toFloat()
        userVolume = cmd.optDouble("user", value.toDouble()).toFloat()
        p.volume = value.coerceIn(0f, 1f)
    }

    private fun ensureService() {
        if (PlaybackService.running) return
        val ctx = appContext ?: return
        try {
            ctx.startService(Intent(ctx, PlaybackService::class.java))
        } catch (e: IllegalStateException) {
            // Not allowed from the background; the next foreground play starts it.
            Log.w(TAG, "could not start the playback service", e)
        }
    }

    private fun seconds(value: Double): Long = (value.coerceAtLeast(0.0) * 1000).toLong()

    private fun refreshTiming(p: Player) {
        val known = p.duration
        val duration = if (known != C.TIME_UNSET && known > 0) {
            known
        } else {
            p.currentMediaItem?.mediaMetadata?.extras?.getLong(DURATION, 0L) ?: 0L
        }
        timing = Timing(
            positionMs = p.currentPosition,
            at = SystemClock.elapsedRealtime(),
            playing = p.isPlaying,
            speed = p.playbackParameters.speed,
            durationMs = duration,
            playWhenReady = p.playWhenReady,
            state = p.playbackState,
            hasMedia = p.mediaItemCount > 0,
        )
    }

    private val listener = object : Player.Listener {
        override fun onMediaItemTransition(item: MediaItem?, reason: Int) {
            val previous = lastItemId
            lastItemId = item?.mediaId
            if (pendingLoads.get() == 0) songId = item?.mediaId
            if (reason == Player.MEDIA_ITEM_TRANSITION_REASON_AUTO && previous != null && item != null) {
                actions.add("gapless:$previous|${item.mediaId}")
            }
            // Levelling: each song has its own gain (a load sets its own after this).
            val gain = item?.mediaMetadata?.extras?.getFloat(GAIN, 1f) ?: 1f
            exo?.volume = (userVolume * gain).coerceIn(0f, 1f)
        }

        override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
            if (reason == Player.PLAY_WHEN_READY_CHANGE_REASON_END_OF_MEDIA_ITEM) {
                // The sleep timer's "End of Track": paused at the end of this song.
                exo?.pauseAtEndOfMediaItems = false
                rustWantsPlaying = false
                actions.add("ended")
            } else if (playWhenReady != rustWantsPlaying) {
                rustWantsPlaying = playWhenReady
                actions.add(if (playWhenReady) "play" else "pause")
            }
        }

        override fun onPlayerError(error: PlaybackException) {
            Log.w(TAG, "playback error ${error.errorCodeName}", error)
        }

        override fun onEvents(player: Player, events: Player.Events) {
            refreshTiming(player)
        }
    }
}
