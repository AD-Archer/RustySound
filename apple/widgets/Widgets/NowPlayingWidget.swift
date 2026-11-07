// The Now Playing widget: its timeline and the widget bundle.

import SwiftUI
import WidgetKit

struct NowPlayingEntry: TimelineEntry {
    let date: Date
    let snapshot: NowPlayingSnapshot?
    let cover: CGImage?
    /// Play opens the app instead of messaging it (it has probably been
    /// suspended and would not hear the notification).
    var playOpensApp = false

    static let empty = NowPlayingEntry(date: Date(), snapshot: nil, cover: nil)

    /// The gallery's sample.
    static let preview = NowPlayingEntry(
        date: Date(),
        snapshot: NowPlayingSnapshot(
            version: NowPlayingSnapshot.supportedVersion,
            title: "A Case of You",
            artist: "Joni Mitchell",
            album: "Blue (1971)",
            cover: nil,
            duration: 260,
            position: 96,
            playing: true,
            rate: 1,
            live: false,
            updatedAt: Date().timeIntervalSince1970
        ),
        cover: nil
    )
}

struct NowPlayingProvider: TimelineProvider {
    /// A paused iPhone app is suspended within seconds; past this, Play opens
    /// it. Mac apps keep running, so there Play always messages the app.
    #if os(iOS)
    static let suspendedAfter: TimeInterval? = 20
    #else
    static let suspendedAfter: TimeInterval? = nil
    #endif
    /// Grace after a song's end before showing it stopped, so a quick song
    /// change from the app doesn't flash.
    static let endGrace: TimeInterval = 5

    func placeholder(in context: Context) -> NowPlayingEntry { .preview }

    func getSnapshot(in context: Context, completion: @escaping (NowPlayingEntry) -> Void) {
        let entries = makeEntries(family: context.family, now: Date())
        if context.isPreview, entries.first?.snapshot == nil {
            completion(.preview)
        } else {
            completion(entries.first ?? .empty)
        }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<NowPlayingEntry>) -> Void) {
        // The app reloads the widget on every change; no polling.
        completion(Timeline(entries: makeEntries(family: context.family, now: Date()), policy: .never))
    }

    private func makeEntries(family: WidgetFamily, now: Date) -> [NowPlayingEntry] {
        guard let snapshot = NowPlayingSnapshot.load() else { return [.empty] }
        let cover = snapshot.loadCover(maxPixels: Self.coverPixels(for: family))
        var entries = [NowPlayingEntry(date: now, snapshot: snapshot, cover: cover)]
        if let interval = snapshot.playingInterval {
            // The song runs out with no word from the app (it was closed).
            let end = interval.upperBound.addingTimeInterval(Self.endGrace)
            if end > now {
                entries.append(NowPlayingEntry(
                    date: end,
                    snapshot: snapshot.stoppedAtEnd(),
                    cover: cover,
                    playOpensApp: Self.suspendedAfter != nil
                ))
            }
        } else if !snapshot.playing, let after = Self.suspendedAfter {
            let suspended = snapshot.updated.addingTimeInterval(after)
            if suspended <= now {
                entries[0].playOpensApp = true
            } else {
                entries.append(NowPlayingEntry(date: suspended, snapshot: snapshot, cover: cover, playOpensApp: true))
            }
        }
        return entries
    }

    private static func coverPixels(for family: WidgetFamily) -> Int {
        switch family {
        case .systemSmall: return 220
        case .systemMedium: return 420
        default: return 160
        }
    }
}

struct NowPlayingWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "NowPlaying", provider: NowPlayingProvider()) { entry in
            NowPlayingView(entry: entry)
        }
        .configurationDisplayName("Now Playing")
        .description("The song playing in RustySound, with Play/Pause and Next.")
        .supportedFamilies(Self.families)
    }

    #if os(iOS)
    static let families: [WidgetFamily] = [
        .systemSmall, .systemMedium, .accessoryRectangular, .accessoryCircular, .accessoryInline,
    ]
    #else
    static let families: [WidgetFamily] = [.systemSmall, .systemMedium]
    #endif
}

@main
struct RustySoundWidgets: WidgetBundle {
    var body: some Widget {
        NowPlayingWidget()
    }
}
