// The App Group container the app and the widget share, and the names in it.
// Compiled into the widget extension and into the bridge framework the app
// loads; the Rust side (src/now_playing_widget.rs) uses the same names.

import Foundation

enum SharedContainer {
    /// Written by the app: the song and play state.
    static let snapshotFile = "now-playing.json"
    /// Written by the widget's buttons, read (and removed) by the app.
    static let commandFile = "command.json"
    /// Darwin notification the widget posts after writing a command.
    static let commandNotification = "app.adarcher.rustysound.widget.command"

    /// The App Group id, from `RSAppGroup` in the running bundle's Info.plist
    /// (the bundle script sets it in the app and in the widget).
    static var group: String? {
        guard let group = Bundle.main.object(forInfoDictionaryKey: "RSAppGroup") as? String,
              !group.isEmpty
        else { return nil }
        return group
    }

    /// The container, or nil when the App Group entitlement is missing.
    static var url: URL? {
        guard let group else { return nil }
        return FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)
    }

    static func file(_ name: String) -> URL? {
        url?.appendingPathComponent(name, isDirectory: false)
    }
}
