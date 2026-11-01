// RustySoundWidgetBridge.framework: the app's side of the Now Playing widgets.
//
// WidgetKit has no Objective-C or C API, so the Rust app (which has no Swift
// of its own) loads this framework with dlopen and calls these C entry
// points. See src/now_playing_widget.rs. The bundle script embeds it only
// when the widget is embedded (RS_WIDGETS=1); without it the app skips all
// widget work.

import Foundation
import WidgetKit

/// The App Group container's path (malloc'd; the caller frees it), or NULL
/// when the app has no App Group entitlement.
@_cdecl("rs_widget_container_path")
public func rsWidgetContainerPath() -> UnsafeMutablePointer<CChar>? {
    guard let url = SharedContainer.url else { return nil }
    try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return strdup(url.path)
}

/// Asks WidgetKit to redraw the widgets from the new snapshot.
@_cdecl("rs_widget_reload")
public func rsWidgetReload() {
    WidgetCenter.shared.reloadAllTimelines()
}

private var commandHandler: (@convention(c) () -> Void)?

/// Calls `handler` on the main thread whenever a widget button posts a
/// command. Call once.
@_cdecl("rs_widget_observe_commands")
public func rsWidgetObserveCommands(_ handler: @escaping @convention(c) () -> Void) {
    commandHandler = handler
    CFNotificationCenterAddObserver(
        CFNotificationCenterGetDarwinNotifyCenter(),
        nil,
        { _, _, _, _, _ in
            guard let handler = commandHandler else { return }
            if Thread.isMainThread {
                handler()
            } else {
                DispatchQueue.main.async { handler() }
            }
        },
        SharedContainer.commandNotification as CFString,
        nil,
        .deliverImmediately
    )
}
