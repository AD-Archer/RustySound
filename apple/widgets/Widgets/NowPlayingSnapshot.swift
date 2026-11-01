// The app's snapshot (now-playing.json) and the commands the buttons send.

import Foundation
import ImageIO

/// What the app last wrote. `position` is as of `updatedAt`; while playing,
/// the widget moves it on by itself, so the app writes only on changes.
struct NowPlayingSnapshot: Decodable, Equatable {
    var version: Int
    var title: String
    var artist: String
    var album: String
    /// File name of the cover in the container.
    var cover: String?
    var duration: Double
    var position: Double
    var playing: Bool
    var rate: Double
    var live: Bool
    /// Unix seconds.
    var updatedAt: Double

    enum CodingKeys: String, CodingKey {
        case version, title, artist, album, cover, duration, position, playing, rate, live
        case updatedAt = "updated_at"
    }

    static let supportedVersion = 1

    var updated: Date { Date(timeIntervalSince1970: updatedAt) }

    /// Seconds into the song at `date`.
    func position(at date: Date) -> Double {
        guard playing else { return position }
        let moved = position + max(0, date.timeIntervalSince(updated)) * max(rate, 0)
        return duration > 0 ? min(moved, duration) : moved
    }

    /// The song's span on the clock while it plays, for the self-updating
    /// progress views. Nil when paused, live or of unknown length.
    var playingInterval: ClosedRange<Date>? {
        guard playing, !live, duration > 0, rate > 0 else { return nil }
        let start = updated.addingTimeInterval(-position / rate)
        return start...start.addingTimeInterval(duration / rate)
    }

    /// Progress from 0 to 1 at `date`.
    func fraction(at date: Date) -> Double {
        duration > 0 ? min(max(position(at: date) / duration, 0), 1) : 0
    }

    /// The same song, stopped at its end (the app went quiet mid-song).
    func stoppedAtEnd() -> NowPlayingSnapshot {
        var copy = self
        copy.playing = false
        copy.position = duration
        return copy
    }

    static func load() -> NowPlayingSnapshot? {
        guard let url = SharedContainer.file(SharedContainer.snapshotFile),
              let data = try? Data(contentsOf: url),
              let snapshot = try? JSONDecoder().decode(NowPlayingSnapshot.self, from: data),
              snapshot.version == supportedVersion
        else { return nil }
        return snapshot
    }

    /// When the app last wrote the snapshot (nil: none yet).
    static func stamp() -> Date? {
        guard let path = SharedContainer.file(SharedContainer.snapshotFile)?.path else { return nil }
        return (try? FileManager.default.attributesOfItem(atPath: path))?[.modificationDate] as? Date
    }

    /// The cover, decoded at most `maxPixels` on its long side (WidgetKit
    /// drops widgets whose images are too large).
    func loadCover(maxPixels: Int) -> CGImage? {
        guard let cover, let url = SharedContainer.file(cover),
              let source = CGImageSourceCreateWithURL(url as CFURL, nil)
        else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixels,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }
}

/// A button press for the app (src/now_playing_widget.rs reads it).
enum AppCommand: String {
    case toggle, play, next

    /// Leaves the command in the container.
    func write() {
        guard let url = SharedContainer.file(SharedContainer.commandFile) else { return }
        let command: [String: Any] = [
            "action": rawValue,
            "at": Date().timeIntervalSince1970,
            "id": UUID().uuidString,
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: command) else { return }
        try? data.write(to: url, options: .atomic)
    }

    /// Leaves the command, tells the app, and waits up to `timeout` for the
    /// app's new snapshot, so the widget redraws with the real state.
    func send(timeout: TimeInterval = 1.5) async {
        let before = NowPlayingSnapshot.stamp()
        write()
        CFNotificationCenterPostNotification(
            CFNotificationCenterGetDarwinNotifyCenter(),
            CFNotificationName(SharedContainer.commandNotification as CFString),
            nil,
            nil,
            true
        )
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            try? await Task.sleep(nanoseconds: 100_000_000)
            if NowPlayingSnapshot.stamp() != before { return }
        }
    }
}
