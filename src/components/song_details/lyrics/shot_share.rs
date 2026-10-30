//! Lyrics Shot sharing: the html2canvas capture script, file naming, the
//! shot themes and text palettes, and the status messages. Moved verbatim
//! from the old lyrics panel; only visibility changed.

use crate::api::LyricsResult;
use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
pub(super) struct ScreenshotLyricBar {
    pub text: String,
    pub timestamp_seconds: Option<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ScreenshotShotTheme {
    Lagoon,
    Ember,
    Midnight,
    Cover,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ScreenshotTextPalette {
    Auto,
    Light,
    Dark,
    Gold,
    Cyan,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ScreenshotShareIntent {
    Save,
    Social,
}

pub(super) fn screenshot_lyrics_bars(lyrics: &LyricsResult, sync_lyrics: bool) -> Vec<ScreenshotLyricBar> {
    if sync_lyrics && !lyrics.synced_lines.is_empty() {
        lyrics
            .synced_lines
            .iter()
            .map(|line| ScreenshotLyricBar {
                text: line.text.trim().to_string(),
                timestamp_seconds: Some(line.timestamp_seconds),
            })
            .filter(|line| !line.text.is_empty())
            .collect()
    } else {
        super::plain_lines(lyrics)
            .into_iter()
            .map(|line| ScreenshotLyricBar {
                text: line,
                timestamp_seconds: None,
            })
            .collect()
    }
}

pub(super) fn screenshot_bar_label(bar: &ScreenshotLyricBar, include_timestamp: bool) -> String {
    if include_timestamp {
        if let Some(timestamp_seconds) = bar.timestamp_seconds {
            return format!("{} {}", format_timestamp(timestamp_seconds), bar.text);
        }
    }

    bar.text.clone()
}

pub(super) fn screenshot_share_file_name(song_title: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in song_title.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }

        if slug.len() >= 36 {
            break;
        }
    }

    while slug.ends_with('-') {
        let _ = slug.pop();
    }

    if slug.is_empty() {
        "rustysound-lyrics-shot.png".to_string()
    } else {
        format!("rustysound-{slug}-lyrics-shot.png")
    }
}

pub(super) fn screenshot_share_intent_key(intent: ScreenshotShareIntent) -> &'static str {
    match intent {
        ScreenshotShareIntent::Save => "save",
        ScreenshotShareIntent::Social => "social",
    }
}

#[derive(Clone, Copy)]
pub(super) struct ShotTextColors {
    pub primary: &'static str,
    pub secondary: &'static str,
    pub footer_primary: &'static str,
    pub footer_secondary: &'static str,
    pub lyrics: &'static str,
    pub fallback_cover_bg: &'static str,
    pub fallback_cover_fg: &'static str,
}

pub(super) fn screenshot_shot_text_colors(
    palette: ScreenshotTextPalette,
    auto_prefers_dark_text: bool,
) -> ShotTextColors {
    match palette {
        ScreenshotTextPalette::Auto => {
            if auto_prefers_dark_text {
                ShotTextColors {
                    primary: "#09090b",
                    secondary: "rgba(9,9,11,0.68)",
                    footer_primary: "rgba(9,9,11,0.82)",
                    footer_secondary: "rgba(9,9,11,0.56)",
                    lyrics: "#111827",
                    fallback_cover_bg: "rgba(9,9,11,0.12)",
                    fallback_cover_fg: "rgba(9,9,11,0.8)",
                }
            } else {
                ShotTextColors {
                    primary: "#ffffff",
                    secondary: "rgba(255,255,255,0.72)",
                    footer_primary: "rgba(255,255,255,0.9)",
                    footer_secondary: "rgba(255,255,255,0.6)",
                    lyrics: "#ffffff",
                    fallback_cover_bg: "rgba(255,255,255,0.12)",
                    fallback_cover_fg: "rgba(255,255,255,0.82)",
                }
            }
        }
        ScreenshotTextPalette::Light => ShotTextColors {
            primary: "#ffffff",
            secondary: "rgba(255,255,255,0.74)",
            footer_primary: "rgba(255,255,255,0.9)",
            footer_secondary: "rgba(255,255,255,0.6)",
            lyrics: "#ffffff",
            fallback_cover_bg: "rgba(255,255,255,0.12)",
            fallback_cover_fg: "rgba(255,255,255,0.82)",
        },
        ScreenshotTextPalette::Dark => ShotTextColors {
            primary: "#09090b",
            secondary: "rgba(9,9,11,0.7)",
            footer_primary: "rgba(9,9,11,0.84)",
            footer_secondary: "rgba(9,9,11,0.58)",
            lyrics: "#111827",
            fallback_cover_bg: "rgba(9,9,11,0.12)",
            fallback_cover_fg: "rgba(9,9,11,0.82)",
        },
        ScreenshotTextPalette::Gold => ShotTextColors {
            primary: "#fef3c7",
            secondary: "rgba(253,230,138,0.86)",
            footer_primary: "rgba(253,230,138,0.96)",
            footer_secondary: "rgba(253,230,138,0.72)",
            lyrics: "#fff7d6",
            fallback_cover_bg: "rgba(253,230,138,0.18)",
            fallback_cover_fg: "rgba(253,230,138,0.92)",
        },
        ScreenshotTextPalette::Cyan => ShotTextColors {
            primary: "#ecfeff",
            secondary: "rgba(165,243,252,0.88)",
            footer_primary: "rgba(165,243,252,0.95)",
            footer_secondary: "rgba(165,243,252,0.7)",
            lyrics: "#e0fbff",
            fallback_cover_bg: "rgba(125,211,252,0.22)",
            fallback_cover_fg: "rgba(224,251,255,0.94)",
        },
    }
}


pub(super) fn screenshot_share_status_message(status: &str, intent: ScreenshotShareIntent) -> String {
    match status {
        "shared-image" => match intent {
            ScreenshotShareIntent::Save => {
                "Opened share sheet. Choose \"Save Image\" to store it on your device."
                    .to_string()
            }
            ScreenshotShareIntent::Social => {
                "Opened share sheet with an Social-ready image.".to_string()
            }
        },
        "saved-image" => "Saved the shot image locally. Share it from Photos/Files.".to_string(),
        "shared-text" => match intent {
            ScreenshotShareIntent::Social => {
                "Image sharing was unavailable, so only text could be shared.".to_string()
            }
            _ => "Image share is unsupported here, so text was shared instead.".to_string(),
        },
        "copied" => "Copied caption text to clipboard.".to_string(),
        "cancelled" => match intent {
            ScreenshotShareIntent::Save => "Save cancelled.".to_string(),
            ScreenshotShareIntent::Social => "Social share cancelled.".to_string(),
        },
        "capture-target-missing" => {
            "Could not find the shot preview to capture. Reopen shot mode and try again."
                .to_string()
        }
        "capture-html2canvas-failed" => {
            "Could not capture shot image (html2canvas engine failed).".to_string()
        }
        "capture-html2canvas-clone-failed" => {
            "Could not capture shot image (clone html2canvas fallback failed).".to_string()
        }
        "capture-svg-failed" => "Could not capture shot image (SVG fallback failed).".to_string(),
        "capture-failed" | "encode-failed" => {
            "Could not capture the current shot image on this device.".to_string()
        }
        _ => "Sharing is unavailable on this device.".to_string(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn lyrics_share_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn lyrics_share_delay_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

pub(super) async fn share_screenshot_lyrics_image(
    capture_element_id: String,
    file_name: String,
    share_intent: String,
) -> String {
    let html2canvas_source_escaped =
        serde_json::to_string(HTML2CANVAS_BUNDLE).unwrap_or_else(|_| "\"\"".to_string());
    let capture_id_escaped =
        serde_json::to_string(&capture_element_id).unwrap_or_else(|_| "\"\"".to_string());
    let file_name_escaped =
        serde_json::to_string(&file_name).unwrap_or_else(|_| "\"rustysound-lyrics-shot.png\"".to_string());
    let share_intent_escaped =
        serde_json::to_string(&share_intent).unwrap_or_else(|_| "\"share\"".to_string());
    let script = format!(
        r###"return (async function () {{
            const html2canvasSource = {html2canvas_source_escaped};
            const captureElementId = {capture_id_escaped};
            const fileName = {file_name_escaped};
            const shareIntent = {share_intent_escaped};
            const debugTag = "[rustysound-share-shot]";

            function debugInfo(message, extra) {{
                try {{
                    console.info(debugTag + " " + message, extra || "");
                }} catch (_err) {{}}
            }}

            function debugError(message, err) {{
                try {{
                    console.error(debugTag + " " + message, err || "");
                }} catch (_err) {{}}
            }}

            function hasUnsupportedColorSyntax(value) {{
                if (typeof value !== "string") {{
                    return false;
                }}
                const lower = value.toLowerCase();
                return (
                    lower.includes("oklab(")
                    || lower.includes("oklch(")
                    || lower.includes("color-mix(")
                );
            }}

            let colorCanvasCtx = null;
            function normalizeCssColorValue(value) {{
                if (typeof value !== "string" || value.trim().length === 0) {{
                    return "";
                }}
                try {{
                    if (!colorCanvasCtx) {{
                        const canvas = document.createElement("canvas");
                        canvas.width = 1;
                        canvas.height = 1;
                        colorCanvasCtx = canvas.getContext("2d");
                    }}
                    if (!colorCanvasCtx) {{
                        return "";
                    }}
                    colorCanvasCtx.fillStyle = "#000";
                    colorCanvasCtx.fillStyle = value;
                    const normalized = String(colorCanvasCtx.fillStyle || "");
                    if (normalized && !hasUnsupportedColorSyntax(normalized)) {{
                        return normalized;
                    }}
                }} catch (_err) {{}}
                return "";
            }}

            function replaceUnsupportedColorTokens(value) {{
                if (typeof value !== "string" || !hasUnsupportedColorSyntax(value)) {{
                    return value;
                }}
                let next = value;
                const simpleColorFnRegex = /okl(?:ab|ch)\([^()]*\)/gi;
                next = next.replace(simpleColorFnRegex, (match) => {{
                    const normalized = normalizeCssColorValue(match);
                    return normalized || "rgba(255,255,255,0.72)";
                }});
                const colorMixRegex = /color-mix\((?:[^)(]+|\([^)(]*\))*\)/gi;
                next = next.replace(colorMixRegex, "rgba(255,255,255,0.24)");
                return next;
            }}

            function fallbackStyleValue(prop) {{
                const lower = String(prop || "").toLowerCase();
                if (lower.includes("shadow")) {{
                    return "none";
                }}
                if (lower.includes("background")) {{
                    return "transparent";
                }}
                if (lower.includes("border")) {{
                    return "rgba(255,255,255,0.2)";
                }}
                if (lower.includes("color")) {{
                    return "rgb(255,255,255)";
                }}
                return "";
            }}

            let colorResolverEl = null;
            function resolveStyleValue(prop, value) {{
                if (!hasUnsupportedColorSyntax(value)) {{
                    return value;
                }}
                if (typeof prop !== "string" || prop.startsWith("--")) {{
                    return "";
                }}
                try {{
                    if (!colorResolverEl) {{
                        colorResolverEl = document.createElement("div");
                        colorResolverEl.style.position = "fixed";
                        colorResolverEl.style.left = "-100000px";
                        colorResolverEl.style.top = "0";
                        colorResolverEl.style.width = "1px";
                        colorResolverEl.style.height = "1px";
                        colorResolverEl.style.opacity = "0";
                        colorResolverEl.style.pointerEvents = "none";
                        colorResolverEl.setAttribute("aria-hidden", "true");
                        document.body.appendChild(colorResolverEl);
                    }}
                    colorResolverEl.style.setProperty(prop, value);
                    const resolved = window.getComputedStyle(colorResolverEl).getPropertyValue(prop);
                    colorResolverEl.style.removeProperty(prop);
                    if (
                        typeof resolved === "string"
                        && resolved.length > 0
                        && !hasUnsupportedColorSyntax(resolved)
                    ) {{
                        return resolved;
                    }}
                }} catch (_err) {{}}
                const tokenReplaced = replaceUnsupportedColorTokens(value);
                if (!hasUnsupportedColorSyntax(tokenReplaced)) {{
                    return tokenReplaced;
                }}
                return fallbackStyleValue(prop);
            }}

            function dataUrlToBlob(dataUrl) {{
                if (typeof dataUrl !== "string") {{
                    return null;
                }}
                const parts = dataUrl.split(",");
                if (parts.length !== 2) {{
                    return null;
                }}
                const header = parts[0];
                const base64 = parts[1];
                const match = /data:([^;]+);base64/.exec(header);
                if (!match || !match[1]) {{
                    return null;
                }}
                try {{
                    const bytes = atob(base64);
                    const len = bytes.length;
                    const buffer = new Uint8Array(len);
                    for (let i = 0; i < len; i += 1) {{
                        buffer[i] = bytes.charCodeAt(i);
                    }}
                    return new Blob([buffer], {{ type: match[1] }});
                }} catch (_err) {{
                    return null;
                }}
            }}

            async function ensureLocalHtml2Canvas() {{
                if (typeof window === "undefined" || typeof document === "undefined") {{
                    return null;
                }}
                if (typeof window.html2canvas === "function") {{
                    return window.html2canvas;
                }}
                if (!window.__rustysoundHtml2CanvasPromise) {{
                    window.__rustysoundHtml2CanvasPromise = new Promise((resolve) => {{
                        try {{
                            if (
                                typeof html2canvasSource === "string"
                                && html2canvasSource.length > 128
                            ) {{
                                (0, eval)(html2canvasSource);
                                if (typeof window.html2canvas === "function") {{
                                    debugInfo("Loaded bundled html2canvas source");
                                    resolve(window.html2canvas);
                                    return;
                                }}
                                debugError(
                                    "Bundled html2canvas source loaded but window.html2canvas is missing"
                                );
                            }} else {{
                                debugError("Bundled html2canvas source missing or empty");
                            }}
                        }} catch (_err) {{
                            debugError("Evaluating bundled html2canvas failed", _err);
                        }}
                        resolve(window.html2canvas || null);
                    }});
                }}
                const loaded = await window.__rustysoundHtml2CanvasPromise;
                return loaded || window.html2canvas || null;
            }}

            function blobToDataUrl(blob) {{
                return new Promise((resolve, reject) => {{
                    const reader = new FileReader();
                    reader.onload = () => resolve(String(reader.result || ""));
                    reader.onerror = () => reject(new Error("read-failed"));
                    reader.readAsDataURL(blob);
                }});
            }}

            async function inlineImages(sourceRoot, cloneRoot) {{
                const sourceImages = Array.from(sourceRoot.querySelectorAll("img"));
                const cloneImages = Array.from(cloneRoot.querySelectorAll("img"));
                const count = Math.min(sourceImages.length, cloneImages.length);
                for (let i = 0; i < count; i += 1) {{
                    const source = sourceImages[i];
                    const clone = cloneImages[i];
                    const src = source.currentSrc || source.src || clone.getAttribute("src") || "";
                    if (!src) {{
                        continue;
                    }}
                    clone.setAttribute("src", src);
                    clone.setAttribute("data-rustysound-inline-image", "external");
                    if (src.startsWith("data:")) {{
                        clone.setAttribute("data-rustysound-inline-image", "inlined");
                        continue;
                    }}
                    try {{
                        const response = await fetch(src, {{ mode: "cors", credentials: "include" }});
                        if (!response.ok) {{
                            continue;
                        }}
                        const blob = await response.blob();
                        const dataUrl = await blobToDataUrl(blob);
                        if (dataUrl) {{
                            clone.setAttribute("src", dataUrl);
                            clone.setAttribute("data-rustysound-inline-image", "inlined");
                        }}
                    }} catch (_err) {{}}
                }}
            }}

            function copyComputedStyles(source, target) {{
                const computed = window.getComputedStyle(source);
                for (let i = 0; i < computed.length; i += 1) {{
                    const prop = computed[i];
                    const value = computed.getPropertyValue(prop);
                    const resolvedValue = resolveStyleValue(prop, value);
                    if (resolvedValue === "") {{
                        continue;
                    }}
                    const priority = computed.getPropertyPriority(prop);
                    target.style.setProperty(prop, resolvedValue, priority);
                }}
                target.style.setProperty("transform", "none");
                target.style.setProperty("animation", "none");
                target.style.setProperty("transition", "none");

                const sourceChildren = source.children || [];
                const targetChildren = target.children || [];
                const len = Math.min(sourceChildren.length, targetChildren.length);
                for (let i = 0; i < len; i += 1) {{
                    copyComputedStyles(sourceChildren[i], targetChildren[i]);
                }}
            }}

            function removeExternalImageSources(root) {{
                const images = Array.from(
                    root.querySelectorAll("img[data-rustysound-inline-image='external']")
                );
                for (const image of images) {{
                    image.removeAttribute("src");
                    image.style.setProperty("opacity", "0");
                }}
                return images.length > 0;
            }}

            function disableDocumentStylesheets() {{
                const touched = [];
                try {{
                    const nodes = Array.from(
                        document.querySelectorAll("style, link[rel='stylesheet']")
                    );
                    for (const node of nodes) {{
                        try {{
                            const previous = !!node.disabled;
                            node.disabled = true;
                            touched.push([node, previous]);
                        }} catch (_err) {{}}
                    }}
                }} catch (_err) {{}}
                return () => {{
                    for (const entry of touched) {{
                        const node = entry[0];
                        const previous = entry[1];
                        try {{
                            node.disabled = previous;
                        }} catch (_err) {{}}
                    }}
                }};
            }}

            async function captureViaHtml2CanvasClone(target, html2canvasFn, scale) {{
                const rect = target.getBoundingClientRect();
                const width = Math.max(1, Math.round(rect.width));
                const height = Math.max(1, Math.round(rect.height));

                const cloneHolder = document.createElement("div");
                cloneHolder.style.position = "fixed";
                cloneHolder.style.left = "-100000px";
                cloneHolder.style.top = "0";
                cloneHolder.style.width = width + "px";
                cloneHolder.style.height = height + "px";
                cloneHolder.style.overflow = "hidden";
                cloneHolder.style.opacity = "0";
                cloneHolder.style.pointerEvents = "none";
                cloneHolder.style.zIndex = "-1";

                const clone = target.cloneNode(true);
                copyComputedStyles(target, clone);
                clone.style.setProperty("margin", "0");
                clone.style.setProperty("width", width + "px");
                clone.style.setProperty("height", height + "px");
                await inlineImages(target, clone);
                cloneHolder.appendChild(clone);
                document.body.appendChild(cloneHolder);

                const restoreStylesheets = disableDocumentStylesheets();
                try {{
                    const renderedCanvas = await html2canvasFn(clone, {{
                        backgroundColor: null,
                        useCORS: true,
                        allowTaint: false,
                        scale,
                        logging: false,
                        imageTimeout: 9000,
                        removeContainer: false,
                    }});
                    if (!renderedCanvas || typeof renderedCanvas.toDataURL !== "function") {{
                        throw new Error("clone-html2canvas-returned-empty");
                    }}
                    return renderedCanvas.toDataURL("image/png");
                }} finally {{
                    restoreStylesheets();
                    cloneHolder.remove();
                }}
            }}

            async function renderCloneToDataUrl(clone, width, height, scale) {{
                const wrapper = document.createElement("div");
                wrapper.setAttribute("xmlns", "http://www.w3.org/1999/xhtml");
                wrapper.style.width = width + "px";
                wrapper.style.height = height + "px";
                wrapper.style.overflow = "hidden";
                wrapper.appendChild(clone);

                const serialized = new XMLSerializer().serializeToString(wrapper);
                const svg = [
                    '<svg xmlns="http://www.w3.org/2000/svg" width="' + width + '" height="' + height + '" viewBox="0 0 ' + width + " " + height + '">',
                    '<foreignObject x="0" y="0" width="100%" height="100%">',
                    serialized,
                    "</foreignObject>",
                    "</svg>",
                ].join("");
                const svgBlob = new Blob([svg], {{ type: "image/svg+xml;charset=utf-8" }});
                const svgUrl = URL.createObjectURL(svgBlob);

                try {{
                    const image = await new Promise((resolve, reject) => {{
                        const img = new Image();
                        img.onload = () => resolve(img);
                        img.onerror = () => reject(new Error("svg-image-load-failed"));
                        img.src = svgUrl;
                    }});

                    const canvas = document.createElement("canvas");
                    canvas.width = Math.max(1, Math.round(width * scale));
                    canvas.height = Math.max(1, Math.round(height * scale));
                    const ctx = canvas.getContext("2d");
                    if (!ctx) {{
                        throw new Error("svg-canvas-context-missing");
                    }}
                    ctx.setTransform(scale, 0, 0, scale, 0, 0);
                    ctx.drawImage(image, 0, 0, width, height);
                    return canvas.toDataURL("image/png");
                }} finally {{
                    URL.revokeObjectURL(svgUrl);
                }}
            }}

            async function captureViaSvgForeignObject(target, scale) {{
                const rect = target.getBoundingClientRect();
                const width = Math.max(1, Math.round(rect.width));
                const height = Math.max(1, Math.round(rect.height));

                const clone = target.cloneNode(true);
                copyComputedStyles(target, clone);
                clone.style.setProperty("margin", "0");
                clone.style.setProperty("width", width + "px");
                clone.style.setProperty("height", height + "px");

                await inlineImages(target, clone);
                try {{
                    return await renderCloneToDataUrl(clone, width, height, scale);
                }} catch (_initialRenderErr) {{
                    const hadExternalImages = removeExternalImageSources(clone);
                    if (!hadExternalImages) {{
                        throw _initialRenderErr;
                    }}
                    return await renderCloneToDataUrl(clone, width, height, scale);
                }}
            }}

            const target = document.getElementById(captureElementId);
            if (!target) {{
                debugError("Capture target element missing", captureElementId);
                return "capture-target-missing";
            }}

            let pngBlob = null;
            let captureError = "capture-failed";
            try {{
                const ratio =
                    typeof window !== "undefined" ? Number(window.devicePixelRatio || 1) : 1;
                const scale = Math.max(2, Math.min(3, ratio || 1));

                const targetImages = Array.from(target.querySelectorAll("img"));
                await Promise.all(
                    targetImages.map((img) => {{
                        if (img.complete) {{
                            return Promise.resolve();
                        }}
                        return new Promise((resolve) => {{
                            img.addEventListener("load", () => resolve(), {{ once: true }});
                            img.addEventListener("error", () => resolve(), {{ once: true }});
                            setTimeout(() => resolve(), 1200);
                        }});
                    }})
                );

                let html2canvasFn =
                    typeof window !== "undefined" ? window.html2canvas : null;
                if (typeof html2canvasFn !== "function") {{
                    html2canvasFn = await ensureLocalHtml2Canvas();
                }}
                if (typeof html2canvasFn === "function") {{
                    try {{
                        const renderedCanvas = await html2canvasFn(target, {{
                            backgroundColor: null,
                            useCORS: true,
                            allowTaint: false,
                            scale,
                            logging: false,
                            imageTimeout: 9000,
                            removeContainer: true,
                        }});
                        if (renderedCanvas && typeof renderedCanvas.toDataURL === "function") {{
                            const pngDataUrl = renderedCanvas.toDataURL("image/png");
                            pngBlob = dataUrlToBlob(pngDataUrl);
                        }}
                    }} catch (_html2canvasErr) {{
                        debugError("html2canvas capture failed", _html2canvasErr);
                        captureError = "capture-html2canvas-failed";
                    }}
                }}

                if (!pngBlob && typeof html2canvasFn === "function") {{
                    try {{
                        const pngDataUrl = await captureViaHtml2CanvasClone(
                            target,
                            html2canvasFn,
                            scale
                        );
                        pngBlob = dataUrlToBlob(pngDataUrl);
                        if (pngBlob) {{
                            debugInfo("Clone html2canvas capture succeeded");
                        }}
                    }} catch (_cloneHtml2canvasErr) {{
                        debugError(
                            "clone html2canvas capture failed",
                            _cloneHtml2canvasErr
                        );
                        captureError = "capture-html2canvas-clone-failed";
                    }}
                }}

                if (!pngBlob) {{
                    try {{
                        const pngDataUrl = await captureViaSvgForeignObject(target, scale);
                        pngBlob = dataUrlToBlob(pngDataUrl);
                    }} catch (_svgErr) {{
                        debugError("SVG foreignObject capture failed", _svgErr);
                        captureError = "capture-svg-failed";
                    }}
                }}
            }} catch (_err) {{
                debugError("Unexpected capture pipeline failure", _err);
                captureError = "capture-failed";
            }}

            if (!pngBlob) {{
                debugError("No PNG blob produced", {{
                    captureError,
                    captureElementId,
                    hasHtml2Canvas: typeof window !== "undefined" && typeof window.html2canvas === "function",
                }});
                return captureError;
            }}

            function ensureFileExtension(name, extension) {{
                const safeName = String(name || "").trim();
                if (!safeName) {{
                    return "rustysound-lyrics-shot" + extension;
                }}
                const normalized = extension.startsWith(".") ? extension : ("." + extension);
                const lower = safeName.toLowerCase();
                if (lower.endsWith(normalized.toLowerCase())) {{
                    return safeName;
                }}
                const dotIndex = safeName.lastIndexOf(".");
                if (dotIndex > 0) {{
                    return safeName.slice(0, dotIndex) + normalized;
                }}
                return safeName + normalized;
            }}

            async function convertBlobToJpeg(blob) {{
                const blobUrl = URL.createObjectURL(blob);
                try {{
                    const image = await new Promise((resolve, reject) => {{
                        const img = new Image();
                        img.onload = () => resolve(img);
                        img.onerror = () => reject(new Error("jpeg-conversion-image-load-failed"));
                        img.src = blobUrl;
                    }});
                    const width = Math.max(1, Math.round(image.width || 0));
                    const height = Math.max(1, Math.round(image.height || 0));
                    if (width <= 0 || height <= 0) {{
                        throw new Error("jpeg-conversion-invalid-size");
                    }}
                    const canvas = document.createElement("canvas");
                    canvas.width = width;
                    canvas.height = height;
                    const ctx = canvas.getContext("2d");
                    if (!ctx) {{
                        throw new Error("jpeg-conversion-context-missing");
                    }}
                    ctx.drawImage(image, 0, 0, width, height);
                    return await new Promise((resolve, reject) => {{
                        canvas.toBlob((nextBlob) => {{
                            if (nextBlob) {{
                                resolve(nextBlob);
                            }} else {{
                                reject(new Error("jpeg-conversion-blob-missing"));
                            }}
                        }}, "image/jpeg", 0.94);
                    }});
                }} finally {{
                    URL.revokeObjectURL(blobUrl);
                }}
            }}

            const normalizedShareIntent = String(shareIntent || "share")
                .trim()
                .toLowerCase();
            const preferSaveIntent = normalizedShareIntent === "save";
            const preferSocialIntent = normalizedShareIntent === "social";

            const rawFileName =
                (typeof fileName === "string" && fileName.trim().length > 0)
                    ? fileName.trim()
                    : "rustysound-lyrics-shot.png";
            let imageBlob = pngBlob;
            let imageMimeType = "image/png";
            let resolvedFileName = ensureFileExtension(rawFileName, ".png");

            if (preferSocialIntent) {{
                try {{
                    imageBlob = await convertBlobToJpeg(pngBlob);
                    imageMimeType = "image/jpeg";
                    resolvedFileName = ensureFileExtension(rawFileName, ".jpg");
                }} catch (_jpegErr) {{
                    debugError("Social JPEG conversion failed", _jpegErr);
                }}
            }}

            const imageFile = new File([imageBlob], resolvedFileName, {{ type: imageMimeType }});
            let canShareFiles = false;
            if (
                typeof navigator !== "undefined"
                && typeof navigator.share === "function"
            ) {{
                try {{
                    canShareFiles =
                        !navigator.canShare
                        || navigator.canShare({{ files: [imageFile] }});
                }} catch (_canShareErr) {{
                    debugError("navigator.canShare check failed", _canShareErr);
                    canShareFiles = false;
                }}
            }}

            try {{
                if (canShareFiles) {{
                    const imageShareData = {{
                        files: [imageFile],
                    }};
                    await navigator.share(imageShareData);
                    return "shared-image";
                }}
            }} catch (err) {{
                if (err && err.name === "AbortError") {{
                    return "cancelled";
                }}
                debugError("navigator.share image path failed", err);
            }}

            try {{
                const objectUrl = URL.createObjectURL(imageBlob);
                const anchor = document.createElement("a");
                anchor.href = objectUrl;
                anchor.download = resolvedFileName;
                anchor.rel = "noopener";
                anchor.style.display = "none";
                document.body.appendChild(anchor);
                anchor.click();
                anchor.remove();
                setTimeout(() => URL.revokeObjectURL(objectUrl), 1500);
                return "saved-image";
            }} catch (_err) {{
                debugError("Local image download fallback failed", _err);
            }}

            return "unavailable";
        }})();"###,
        html2canvas_source_escaped = html2canvas_source_escaped,
        capture_id_escaped = capture_id_escaped,
        file_name_escaped = file_name_escaped,
        share_intent_escaped = share_intent_escaped
    );

    document::eval(&script)
        .join::<String>()
        .await
        .unwrap_or_else(|_| "unavailable".to_string())
}


pub(super) const HTML2CANVAS_BUNDLE: &str = include_str!("../../../../assets/vendor/html2canvas.min.js");

pub(super) fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u32;
    format!("{:02}:{:02}", total / 60, total % 60)
}
