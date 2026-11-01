//! "Share Link…" for songs, albums and playlists: one helper for every menu.
//! It asks the server for a share (`createShare`, description = the title),
//! then hands the link on the platform's way and says "Link copied":
//!
//! * Web and other desktops: the clipboard. Safari only lets a page write
//!   the clipboard inside the click, so the write starts there with a
//!   promised value (`ClipboardItem`) that the link fills in once the server
//!   answers; `writeText` and a hidden text field are the fallbacks.
//! * Mac app: the system pasteboard (`pbcopy`), which needs no gesture.
//! * iPhone: the share sheet (`navigator.share`, as the lyrics Shot uses);
//!   where the sheet can't open, the pasteboard.
//!
//! Servers with sharing turned off are remembered for the session, and the
//! menus stop offering the item (`can_share`).

use crate::api::{known_share_support, Capabilities, NavidromeClient, ServerConfig};
use crate::components::ui::StatusToast;
#[allow(unused_imports)]
use dioxus::prelude::*;

/// What to share.
#[derive(Clone, PartialEq, Debug)]
pub struct ShareTarget {
    pub server_id: String,
    /// Song, album or playlist id.
    pub id: String,
    /// The share's description: the song, album or playlist title.
    pub title: String,
}

/// Whether menus offer "Share Link…" for an item on `server_id`: online, a
/// configured server that has share links (not Jellyfin, not music on the
/// device), and not known to have sharing off.
pub fn can_share(servers: &[ServerConfig], server_id: &str, offline: bool) -> bool {
    !offline
        && Capabilities::for_source(servers, server_id).sharing
        && known_share_support(server_id) != Some(false)
}

/// Creates a share link for `target` and copies (or shares) it. Call from
/// the click itself: the web clipboard needs the gesture.
pub fn share_link(servers: &[ServerConfig], toast: StatusToast, target: ShareTarget) {
    let Some(server) = servers.iter().find(|s| s.id == target.server_id).cloned() else {
        toast.show("That server isn't connected.", true);
        return;
    };
    begin_copy();
    toast.spawn(async move {
        let client = NavidromeClient::new(server);
        if !client.shares_supported().await {
            end_copy(None).await;
            toast.show("Sharing is turned off on this server.", true);
            return;
        }
        let url = match client.create_share(&target.id, Some(&target.title)).await {
            Ok(url) => url,
            Err(_) => {
                end_copy(None).await;
                toast.show("Couldn't create a share link.", true);
                return;
            }
        };
        match deliver(&url, &target.title).await {
            Delivered::Copied => toast.show("Link copied", false),
            Delivered::Shared => {}
            Delivered::Failed => toast.show("Couldn't copy the link.", true),
        }
    });
}

enum Delivered {
    Copied,
    /// The share sheet took it (or was dismissed): nothing to say.
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    Shared,
    Failed,
}

// ---- Web and other desktops: the page's clipboard ----------------------

/// Starts a clipboard write inside the click, for a value that comes later.
#[cfg(not(any(target_os = "ios", all(feature = "desktop", target_os = "macos"))))]
const BEGIN_COPY_JS: &str = r#"
let resolve;
const text = new Promise((r) => { resolve = r; });
let wrote = null;
try {
  if (navigator.clipboard && navigator.clipboard.write && typeof ClipboardItem === "function") {
    const blob = text.then((t) => {
      if (!t) throw new Error("no link");
      return new Blob([t], { type: "text/plain" });
    });
    wrote = navigator.clipboard.write([new ClipboardItem({ "text/plain": blob })]);
    wrote.catch(() => {});
  }
} catch (_) {
  wrote = null;
}
window.__rsShareCopy = { resolve, wrote };
return true;
"#;

/// Fills in the link (`null`: nothing to copy) and reports whether it got
/// onto the clipboard.
#[cfg(not(any(target_os = "ios", all(feature = "desktop", target_os = "macos"))))]
const END_COPY_JS: &str = r#"
const pending = window.__rsShareCopy;
window.__rsShareCopy = null;
if (pending) pending.resolve(url || "");
if (!url) return false;
if (pending && pending.wrote) {
  try { await pending.wrote; return true; } catch (_) {}
}
try {
  await navigator.clipboard.writeText(url);
  return true;
} catch (_) {}
const field = document.createElement("textarea");
field.value = url;
field.setAttribute("readonly", "");
field.style.position = "fixed";
field.style.opacity = "0";
document.body.appendChild(field);
field.select();
let ok = false;
try { ok = document.execCommand("copy"); } catch (_) {}
field.remove();
return ok;
"#;

#[cfg(not(any(target_os = "ios", all(feature = "desktop", target_os = "macos"))))]
fn begin_copy() {
    let _ = document::eval(BEGIN_COPY_JS);
}

#[cfg(not(any(target_os = "ios", all(feature = "desktop", target_os = "macos"))))]
async fn end_copy(url: Option<&str>) -> bool {
    let url = serde_json::to_string(&url).unwrap_or_else(|_| "null".to_string());
    document::eval(&format!("const url = {url};\n{END_COPY_JS}"))
        .join::<bool>()
        .await
        .unwrap_or(false)
}

#[cfg(not(any(target_os = "ios", all(feature = "desktop", target_os = "macos"))))]
async fn deliver(url: &str, _title: &str) -> Delivered {
    if end_copy(Some(url)).await {
        Delivered::Copied
    } else {
        Delivered::Failed
    }
}

// ---- Mac app: the system pasteboard ------------------------------------

#[cfg(all(feature = "desktop", target_os = "macos"))]
fn begin_copy() {}

#[cfg(all(feature = "desktop", target_os = "macos"))]
async fn end_copy(_: Option<&str>) {}

#[cfg(all(feature = "desktop", target_os = "macos"))]
async fn deliver(url: &str, _title: &str) -> Delivered {
    use std::io::Write;
    let url = url.to_string();
    let copied = tokio::task::spawn_blocking(move || {
        let mut child = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .ok()?;
        child.stdin.take()?.write_all(url.as_bytes()).ok()?;
        child.wait().ok().filter(|status| status.success())
    })
    .await
    .ok()
    .flatten()
    .is_some();
    if copied {
        Delivered::Copied
    } else {
        Delivered::Failed
    }
}

// ---- iPhone: the share sheet, else the pasteboard ----------------------

#[cfg(target_os = "ios")]
fn begin_copy() {}

#[cfg(target_os = "ios")]
async fn end_copy(_: Option<&str>) {}

#[cfg(target_os = "ios")]
async fn deliver(url: &str, title: &str) -> Delivered {
    let data = serde_json::json!({ "url": url, "title": title }).to_string();
    let script = format!(
        r#"
const data = {data};
if (typeof navigator.share !== "function") return "none";
try {{
  await navigator.share(data);
  return "shared";
}} catch (err) {{
  return err && err.name === "AbortError" ? "shared" : "none";
}}
"#
    );
    let outcome = document::eval(&script).join::<String>().await.unwrap_or_default();
    if outcome == "shared" {
        return Delivered::Shared;
    }
    if ios_pasteboard::copy(url) {
        Delivered::Copied
    } else {
        Delivered::Failed
    }
}

#[cfg(target_os = "ios")]
#[allow(unexpected_cfgs)] // objc 0.2 macros probe `cfg(feature = "cargo-clippy")`.
mod ios_pasteboard {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CString;

    /// `UIPasteboard.generalPasteboard.string = text`.
    pub(super) fn copy(text: &str) -> bool {
        let Ok(text) = CString::new(text) else {
            return false;
        };
        unsafe {
            let string: *mut Object = msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()];
            let board: *mut Object = msg_send![class!(UIPasteboard), generalPasteboard];
            if string.is_null() || board.is_null() {
                return false;
            }
            let _: () = msg_send![board, setString: string];
        }
        true
    }
}
