// The widget's buttons. The app is a Rust binary with no App Intents of its
// own, so these run in the widget's process: they leave a command in the App
// Group container and post a Darwin notification, which the app (alive in
// the background while it plays) handles with the lock screen's remote
// command handlers. When the app has been paused long enough to be
// suspended, Play uses `OpenAndPlayIntent` instead, which opens the app.

import AppIntents
import Foundation

struct TogglePlaybackIntent: AudioPlaybackIntent {
    static var title: LocalizedStringResource = "Play or Pause"
    static var description = IntentDescription("Plays or pauses the song in RustySound.")
    static var isDiscoverable = false
    // Media controls work from the Lock Screen without unlocking.
    static var authenticationPolicy: IntentAuthenticationPolicy = .alwaysAllowed

    func perform() async throws -> some IntentResult {
        await AppCommand.toggle.send()
        return .result()
    }
}

struct NextTrackIntent: AudioPlaybackIntent {
    static var title: LocalizedStringResource = "Next Song"
    static var description = IntentDescription("Skips to the next song in RustySound.")
    static var isDiscoverable = false
    static var authenticationPolicy: IntentAuthenticationPolicy = .alwaysAllowed

    func perform() async throws -> some IntentResult {
        await AppCommand.next.send()
        return .result()
    }
}

/// Play for a suspended app: leaves the command and opens the app, which
/// plays as it comes to the foreground.
struct OpenAndPlayIntent: AppIntent {
    static var title: LocalizedStringResource = "Play in RustySound"
    static var description = IntentDescription("Opens RustySound and plays.")
    static var isDiscoverable = false
    static var openAppWhenRun = true

    func perform() async throws -> some IntentResult {
        AppCommand.play.write()
        return .result()
    }
}
