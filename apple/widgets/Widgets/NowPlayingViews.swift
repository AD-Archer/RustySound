// The widget's views, in the app's "Stage" look: black with a soft lift at
// the top, the cover as the hero, system type, white and grey ink
// (assets/styling/tokens.css, --stage-*).

import AppIntents
import SwiftUI
import WidgetKit

enum Stage {
    static let lift = Color(red: 0x26 / 255, green: 0x28 / 255, blue: 0x2C / 255)
    static let ink = Color.white
    static let muted = Color(red: 0x9A / 255, green: 0x9C / 255, blue: 0xA2 / 255)
    static let faint = Color(red: 0x74 / 255, green: 0x77 / 255, blue: 0x7E / 255)
    static let fill = Color(red: 0xC9 / 255, green: 0xCB / 255, blue: 0xD0 / 255)
    static let line = Color(red: 0x3A / 255, green: 0x3C / 255, blue: 0x41 / 255)
    static let coverBlank = Color(red: 0x22 / 255, green: 0x22 / 255, blue: 0x22 / 255)
    static let control = Color.white.opacity(0.12)
}

/// `--stage-bg`: radial-gradient(120% 90% at 50% 0%, #26282c 0%, #000 62%).
struct StageBackground: View {
    var body: some View {
        EllipticalGradient(
            colors: [Stage.lift, .black],
            center: .top,
            startRadiusFraction: 0,
            endRadiusFraction: 0.62
        )
        .scaleEffect(x: 1.2, y: 1.8, anchor: .top)
        .background(Color.black)
    }
}

struct NowPlayingView: View {
    let entry: NowPlayingEntry
    @Environment(\.widgetFamily) private var family

    var body: some View {
        switch family {
        #if os(iOS)
        case .accessoryRectangular:
            AccessoryRectangular(entry: entry).containerBackground(for: .widget) { Color.clear }
        case .accessoryCircular:
            AccessoryCircular(entry: entry).containerBackground(for: .widget) { AccessoryWidgetBackground() }
        case .accessoryInline:
            AccessoryInline(entry: entry).containerBackground(for: .widget) { Color.clear }
        #endif
        case .systemMedium:
            MediumNowPlaying(entry: entry).containerBackground(for: .widget) { StageBackground() }
        default:
            SmallNowPlaying(entry: entry).containerBackground(for: .widget) { StageBackground() }
        }
    }
}

// MARK: - Home screen and desktop

struct SmallNowPlaying: View {
    let entry: NowPlayingEntry

    var body: some View {
        if let snapshot = entry.snapshot {
            VStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .top, spacing: 0) {
                    CoverArt(image: entry.cover)
                        .frame(width: 66, height: 66)
                    Spacer(minLength: 6)
                    VStack(spacing: 8) {
                        PlayPauseButton(entry: entry, snapshot: snapshot, size: 29)
                        NextButton(size: 29)
                    }
                }
                Spacer(minLength: 6)
                Text(snapshot.title)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Stage.ink)
                    .lineLimit(1)
                Text(snapshot.artist)
                    .font(.system(size: 12))
                    .foregroundStyle(Stage.muted)
                    .lineLimit(1)
                    .padding(.top, 1)
                SongProgress(snapshot: snapshot, date: entry.date)
                    .padding(.top, 8)
            }
        } else {
            NotPlaying(compact: true)
        }
    }
}

struct MediumNowPlaying: View {
    let entry: NowPlayingEntry

    var body: some View {
        HStack(spacing: 14) {
            CoverArt(image: entry.cover)
                .aspectRatio(1, contentMode: .fit)
                .frame(maxHeight: .infinity)
            if let snapshot = entry.snapshot {
                VStack(alignment: .leading, spacing: 0) {
                    Text(snapshot.title)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(Stage.ink)
                        .lineLimit(2)
                    Text(snapshot.artist)
                        .font(.system(size: 13))
                        .foregroundStyle(Stage.muted)
                        .lineLimit(1)
                        .padding(.top, 2)
                    if !snapshot.live {
                        Text(snapshot.album)
                            .font(.system(size: 12))
                            .foregroundStyle(Stage.faint)
                            .lineLimit(1)
                            .padding(.top, 1)
                    }
                    Spacer(minLength: 6)
                    SongProgress(snapshot: snapshot, date: entry.date)
                    HStack(alignment: .center, spacing: 0) {
                        SongTime(snapshot: snapshot, date: entry.date)
                        Spacer(minLength: 8)
                        HStack(spacing: 14) {
                            PlayPauseButton(entry: entry, snapshot: snapshot, size: 30)
                            NextButton(size: 30)
                        }
                    }
                    .padding(.top, 6)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                NotPlaying(compact: false)
            }
        }
    }
}

struct NotPlaying: View {
    let compact: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if compact {
                CoverArt(image: nil).frame(width: 66, height: 66)
                Spacer(minLength: 6)
            }
            Text("Not Playing")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Stage.ink)
            Text("Pick something in RustySound.")
                .font(.system(size: 12))
                .foregroundStyle(Stage.muted)
                .lineLimit(2)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: compact ? .bottomLeading : .leading)
    }
}

/// The cover, or a blank tile with a note.
struct CoverArt: View {
    let image: CGImage?

    var body: some View {
        ZStack {
            if let image {
                Image(decorative: image, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            } else {
                Stage.coverBlank
                Image(systemName: "music.note")
                    .font(.system(size: 20, weight: .medium))
                    .foregroundStyle(Stage.faint)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 4, style: .continuous)
                .strokeBorder(Color.white.opacity(0.06), lineWidth: 0.5)
        )
    }
}

/// A thin bar that moves by itself while playing; LIVE for radio.
struct SongProgress: View {
    let snapshot: NowPlayingSnapshot
    let date: Date

    var body: some View {
        if snapshot.live {
            LiveTag()
        } else if let interval = snapshot.playingInterval {
            ProgressView(timerInterval: interval, countsDown: false) {
                EmptyView()
            } currentValueLabel: {
                EmptyView()
            }
            .progressViewStyle(.linear)
            .tint(Stage.fill)
            .labelsHidden()
        } else {
            ProgressView(value: snapshot.fraction(at: date))
                .progressViewStyle(.linear)
                .tint(Stage.fill)
                .labelsHidden()
        }
    }
}

/// "1:36 / 4:20"; the elapsed part counts up by itself while playing.
struct SongTime: View {
    let snapshot: NowPlayingSnapshot
    let date: Date

    var body: some View {
        Group {
            if snapshot.live || snapshot.duration <= 0 {
                Text(" ")
            } else if let interval = snapshot.playingInterval, abs(snapshot.rate - 1) < 0.01 {
                Text(timerInterval: interval, countsDown: false) + Text(" / " + Self.clock(snapshot.duration))
            } else {
                Text(Self.clock(snapshot.position(at: date)) + " / " + Self.clock(snapshot.duration))
            }
        }
        .font(.system(size: 11).monospacedDigit())
        .foregroundStyle(Stage.faint)
        .lineLimit(1)
    }

    static func clock(_ seconds: Double) -> String {
        let total = Int(max(seconds, 0).rounded(.down))
        return total >= 3600
            ? String(format: "%d:%02d:%02d", total / 3600, total / 60 % 60, total % 60)
            : String(format: "%d:%02d", total / 60, total % 60)
    }
}

struct LiveTag: View {
    var body: some View {
        Text("LIVE")
            .font(.system(size: 10, weight: .semibold))
            .tracking(0.6)
            .foregroundStyle(Stage.muted)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(Stage.line, lineWidth: 1))
    }
}

struct PlayPauseButton: View {
    let entry: NowPlayingEntry
    let snapshot: NowPlayingSnapshot
    let size: CGFloat

    var body: some View {
        if !snapshot.playing && entry.playOpensApp {
            Button(intent: OpenAndPlayIntent()) { glyph }.buttonStyle(.plain)
        } else {
            Button(intent: TogglePlaybackIntent()) { glyph }.buttonStyle(.plain)
        }
    }

    private var glyph: some View {
        Image(systemName: snapshot.playing ? "pause.fill" : "play.fill")
            .font(.system(size: size * 0.42, weight: .semibold))
            .foregroundStyle(Stage.ink)
            .frame(width: size, height: size)
            .background(Circle().fill(Stage.control))
            .accessibilityLabel(snapshot.playing ? "Pause" : "Play")
    }
}

struct NextButton: View {
    let size: CGFloat

    var body: some View {
        Button(intent: NextTrackIntent()) {
            Image(systemName: "forward.fill")
                .font(.system(size: size * 0.4, weight: .semibold))
                .foregroundStyle(Stage.muted)
                .frame(width: size, height: size)
                .contentShape(Rectangle())
                .accessibilityLabel("Next")
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Lock Screen (iPhone)

#if os(iOS)
struct AccessoryRectangular: View {
    let entry: NowPlayingEntry

    var body: some View {
        if let snapshot = entry.snapshot {
            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 4) {
                    Image(systemName: snapshot.playing ? "waveform" : "pause.fill")
                        .font(.system(size: 11, weight: .semibold))
                        .widgetAccentable()
                    Text(snapshot.title)
                        .font(.headline)
                        .lineLimit(1)
                }
                Text(snapshot.artist)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if !snapshot.live {
                    AccessoryProgress(snapshot: snapshot, date: entry.date)
                        .padding(.top, 3)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            Label("Not Playing", systemImage: "music.note")
                .font(.headline)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

struct AccessoryProgress: View {
    let snapshot: NowPlayingSnapshot
    let date: Date

    var body: some View {
        if let interval = snapshot.playingInterval {
            ProgressView(timerInterval: interval, countsDown: false) {
                EmptyView()
            } currentValueLabel: {
                EmptyView()
            }
            .progressViewStyle(.linear)
            .labelsHidden()
        } else {
            ProgressView(value: snapshot.fraction(at: date))
                .progressViewStyle(.linear)
                .labelsHidden()
        }
    }
}

/// A play/pause button ringed by the song's progress.
struct AccessoryCircular: View {
    let entry: NowPlayingEntry

    var body: some View {
        if let snapshot = entry.snapshot {
            if !snapshot.playing && entry.playOpensApp {
                Button(intent: OpenAndPlayIntent()) { ring(snapshot) }.buttonStyle(.plain)
            } else {
                Button(intent: TogglePlaybackIntent()) { ring(snapshot) }.buttonStyle(.plain)
            }
        } else {
            Image(systemName: "music.note")
                .font(.system(size: 20, weight: .medium))
        }
    }

    @ViewBuilder
    private func ring(_ snapshot: NowPlayingSnapshot) -> some View {
        let glyph = Image(systemName: snapshot.playing ? "pause.fill" : "play.fill")
            .font(.system(size: 16, weight: .semibold))
            .accessibilityLabel(snapshot.playing ? "Pause" : "Play")
        if let interval = snapshot.playingInterval {
            ProgressView(timerInterval: interval, countsDown: false) {
                EmptyView()
            } currentValueLabel: {
                glyph
            }
            .progressViewStyle(.circular)
        } else {
            Gauge(value: snapshot.fraction(at: entry.date)) {
                EmptyView()
            } currentValueLabel: {
                glyph
            }
            .gaugeStyle(.accessoryCircularCapacity)
        }
    }
}

struct AccessoryInline: View {
    let entry: NowPlayingEntry

    var body: some View {
        if let snapshot = entry.snapshot {
            Label("\(snapshot.title) — \(snapshot.artist)", systemImage: snapshot.playing ? "music.note" : "pause.fill")
        } else {
            Label("Not Playing", systemImage: "music.note")
        }
    }
}
#endif
