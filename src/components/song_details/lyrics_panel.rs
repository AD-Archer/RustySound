// Full lyrics panel including manual search and candidate results.

#[derive(Props, Clone, PartialEq)]
struct LyricsPanelProps {
    panel_dom_key: String,
    lyrics: Option<Result<LyricsResult, String>>,
    lyrics_candidates: Option<Result<Vec<LyricsSearchCandidate>, String>>,
    lyrics_candidates_search_term: Option<String>,
    selected_query_override: Option<LyricsQuery>,
    current_time: f64,
    offset_seconds: f64,
    sync_lyrics: bool,
    is_live_stream: bool,
    on_refresh: EventHandler<MouseEvent>,
    default_search_title: String,
    manual_search_title: Option<String>,
    on_manual_search: EventHandler<String>,
    on_select_lyrics_candidate: EventHandler<LyricsQuery>,
    on_clear_manual_search: EventHandler<MouseEvent>,
}

fn plain_lyrics_lines(lyrics: &LyricsResult) -> Vec<String> {
    lyrics
        .plain_lyrics
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

#[derive(Clone, PartialEq)]
struct ScreenshotLyricBar {
    text: String,
    timestamp_seconds: Option<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScreenshotShotTheme {
    Lagoon,
    Ember,
    Midnight,
    Cover,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScreenshotTextPalette {
    Auto,
    Light,
    Dark,
    Gold,
    Cyan,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScreenshotShareIntent {
    Save,
    Social,
}

fn screenshot_lyrics_bars(lyrics: &LyricsResult, sync_lyrics: bool) -> Vec<ScreenshotLyricBar> {
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
        plain_lyrics_lines(lyrics)
            .into_iter()
            .map(|line| ScreenshotLyricBar {
                text: line,
                timestamp_seconds: None,
            })
            .collect()
    }
}

fn screenshot_bar_label(bar: &ScreenshotLyricBar, include_timestamp: bool) -> String {
    if include_timestamp {
        if let Some(timestamp_seconds) = bar.timestamp_seconds {
            return format!("{} {}", format_timestamp(timestamp_seconds), bar.text);
        }
    }

    bar.text.clone()
}

fn screenshot_share_file_name(song_title: &str) -> String {
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

fn screenshot_share_intent_key(intent: ScreenshotShareIntent) -> &'static str {
    match intent {
        ScreenshotShareIntent::Save => "save",
        ScreenshotShareIntent::Social => "social",
    }
}

#[derive(Clone, Copy)]
struct ShotTextColors {
    primary: &'static str,
    secondary: &'static str,
    footer_primary: &'static str,
    footer_secondary: &'static str,
    lyrics: &'static str,
    fallback_cover_bg: &'static str,
    fallback_cover_fg: &'static str,
}

fn screenshot_shot_text_colors(
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

fn screenshot_picker_pill_class(active: bool) -> &'static str {
    if active {
        "inline-flex items-center gap-2 rounded-full border border-white/35 bg-white/12 px-3 py-2 text-sm text-white"
    } else {
        "inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/5 px-3 py-2 text-sm text-white/72 hover:text-white hover:border-white/25 transition-colors"
    }
}

fn screenshot_share_status_message(status: &str, intent: ScreenshotShareIntent) -> String {
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
async fn lyrics_share_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn lyrics_share_delay_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

async fn share_screenshot_lyrics_image(
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

const RUSTYSOUND_MARK: Asset = asset!("/assets/favicon-96x96.png");
const HTML2CANVAS_BUNDLE: &str = include_str!("../../../assets/vendor/html2canvas.min.js");

#[component]
fn LyricsPanel(props: LyricsPanelProps) -> Element {
    let navigation = use_context::<Navigation>();
    let controller = use_context::<SongDetailsController>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let audio_state = use_context::<Signal<AudioState>>();
    let search_panel_open = use_signal(|| false);
    let screenshot_view_open = use_signal(|| false);
    let screenshot_selection_start = use_signal(|| 0_usize);
    let screenshot_selection_count = use_signal(|| 1_usize);
    let screenshot_manual_selection = use_signal(|| false);
    let screenshot_shot_mode = use_signal(|| false);
    let screenshot_shot_customize_open = use_signal(|| false);
    let screenshot_shot_font_scale = use_signal(|| 100_i32);
    let screenshot_shot_blur = use_signal(|| 28_i32);
    let screenshot_shot_text_palette = use_signal(|| ScreenshotTextPalette::Auto);
    let screenshot_share_feedback = use_signal(|| None::<String>);
    let screenshot_share_feedback_generation = use_signal(|| 0_u64);
    let screenshot_share_pending = use_signal(|| false);
    let screenshot_shot_theme = {
        let default_theme = app_settings().lyrics_default_theme.clone();
        use_signal(move || match default_theme.as_str() {
            "lagoon" => ScreenshotShotTheme::Lagoon,
            "ember" => ScreenshotShotTheme::Ember,
            "midnight" => ScreenshotShotTheme::Midnight,
            _ => ScreenshotShotTheme::Cover,
        })
    };
    let theme_picker_open = use_signal(|| false);
    let programmatic_scroll_until_ms = use_signal(|| 0.0_f64);
    let manual_scroll_hold_until_ms = use_signal(|| 0.0_f64);
    let last_centered_index = use_signal(|| None::<usize>);

    let screenshot_settings = app_settings();
    let screenshot_mode_enabled = screenshot_settings.lyrics_screenshot_mode;
    let screenshot_show_timestamps = screenshot_settings.lyrics_screenshot_timestamps;
    let screenshot_song = controller.current().song;
    let screenshot_cover_url = screenshot_song
        .as_ref()
        .and_then(|song| song_cover_url(song, &servers(), 900))
        .filter(|url| !url.trim().is_empty());
    let screenshot_song_title = screenshot_song
        .as_ref()
        .map(|song| song.title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Unknown Song".to_string());
    let screenshot_song_artist = screenshot_song
        .as_ref()
        .and_then(|song| song.artist.as_ref())
        .map(|artist| artist.trim().to_string())
        .filter(|artist| !artist.is_empty());

    let on_open_settings = {
        let navigation = navigation.clone();
        let mut controller = controller.clone();
        move |_| {
            controller.close();
            navigation.navigate_to(AppView::SettingsView {});
        }
    };

    let mut search_input = use_signal(|| {
        props
            .manual_search_title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| props.default_search_title.clone())
    });

    let on_toggle_search_panel = {
        let mut search_panel_open = search_panel_open.clone();
        move |_| {
            search_panel_open.set(!search_panel_open());
        }
    };

    let on_search_submit = {
        let search_input = search_input.clone();
        let on_manual_search = props.on_manual_search.clone();
        move |_| {
            on_manual_search.call(search_input().trim().to_string());
        }
    };

    let on_use_current_song = {
        let mut search_input = search_input.clone();
        let default_search_title = props.default_search_title.clone();
        let on_clear_manual_search = props.on_clear_manual_search.clone();
        move |evt: MouseEvent| {
            search_input.set(default_search_title.clone());
            on_clear_manual_search.call(evt);
        }
    };

    let mut on_pick_candidate = {
        let on_select_lyrics_candidate = props.on_select_lyrics_candidate.clone();
        let mut search_panel_open = search_panel_open.clone();
        move |query: LyricsQuery| {
            on_select_lyrics_candidate.call(query);
            search_panel_open.set(false);
        }
    };

    {
        let mut search_input = search_input.clone();
        let manual_search_title = props.manual_search_title.clone();
        let default_search_title = props.default_search_title.clone();
        use_effect(move || {
            let next_value = manual_search_title
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default_search_title.clone());
            if search_input() != next_value {
                search_input.set(next_value);
            }
        });
    }

    let mut on_seek_line = {
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        let offset_seconds = props.offset_seconds;
        let sync_lyrics = props.sync_lyrics;
        let is_live_stream = props.is_live_stream;
        move |line: LyricLine| {
            if !sync_lyrics || is_live_stream {
                return;
            }
            let target = (line.timestamp_seconds - offset_seconds).max(0.0);
            playback_position.set(target);
            audio_state.write().current_time.set(target);
            seek_to(target);
        }
    };

    let last_successful_lyrics = use_signal(|| None::<LyricsResult>);
    {
        let mut last_successful_lyrics = last_successful_lyrics.clone();
        let latest_lyrics = props.lyrics.clone();
        use_effect(move || {
            if let Some(Ok(lyrics)) = latest_lyrics.as_ref() {
                if last_successful_lyrics().as_ref() != Some(lyrics) {
                    last_successful_lyrics.set(Some(lyrics.clone()));
                }
            }
        });
    }

    let fetch_error = match props.lyrics.clone() {
        Some(Err(error)) => Some(error),
        _ => None,
    };
    let display_lyrics = match props.lyrics.clone() {
        Some(Ok(lyrics)) => Some(lyrics),
        Some(Err(_)) | None => last_successful_lyrics(),
    };
    let screenshot_bars = display_lyrics
        .as_ref()
        .map(|lyrics| screenshot_lyrics_bars(lyrics, props.sync_lyrics))
        .unwrap_or_default();
    let screenshot_available = !screenshot_bars.is_empty();
    let screenshot_selected_start = if screenshot_available {
        screenshot_selection_start().min(screenshot_bars.len() - 1)
    } else {
        0
    };
    let screenshot_selected_count = if screenshot_available {
        screenshot_selection_count()
            .clamp(1, 5)
            .min(screenshot_bars.len() - screenshot_selected_start)
    } else {
        0
    };
    let screenshot_selected_end = if screenshot_selected_count > 0 {
        screenshot_selected_start + screenshot_selected_count - 1
    } else {
        0
    };
    let screenshot_scroll_container_id = format!(
        "lyrics-screenshot-scroll-{}",
        sanitize_dom_id(&props.panel_dom_key)
    );
    let screenshot_shot_card_id = format!(
        "lyrics-screenshot-shot-card-{}",
        sanitize_dom_id(&props.panel_dom_key)
    );
    let screenshot_shot_mode_enabled = screenshot_shot_mode();
    let screenshot_shot_customize_opened = screenshot_shot_customize_open();
    let screenshot_shot_font_scale_percent = screenshot_shot_font_scale().clamp(80, 130);
    let screenshot_shot_font_scale_ratio = screenshot_shot_font_scale_percent as f64 / 100.0;
    let screenshot_shot_theme_active = match screenshot_shot_theme() {
        ScreenshotShotTheme::Cover if screenshot_cover_url.is_some() => ScreenshotShotTheme::Cover,
        ScreenshotShotTheme::Cover => ScreenshotShotTheme::Lagoon,
        theme => theme,
    };
    let screenshot_shot_dark_text = matches!(
        screenshot_shot_theme_active,
        ScreenshotShotTheme::Lagoon | ScreenshotShotTheme::Ember
    );
    let screenshot_shot_blur_strength_px = screenshot_shot_blur().clamp(8, 48);
    let screenshot_shot_card_blur_strength_px =
        ((screenshot_shot_blur_strength_px as f64) * 0.72).round() as i32;
    let screenshot_main_backdrop_cover_style = format!(
        "filter: blur({}px); -webkit-filter: blur({}px);",
        screenshot_shot_blur_strength_px, screenshot_shot_blur_strength_px
    );
    let screenshot_card_backdrop_cover_style = format!(
        "filter: blur({}px); -webkit-filter: blur({}px);",
        screenshot_shot_card_blur_strength_px, screenshot_shot_card_blur_strength_px
    );
    let screenshot_shot_card_style = match screenshot_shot_theme_active {
        ScreenshotShotTheme::Lagoon => {
            "width:min(33rem, calc(100vw - 2.5rem), calc(100vh - 7rem)); background:linear-gradient(180deg,#79d0da 0%,#4d9fb6 100%);"
        }
        ScreenshotShotTheme::Ember => {
            "width:min(33rem, calc(100vw - 2.5rem), calc(100vh - 7rem)); background:linear-gradient(180deg,#f3cc8d 0%,#d17a66 100%);"
        }
        ScreenshotShotTheme::Midnight => {
            "width:min(33rem, calc(100vw - 2.5rem), calc(100vh - 7rem)); background:linear-gradient(180deg,#253552 0%,#0d1422 100%);"
        }
        ScreenshotShotTheme::Cover => {
            "width:min(33rem, calc(100vw - 2.5rem), calc(100vh - 7rem)); background:#111827;"
        }
    };
    let screenshot_shot_card_overlay_class = match screenshot_shot_theme_active {
        ScreenshotShotTheme::Lagoon => {
            "absolute inset-0 bg-[linear-gradient(180deg,rgba(255,255,255,0.16)_0%,rgba(255,255,255,0.03)_100%)]"
        }
        ScreenshotShotTheme::Ember => {
            "absolute inset-0 bg-[linear-gradient(180deg,rgba(255,255,255,0.18)_0%,rgba(255,255,255,0.02)_100%)]"
        }
        ScreenshotShotTheme::Midnight => {
            "absolute inset-0 bg-[linear-gradient(180deg,rgba(255,255,255,0.08)_0%,rgba(255,255,255,0.02)_100%)]"
        }
        ScreenshotShotTheme::Cover => {
            "absolute inset-0 bg-[linear-gradient(180deg,rgba(7,10,14,0.18)_0%,rgba(7,10,14,0.22)_28%,rgba(7,10,14,0.78)_100%)]"
        }
    };
    let main_backdrop_overlay_class = match screenshot_shot_theme_active {
        ScreenshotShotTheme::Lagoon => {
            "absolute inset-0 bg-[linear-gradient(180deg,rgba(74,145,173,0.72)_0%,rgba(26,57,73,0.84)_42%,rgba(8,11,16,0.98)_100%)]"
        }
        ScreenshotShotTheme::Ember => {
            "absolute inset-0 bg-[linear-gradient(180deg,rgba(210,140,90,0.72)_0%,rgba(140,60,40,0.84)_42%,rgba(8,11,16,0.98)_100%)]"
        }
        ScreenshotShotTheme::Midnight => {
            "absolute inset-0 bg-[linear-gradient(180deg,rgba(30,45,75,0.82)_0%,rgba(10,16,30,0.92)_42%,rgba(4,6,12,0.99)_100%)]"
        }
        ScreenshotShotTheme::Cover => {
            "absolute inset-0 bg-[linear-gradient(180deg,rgba(0,0,0,0.18)_0%,rgba(0,0,0,0.46)_42%,rgba(0,0,0,0.92)_100%)]"
        }
    };
    let screenshot_text_palette_active = screenshot_shot_text_palette();
    let screenshot_text_colors =
        screenshot_shot_text_colors(screenshot_text_palette_active, screenshot_shot_dark_text);
    let screenshot_shot_primary_text_style = format!("color:{};", screenshot_text_colors.primary);
    let screenshot_shot_secondary_text_style =
        format!("color:{};", screenshot_text_colors.secondary);
    let screenshot_shot_footer_primary_text_style =
        format!("color:{};", screenshot_text_colors.footer_primary);
    let screenshot_shot_footer_secondary_text_style =
        format!("color:{};", screenshot_text_colors.footer_secondary);
    let screenshot_shot_fallback_cover_style = format!(
        "background:{}; color:{};",
        screenshot_text_colors.fallback_cover_bg, screenshot_text_colors.fallback_cover_fg
    );
    let screenshot_selected_line_class =
        "block w-full rounded-2xl px-1 py-1.5 text-left text-[1.85rem] md:text-[3.05rem] font-semibold leading-[1.08] text-white whitespace-pre-wrap break-words transition-colors";
    let screenshot_unselected_line_class =
        "block w-full rounded-2xl px-1 py-1.5 text-left text-[1.85rem] md:text-[3.05rem] font-semibold leading-[1.08] text-white/36 hover:bg-white/6 hover:text-white/58 whitespace-pre-wrap break-words transition-colors";
    let screenshot_browser_width_class = "max-w-5xl";
    let screenshot_selected_bars = if screenshot_selected_count > 0 {
        screenshot_bars
            .iter()
            .skip(screenshot_selected_start)
            .take(screenshot_selected_count)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let screenshot_share_lyrics_class = "font-semibold";
    let screenshot_share_spacing_class = match screenshot_selected_count {
        0 | 1 => "space-y-6",
        2 => "space-y-5",
        3 => "space-y-4",
        4 => "space-y-3.5",
        _ => "space-y-3",
    };
    let (screenshot_share_font_size_rem, screenshot_share_line_height) =
        match screenshot_selected_count {
            0 | 1 => (2.0, 1.02),
            2 => (1.7, 1.05),
            3 => (1.45, 1.08),
            4 => (1.2, 1.1),
            _ => (1.02, 1.1),
        };
    let screenshot_share_lyrics_style = format!(
        "font-size:{:.3}rem; line-height:{:.3}; color:{};",
        screenshot_share_font_size_rem * screenshot_shot_font_scale_ratio,
        screenshot_share_line_height,
        screenshot_text_colors.lyrics
    );
    let toolbar_button_base_class =
        "h-10 w-10 rounded-full border flex items-center justify-center transition-colors";
    let screenshot_share_feedback_message = screenshot_share_feedback();
    let playback_seconds_signal = playback_position();
    let playback_seconds = if (props.current_time - playback_seconds_signal).abs() > 1.0 {
        props.current_time
    } else {
        playback_seconds_signal
    };
    let active_synced_index = if !props.sync_lyrics || props.is_live_stream {
        None
    } else {
        display_lyrics.as_ref().and_then(|lyrics| {
            active_lyric_index(
                &lyrics.synced_lines,
                playback_seconds + props.offset_seconds,
            )
        })
    };

    let scroll_container_id = format!("lyrics-scroll-{}", sanitize_dom_id(&props.panel_dom_key));

    let on_lyrics_scrolled = {
        let programmatic_scroll_until_ms = programmatic_scroll_until_ms.clone();
        let mut manual_scroll_hold_until_ms = manual_scroll_hold_until_ms.clone();
        let mut last_centered_index = last_centered_index.clone();
        move |_| {
            let now = now_millis();
            if now < programmatic_scroll_until_ms() {
                return;
            }
            manual_scroll_hold_until_ms.set(now + 1800.0);
            last_centered_index.set(None);
        }
    };

    {
        let active_synced_index = active_synced_index;
        let scroll_container_id = scroll_container_id.clone();
        let sync_lyrics = props.sync_lyrics;
        let is_live_stream = props.is_live_stream;
        let audio_state = audio_state.clone();
        let mut programmatic_scroll_until_ms = programmatic_scroll_until_ms.clone();
        let manual_scroll_hold_until_ms = manual_scroll_hold_until_ms.clone();
        let mut last_centered_index = last_centered_index.clone();
        use_effect(move || {
            let _playback_tick = (audio_state().current_time)();
            let Some(index) = active_synced_index else {
                return;
            };
            if !sync_lyrics || is_live_stream {
                return;
            }
            if now_millis() < manual_scroll_hold_until_ms() {
                return;
            }

            let should_recenter = last_centered_index() != Some(index);
            if !should_recenter {
                return;
            }

            let line_id = format!("{scroll_container_id}-line-{index}");
            let script = format!(
                r#"(function() {{
                    const container = document.getElementById("{scroll_container_id}");
                    const line = document.getElementById("{line_id}");
                    if (!container || !line) return;
                    const cRect = container.getBoundingClientRect();
                    const lRect = line.getBoundingClientRect();
                    const target = container.scrollTop + (lRect.top - cRect.top) - (cRect.height / 2) + (lRect.height / 2);
                    container.scrollTo({{ top: target, behavior: "auto" }});
                }})();"#
            );
            let _ = document::eval(&script);
            programmatic_scroll_until_ms.set(now_millis() + 250.0);
            last_centered_index.set(Some(index));
        });
    }

    {
        let screenshot_view_open = screenshot_view_open.clone();
        let screenshot_selection_start = screenshot_selection_start.clone();
        let screenshot_scroll_container_id = screenshot_scroll_container_id.clone();
        let screenshot_bar_total = screenshot_bars.len();
        use_effect(move || {
            if !screenshot_view_open() || screenshot_bar_total == 0 {
                return;
            }

            let selected_start = screenshot_selection_start().min(screenshot_bar_total - 1);
            let line_id = format!("{screenshot_scroll_container_id}-line-{selected_start}");
            let script = format!(
                r#"(function() {{
                    const container = document.getElementById("{screenshot_scroll_container_id}");
                    const line = document.getElementById("{line_id}");
                    if (!container || !line) return;
                    const cRect = container.getBoundingClientRect();
                    const lRect = line.getBoundingClientRect();
                    const target = container.scrollTop + (lRect.top - cRect.top) - (cRect.height / 2) + (lRect.height / 2);
                    container.scrollTo({{ top: target, behavior: "auto" }});
                }})();"#
            );
            let _ = document::eval(&script);
        });
    }

    {
        let screenshot_view_open = screenshot_view_open.clone();
        let screenshot_manual_selection = screenshot_manual_selection.clone();
        let mut screenshot_selection_start = screenshot_selection_start.clone();
        let mut screenshot_selection_count = screenshot_selection_count.clone();
        let active_synced_index = active_synced_index;
        let sync_lyrics = props.sync_lyrics;
        let is_live_stream = props.is_live_stream;
        let audio_state = audio_state.clone();
        use_effect(move || {
            let _playback_tick = (audio_state().current_time)();
            if !screenshot_view_open()
                || screenshot_manual_selection()
                || !sync_lyrics
                || is_live_stream
            {
                return;
            }

            if let Some(index) = active_synced_index {
                if screenshot_selection_start() != index {
                    screenshot_selection_start.set(index);
                }
                if screenshot_selection_count() != 1 {
                    screenshot_selection_count.set(1);
                }
            }
        });
    }

    let on_open_screenshot_view = {
        let mut screenshot_view_open = screenshot_view_open.clone();
        let mut screenshot_selection_start = screenshot_selection_start.clone();
        let mut screenshot_selection_count = screenshot_selection_count.clone();
        let mut screenshot_manual_selection = screenshot_manual_selection.clone();
        let mut screenshot_shot_mode = screenshot_shot_mode.clone();
        let mut screenshot_shot_customize_open = screenshot_shot_customize_open.clone();
        let mut screenshot_share_feedback = screenshot_share_feedback.clone();
        let mut screenshot_share_pending = screenshot_share_pending.clone();
        let active_synced_index = active_synced_index;
        let screenshot_bars = screenshot_bars.clone();
        move |_| {
            let focus_index = active_synced_index
                .unwrap_or(0)
                .min(screenshot_bars.len().saturating_sub(1));
            screenshot_selection_start.set(focus_index);
            screenshot_selection_count.set(1);
            screenshot_manual_selection.set(false);
            screenshot_shot_mode.set(false);
            screenshot_shot_customize_open.set(false);
            screenshot_share_feedback.set(None);
            screenshot_share_pending.set(false);
            screenshot_view_open.set(true);
        }
    };

    let on_close_screenshot_view = {
        let mut screenshot_view_open = screenshot_view_open.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            screenshot_view_open.set(false);
        }
    };

    let start_screenshot_share: std::rc::Rc<dyn Fn(ScreenshotShareIntent)> = {
        let screenshot_song_title = screenshot_song_title.clone();
        let screenshot_shot_card_id = screenshot_shot_card_id.clone();
        let screenshot_selected_bars = screenshot_selected_bars.clone();
        let screenshot_share_feedback = screenshot_share_feedback.clone();
        let screenshot_share_feedback_generation = screenshot_share_feedback_generation.clone();
        let screenshot_share_pending = screenshot_share_pending.clone();

        std::rc::Rc::new(move |share_intent: ScreenshotShareIntent| {
            let mut screenshot_share_feedback = screenshot_share_feedback.clone();
            let mut screenshot_share_feedback_generation = screenshot_share_feedback_generation.clone();
            let mut screenshot_share_pending = screenshot_share_pending.clone();

            if screenshot_share_pending() {
                return;
            }

            if screenshot_selected_bars.is_empty() {
                screenshot_share_feedback.set(Some(
                    "Select one or more lyric lines to share.".to_string(),
                ));
                return;
            }

            let has_shareable_line = screenshot_selected_bars
                .iter()
                .any(|bar| !bar.text.trim().is_empty());
            if !has_shareable_line {
                screenshot_share_feedback.set(Some(
                    "Select one or more lyric lines to share.".to_string(),
                ));
                return;
            }
            let share_file_name = screenshot_share_file_name(&screenshot_song_title);
            let capture_id = screenshot_shot_card_id.clone();
            let share_intent_key = screenshot_share_intent_key(share_intent).to_string();

            screenshot_share_pending.set(true);
            screenshot_share_feedback_generation.with_mut(|value| {
                *value = value.saturating_add(1);
            });
            let generation = *screenshot_share_feedback_generation.peek();

            let mut screenshot_share_feedback = screenshot_share_feedback.clone();
            let screenshot_share_feedback_generation = screenshot_share_feedback_generation.clone();
            let mut screenshot_share_pending = screenshot_share_pending.clone();
            spawn(async move {
                let debug_capture_id = capture_id.clone();
                let debug_share_file_name = share_file_name.clone();
                let status = share_screenshot_lyrics_image(
                    capture_id,
                    share_file_name,
                    share_intent_key,
                )
                .await;
                eprintln!(
                    "[lyrics-share-shot] status={status} intent={} capture_id={} file={}",
                    screenshot_share_intent_key(share_intent),
                    debug_capture_id,
                    debug_share_file_name
                );
                let message = screenshot_share_status_message(&status, share_intent);

                screenshot_share_pending.set(false);
                screenshot_share_feedback.set(Some(message));

                lyrics_share_delay_ms(2600).await;
                if *screenshot_share_feedback_generation.peek() == generation {
                    screenshot_share_feedback.set(None);
                }
            });
        })
    };

    let on_save_screenshot_shot = {
        let start_screenshot_share = start_screenshot_share.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            start_screenshot_share(ScreenshotShareIntent::Save);
        }
    };

    let on_share_screenshot_to_Social = {
        let start_screenshot_share = start_screenshot_share.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            start_screenshot_share(ScreenshotShareIntent::Social);
        }
    };

    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center justify-between gap-2",
                button {
                    class: if search_panel_open() { "{toolbar_button_base_class} border-emerald-500/50 text-emerald-300 hover:text-emerald-200" } else { "{toolbar_button_base_class} border-zinc-700/70 text-zinc-300 hover:text-white" },
                    title: if search_panel_open() { "Close lyrics search" } else { "Open lyrics search" },
                    onclick: on_toggle_search_panel,
                    Icon {
                        name: "search".to_string(),
                        class: "w-4.5 h-4.5".to_string(),
                    }
                }
                div { class: "flex items-center gap-2",
                    if screenshot_mode_enabled {
                        button {
                            class: if screenshot_available { "{toolbar_button_base_class} border-cyan-500/40 text-cyan-300 hover:text-white hover:border-cyan-300" } else { "{toolbar_button_base_class} border-zinc-700/70 text-zinc-500 cursor-not-allowed" },
                            title: "Open lyrics screenshot view",
                            disabled: !screenshot_available,
                            onclick: on_open_screenshot_view,
                            Icon {
                                name: "eye".to_string(),
                                class: "w-4.5 h-4.5".to_string(),
                            }
                        }
                    }
                    button {
                        class: "{toolbar_button_base_class} border-zinc-700/70 text-zinc-300 hover:text-white",
                        title: "Refresh lyrics",
                        onclick: move |evt| props.on_refresh.call(evt),
                        Icon {
                            name: "refresh-cw".to_string(),
                            class: "w-4.5 h-4.5".to_string(),
                        }
                    }
                    button {
                        class: "{toolbar_button_base_class} border-emerald-500/40 bg-emerald-500/20 text-emerald-300 hover:text-emerald-200",
                        title: "Open lyrics settings",
                        onclick: on_open_settings,
                        Icon {
                            name: "settings".to_string(),
                            class: "w-4.5 h-4.5".to_string(),
                        }
                    }
                }
            }

            if search_panel_open() {
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-3 space-y-3",
                    p { class: "text-xs uppercase tracking-wider text-zinc-500",
                        "Search And Pick Lyrics"
                    }
                    div { class: "flex flex-col sm:flex-row gap-2",
                        input {
                            r#type: "text",
                            value: "{search_input}",
                            placeholder: "Enter a song title",
                            class: "flex-1 px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-950 text-zinc-100 text-sm focus:outline-none focus:border-emerald-500/50",
                            oninput: move |evt| search_input.set(evt.value()),
                        }
                        button {
                            class: "px-3 py-2 rounded-lg bg-emerald-500 hover:bg-emerald-400 text-white text-sm transition-colors",
                            onclick: on_search_submit,
                            "Search"
                        }
                        button {
                            class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-400 hover:text-white transition-colors text-sm",
                            onclick: on_use_current_song,
                            "Use Current Song"
                        }
                    }

                    if let Some(search_term) = props
                        .lyrics_candidates_search_term
                        .clone()
                        .filter(|value| !value.trim().is_empty())
                    {
                        div { class: "rounded-lg border border-zinc-800/80 bg-zinc-950/60 p-2 space-y-2",
                            p { class: "text-[11px] text-zinc-500 px-1",
                                "Pick the best match for \"{search_term}\""
                            }
                            match props.lyrics_candidates.clone() {
                                None => rsx! {
                                    div { class: "px-2 py-3 flex items-center gap-2 text-zinc-500 text-sm",
                                        Icon { name: "loader".to_string(), class: "w-4 h-4".to_string() }
                                        "Searching..."
                                    }
                                },
                                Some(Err(error)) => rsx! {
                                    p { class: "px-2 py-2 text-xs text-zinc-500 break-words", "{error}" }
                                },
                                Some(Ok(candidates)) => {
                                    if candidates.is_empty() {
                                        rsx! {
                                            p { class: "px-2 py-2 text-xs text-zinc-500", "No lyric matches found." }
                                        }
                                    } else {
                                        rsx! {
                                            div { class: "max-h-56 overflow-y-auto pr-1 space-y-1",
                                                for candidate in candidates {
                                                    button {
                                                        class: if props
                                                            .selected_query_override
                                                            .as_ref()
                                                            == Some(&candidate.query)
                                                        {
                                                            "w-full text-left p-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10"
                                                        } else {
                                                            "w-full text-left p-2 rounded-lg border border-zinc-800/70 hover:border-zinc-600 hover:bg-zinc-900/70 transition-colors"
                                                        },
                                                        onclick: {
                                                            let query = candidate.query.clone();
                                                            move |_| on_pick_candidate(query.clone())
                                                        },
                                                        div { class: "flex items-center justify-between gap-3",
                                                            div { class: "min-w-0",
                                                                p { class: "text-sm text-white truncate", "{candidate.title}" }
                                                                p { class: "text-xs text-zinc-500 truncate", "{candidate.artist}" }
                                                            }
                                                            div { class: "text-right flex-shrink-0",
                                                                p { class: "text-[10px] uppercase tracking-wider text-zinc-500",
                                                                    "{candidate.provider.label()}"
                                                                }
                                                                if let Some(duration) = candidate.duration_seconds {
                                                                    p { class: "text-[11px] text-zinc-500 font-mono",
                                                                        "{format_duration(duration)}"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        if !candidate.album.trim().is_empty() {
                                                            p { class: "text-[11px] text-zinc-600 truncate mt-1", "{candidate.album}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div {
                id: "{scroll_container_id}",
                onscroll: on_lyrics_scrolled,
                class: "rounded-xl border border-zinc-800/80 bg-zinc-900/40 min-h-[52vh] md:min-h-[64vh] max-h-[76vh] overflow-y-auto overflow-x-hidden",
                if props.is_live_stream {
                    p { class: "px-5 pt-4 text-xs text-zinc-500",
                        "Live stream detected: synced lyric scrolling and seek controls are disabled."
                    }
                }
                match display_lyrics.clone() {
                    None => {
                        if let Some(error) = fetch_error {
                            rsx! {
                                div { class: "p-6 space-y-2",
                                    p { class: "text-sm text-zinc-400", "No lyrics found for this song." }
                                    p { class: "text-xs text-zinc-500", "Try a manual search and pick the exact match." }
                                    p { class: "text-xs text-zinc-600 break-words", "{error}" }
                                }
                            }
                        } else {
                            rsx! {
                                div { class: "p-6 flex items-center justify-center text-zinc-500 gap-2",
                                    Icon { name: "loader".to_string(), class: "w-4 h-4".to_string() }
                                    "Loading lyrics..."
                                }
                            }
                        }
                    }
                    Some(lyrics) => {
                        if !props.sync_lyrics || lyrics.synced_lines.is_empty() {
                            let lines = plain_lyrics_lines(&lyrics);

                            rsx! {
                                div { class: "p-5 space-y-2",
                                    if fetch_error.is_some() {
                                        p { class: "text-xs text-amber-300/90",
                                            "Using last loaded lyrics because the latest fetch failed."
                                        }
                                    }
                                    div { class: "text-xs uppercase tracking-wider text-zinc-500 pb-1",
                                        "Source: {lyrics.provider.label()}"
                                    }
                                    if props.sync_lyrics && lyrics.synced_lines.is_empty() {
                                        p { class: "text-xs text-zinc-500",
                                            "Synced timestamps are not available from this result. Showing plain lyrics."
                                        }
                                    }
                                    if lines.is_empty() {
                                        p { class: "text-base text-zinc-500", "Lyrics unavailable." }
                                    } else {
                                        for line in lines {
                                            p { class: "text-base text-zinc-300 leading-relaxed whitespace-pre-wrap break-words",
                                                "{line}"
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {
                                div { class: "p-4 space-y-1",
                                    if fetch_error.is_some() {
                                        p { class: "text-xs text-amber-300/90 pb-1",
                                            "Using last loaded lyrics because the latest fetch failed."
                                        }
                                    }
                                    div { class: "text-xs uppercase tracking-wider text-zinc-500 pb-1",
                                        "Source: {lyrics.provider.label()}"
                                    }
                                    for (index , line) in lyrics.synced_lines.iter().enumerate() {
                                        button {
                                            id: format!("{scroll_container_id}-line-{index}"),
                                            class: if Some(index) == active_synced_index { "w-full text-left px-3 py-2.5 rounded-lg bg-emerald-500/15 text-emerald-300 overflow-hidden" } else { "w-full text-left px-3 py-2 rounded-lg text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/60 transition-colors overflow-hidden" },
                                            onclick: {
                                                let line = line.clone();
                                                move |_| on_seek_line(line.clone())
                                            },
                                            span { class: "text-xs text-zinc-500 mr-2 font-mono",
                                                "{format_timestamp(line.timestamp_seconds)}"
                                            }
                                            span { class: if Some(index) == active_synced_index { "text-lg md:text-xl font-semibold leading-relaxed whitespace-pre-wrap break-words align-top" } else { "text-base leading-relaxed whitespace-pre-wrap break-words align-top" },
                                                "{line.text}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if screenshot_view_open() && screenshot_mode_enabled {
                div {
                    class: "fixed inset-0 z-[120] bg-black/88 backdrop-blur-md",
                    onclick: {
                        let mut screenshot_view_open = screenshot_view_open.clone();
                        let mut screenshot_shot_mode = screenshot_shot_mode.clone();
                        let mut screenshot_shot_customize_open = screenshot_shot_customize_open.clone();
                        let mut theme_picker_open = theme_picker_open.clone();
                        move |_| {
                            if theme_picker_open() {
                                theme_picker_open.set(false);
                            } else if screenshot_shot_mode() {
                                screenshot_shot_mode.set(false);
                                screenshot_shot_customize_open.set(false);
                            } else {
                                screenshot_view_open.set(false);
                            }
                        }
                    },
                    div { class: "absolute top-12 right-4 z-20 flex items-center gap-2 md:top-14 md:right-6",
                        if screenshot_shot_mode_enabled {
                            div { class: "flex items-center gap-2",
                                button {
                                    class: if screenshot_share_pending() { "rounded-full border border-emerald-300/40 bg-emerald-200/20 px-3 py-2 text-sm text-emerald-100/85 transition-colors cursor-wait" } else if screenshot_selected_bars.is_empty() { "rounded-full border border-emerald-300/25 bg-emerald-900/20 px-3 py-2 text-sm text-emerald-100/45 transition-colors cursor-not-allowed" } else { "rounded-full border border-emerald-300/35 bg-emerald-900/30 px-3 py-2 text-sm text-emerald-100/85 hover:text-white hover:border-emerald-200/60 transition-colors" },
                                    title: if screenshot_selected_bars.is_empty() { "Select one or more lyric lines to save" } else if screenshot_share_pending() { "Preparing image for saving..." } else { "Save shot image to device" },
                                    disabled: screenshot_selected_bars.is_empty() || screenshot_share_pending(),
                                    onclick: on_save_screenshot_shot,
                                    "Save"
                                }
                                button {
                                    class: if screenshot_share_pending() { "rounded-full border border-fuchsia-300/35 bg-fuchsia-300/14 px-3 py-2 text-sm text-fuchsia-100/85 transition-colors cursor-wait" } else if screenshot_selected_bars.is_empty() { "rounded-full border border-fuchsia-300/25 bg-fuchsia-900/18 px-3 py-2 text-sm text-fuchsia-100/45 transition-colors cursor-not-allowed" } else { "rounded-full border border-fuchsia-300/35 bg-fuchsia-900/24 px-3 py-2 text-sm text-fuchsia-100/85 hover:text-white hover:border-fuchsia-200/60 transition-colors" },
                                    title: if screenshot_selected_bars.is_empty() { "Select one or more lyric lines to share on Social" } else if screenshot_share_pending() { "Preparing image for Social..." } else { "Share shot image for Social" },
                                    disabled: screenshot_selected_bars.is_empty() || screenshot_share_pending(),
                                    onclick: on_share_screenshot_to_Social,
                                    "Social"
                                }
                            }
                        }
                        button {
                            class: if theme_picker_open() { "rounded-full border border-white/30 bg-white/14 p-2 text-white transition-colors" } else { "rounded-full border border-white/15 bg-black/35 p-2 text-white/80 hover:text-white hover:border-white/30 transition-colors" },
                            title: "Choose background theme",
                            onclick: {
                                let mut theme_picker_open = theme_picker_open.clone();
                                let mut screenshot_shot_customize_open = screenshot_shot_customize_open.clone();
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    let opening = !theme_picker_open();
                                    theme_picker_open.set(opening);
                                    if opening {
                                        screenshot_shot_customize_open.set(false);
                                    }
                                }
                            },
                            Icon {
                                name: "swatch".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        button {
                            class: "rounded-full border border-white/15 bg-black/35 p-2 text-white/80 hover:text-white hover:border-white/30 transition-colors",
                            onclick: on_close_screenshot_view,
                            Icon {
                                name: "x".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                    }
                    if let Some(message) = screenshot_share_feedback_message.clone() {
                        div {
                            class: "absolute left-1/2 top-24 z-20 -translate-x-1/2 rounded-full border border-white/15 bg-black/55 px-4 py-2 text-xs text-white/85 shadow-[0_10px_40px_rgba(0,0,0,0.35)] backdrop-blur md:top-26",
                            onclick: move |evt: MouseEvent| evt.stop_propagation(),
                            "{message}"
                        }
                    }
                    if theme_picker_open() {
                        div {
                            class: "absolute right-4 top-28 z-20 w-[min(18rem,calc(100vw-2rem))] rounded-[1.4rem] border border-white/15 bg-black/45 p-4 text-white shadow-[0_22px_60px_rgba(0,0,0,0.38)] backdrop-blur-xl md:right-6 md:top-32",
                            onclick: move |evt: MouseEvent| evt.stop_propagation(),
                            p { class: "text-sm font-semibold text-white mb-3", "Background theme" }
                            div { class: "flex flex-wrap gap-2",
                                button {
                                    class: if screenshot_shot_theme_active == ScreenshotShotTheme::Lagoon { "inline-flex items-center gap-2 rounded-full border border-white/35 bg-white/12 px-3 py-2 text-sm text-white" } else { "inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/5 px-3 py-2 text-sm text-white/72 hover:text-white hover:border-white/25 transition-colors" },
                                    onclick: {
                                        let mut screenshot_shot_theme = screenshot_shot_theme.clone();
                                        let mut app_settings = app_settings.clone();
                                        let mut theme_picker_open = theme_picker_open.clone();
                                        move |_| {
                                            screenshot_shot_theme.set(ScreenshotShotTheme::Lagoon);
                                            theme_picker_open.set(false);
                                            let mut settings = app_settings();
                                            settings.lyrics_default_theme = "lagoon".to_string();
                                            app_settings.set(settings.clone());
                                            spawn(async move {
                                                let _ = crate::db::save_settings(settings).await;
                                            });
                                        }
                                    },
                                    span { class: "h-3 w-3 rounded-full bg-[#62bac9]" }
                                    "Lagoon"
                                }
                                button {
                                    class: if screenshot_shot_theme_active == ScreenshotShotTheme::Ember { "inline-flex items-center gap-2 rounded-full border border-white/35 bg-white/12 px-3 py-2 text-sm text-white" } else { "inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/5 px-3 py-2 text-sm text-white/72 hover:text-white hover:border-white/25 transition-colors" },
                                    onclick: {
                                        let mut screenshot_shot_theme = screenshot_shot_theme.clone();
                                        let mut app_settings = app_settings.clone();
                                        let mut theme_picker_open = theme_picker_open.clone();
                                        move |_| {
                                            screenshot_shot_theme.set(ScreenshotShotTheme::Ember);
                                            theme_picker_open.set(false);
                                            let mut settings = app_settings();
                                            settings.lyrics_default_theme = "ember".to_string();
                                            app_settings.set(settings.clone());
                                            spawn(async move {
                                                let _ = crate::db::save_settings(settings).await;
                                            });
                                        }
                                    },
                                    span { class: "h-3 w-3 rounded-full bg-[#df8a71]" }
                                    "Ember"
                                }
                                button {
                                    class: if screenshot_shot_theme_active == ScreenshotShotTheme::Midnight { "inline-flex items-center gap-2 rounded-full border border-white/35 bg-white/12 px-3 py-2 text-sm text-white" } else { "inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/5 px-3 py-2 text-sm text-white/72 hover:text-white hover:border-white/25 transition-colors" },
                                    onclick: {
                                        let mut screenshot_shot_theme = screenshot_shot_theme.clone();
                                        let mut app_settings = app_settings.clone();
                                        let mut theme_picker_open = theme_picker_open.clone();
                                        move |_| {
                                            screenshot_shot_theme.set(ScreenshotShotTheme::Midnight);
                                            theme_picker_open.set(false);
                                            let mut settings = app_settings();
                                            settings.lyrics_default_theme = "midnight".to_string();
                                            app_settings.set(settings.clone());
                                            spawn(async move {
                                                let _ = crate::db::save_settings(settings).await;
                                            });
                                        }
                                    },
                                    span { class: "h-3 w-3 rounded-full bg-[#1f2a44]" }
                                    "Midnight"
                                }
                                if screenshot_cover_url.is_some() {
                                    button {
                                        class: if screenshot_shot_theme_active == ScreenshotShotTheme::Cover { "inline-flex items-center gap-2 rounded-full border border-white/35 bg-white/12 px-3 py-2 text-sm text-white" } else { "inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/5 px-3 py-2 text-sm text-white/72 hover:text-white hover:border-white/25 transition-colors" },
                                        onclick: {
                                            let mut screenshot_shot_theme = screenshot_shot_theme.clone();
                                            let mut app_settings = app_settings.clone();
                                            let mut theme_picker_open = theme_picker_open.clone();
                                            move |_| {
                                                screenshot_shot_theme.set(ScreenshotShotTheme::Cover);
                                                theme_picker_open.set(false);
                                                let mut settings = app_settings();
                                                settings.lyrics_default_theme = "cover".to_string();
                                                app_settings.set(settings.clone());
                                                spawn(async move {
                                                    let _ = crate::db::save_settings(settings).await;
                                                });
                                            }
                                        },
                                        if let Some(url) = screenshot_cover_url.clone() {
                                            img {
                                                class: "h-4 w-4 rounded object-cover",
                                                src: "{url}",
                                                alt: "Album art",
                                            }
                                        }
                                        "Cover"
                                    }
                                }
                            }
                        }
                    }
                    if !screenshot_shot_mode_enabled {
                        button {
                            class: "absolute top-12 left-4 z-20 rounded-full border border-white/15 bg-black/35 px-4 py-2 text-sm font-medium text-white/80 hover:text-white hover:border-white/30 transition-colors md:top-14 md:left-6",
                            onclick: {
                                let mut screenshot_shot_mode = screenshot_shot_mode.clone();
                                let mut screenshot_shot_customize_open = screenshot_shot_customize_open.clone();
                                let mut theme_picker_open = theme_picker_open.clone();
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    screenshot_shot_mode.set(true);
                                    screenshot_shot_customize_open.set(false);
                                    theme_picker_open.set(false);
                                }
                            },
                            "Shot"
                        }
                    } else {
                        button {
                            class: if screenshot_shot_customize_opened { "absolute top-12 left-4 z-20 rounded-full border border-white/30 bg-white/14 px-4 py-2 text-sm font-medium text-white transition-colors md:top-14 md:left-6" } else { "absolute top-12 left-4 z-20 rounded-full border border-white/15 bg-black/35 px-4 py-2 text-sm font-medium text-white/80 hover:text-white hover:border-white/30 transition-colors md:top-14 md:left-6" },
                            onclick: {
                                let mut screenshot_shot_customize_open = screenshot_shot_customize_open.clone();
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    screenshot_shot_customize_open
                                        .set(!screenshot_shot_customize_open());
                                }
                            },
                            "Customize"
                        }
                    }
                    if screenshot_shot_mode_enabled && screenshot_shot_customize_opened {
                        div {
                            class: "absolute left-4 top-28 z-20 w-[min(20rem,calc(100vw-2rem))] rounded-[1.4rem] border border-white/15 bg-black/45 p-4 text-white shadow-[0_22px_60px_rgba(0,0,0,0.38)] backdrop-blur-xl md:left-6 md:top-32",
                            onclick: move |evt: MouseEvent| evt.stop_propagation(),
                            div { class: "flex items-center justify-between gap-3",
                                p { class: "text-sm font-semibold text-white", "Customize shot" }
                                button {
                                    class: "rounded-full border border-white/15 px-3 py-1 text-[11px] uppercase tracking-[0.2em] text-white/70 hover:text-white hover:border-white/30 transition-colors",
                                    onclick: {
                                        let mut screenshot_shot_font_scale = screenshot_shot_font_scale.clone();
                                        let mut screenshot_shot_blur = screenshot_shot_blur.clone();
                                        let mut screenshot_shot_text_palette = screenshot_shot_text_palette.clone();
                                        let mut screenshot_shot_theme = screenshot_shot_theme.clone();
                                        let mut app_settings = app_settings.clone();
                                        move |_| {
                                            screenshot_shot_font_scale.set(100);
                                            screenshot_shot_blur.set(28);
                                            screenshot_shot_text_palette.set(ScreenshotTextPalette::Auto);
                                            screenshot_shot_theme.set(ScreenshotShotTheme::Cover);
                                            let mut settings = app_settings();
                                            settings.lyrics_default_theme = "cover".to_string();
                                            app_settings.set(settings.clone());
                                            spawn(async move {
                                                let _ = crate::db::save_settings(settings).await;
                                            });
                                        }
                                    },
                                    "Reset"
                                }
                            }
                            div { class: "mt-4 space-y-2",
                                p { class: "text-[11px] uppercase tracking-[0.22em] text-white/45",
                                    "Lyrics size"
                                }
                                div { class: "flex items-center gap-3",
                                    span { class: "text-xs text-white/55", "A" }
                                    input {
                                        r#type: "range",
                                        min: "80",
                                        max: "130",
                                        step: "5",
                                        value: "{screenshot_shot_font_scale_percent}",
                                        class: "flex-1 h-1.5 cursor-pointer appearance-none rounded-full bg-white/15 accent-white",
                                        oninput: {
                                            let mut screenshot_shot_font_scale = screenshot_shot_font_scale.clone();
                                            move |evt| {
                                                if let Ok(value) = evt.value().parse::<i32>() {
                                                    screenshot_shot_font_scale.set(value.clamp(80, 130));
                                                }
                                            }
                                        },
                                    }
                                    span { class: "text-base font-semibold text-white/78",
                                        "A"
                                    }
                                }
                                p { class: "text-[11px] text-white/45",
                                    "{screenshot_shot_font_scale_percent}%"
                                }
                            }
                            div { class: "mt-4 space-y-2",
                                p { class: "text-[11px] uppercase tracking-[0.22em] text-white/45",
                                    "Album blur"
                                }
                                div { class: "flex items-center gap-3",
                                    span { class: "text-xs text-white/55", "Soft" }
                                    input {
                                        r#type: "range",
                                        min: "8",
                                        max: "48",
                                        step: "2",
                                        value: "{screenshot_shot_blur_strength_px}",
                                        class: "flex-1 h-1.5 cursor-pointer appearance-none rounded-full bg-white/15 accent-white",
                                        oninput: {
                                            let mut screenshot_shot_blur = screenshot_shot_blur.clone();
                                            move |evt| {
                                                if let Ok(value) = evt.value().parse::<i32>() {
                                                    screenshot_shot_blur.set(value.clamp(8, 48));
                                                }
                                            }
                                        },
                                    }
                                    span { class: "text-xs text-white/72", "Bold" }
                                }
                                p { class: "text-[11px] text-white/45",
                                    "{screenshot_shot_blur_strength_px}px"
                                }
                            }
                            div { class: "mt-4 space-y-2",
                                p { class: "text-[11px] uppercase tracking-[0.22em] text-white/45",
                                    "Text color"
                                }
                                div { class: "flex flex-wrap gap-2",
                                    button {
                                        class: screenshot_picker_pill_class(
                                            screenshot_text_palette_active == ScreenshotTextPalette::Auto,
                                        ),
                                        onclick: {
                                            let mut screenshot_shot_text_palette = screenshot_shot_text_palette.clone();
                                            move |_| screenshot_shot_text_palette.set(ScreenshotTextPalette::Auto)
                                        },
                                        span { class: "h-3 w-3 rounded-full bg-gradient-to-br from-white to-zinc-800" }
                                        "Auto"
                                    }
                                    button {
                                        class: screenshot_picker_pill_class(
                                            screenshot_text_palette_active == ScreenshotTextPalette::Light,
                                        ),
                                        onclick: {
                                            let mut screenshot_shot_text_palette = screenshot_shot_text_palette.clone();
                                            move |_| screenshot_shot_text_palette.set(ScreenshotTextPalette::Light)
                                        },
                                        span { class: "h-3 w-3 rounded-full bg-white" }
                                        "Light"
                                    }
                                    button {
                                        class: screenshot_picker_pill_class(
                                            screenshot_text_palette_active == ScreenshotTextPalette::Dark,
                                        ),
                                        onclick: {
                                            let mut screenshot_shot_text_palette = screenshot_shot_text_palette.clone();
                                            move |_| screenshot_shot_text_palette.set(ScreenshotTextPalette::Dark)
                                        },
                                        span { class: "h-3 w-3 rounded-full bg-zinc-900" }
                                        "Dark"
                                    }
                                    button {
                                        class: screenshot_picker_pill_class(
                                            screenshot_text_palette_active == ScreenshotTextPalette::Gold,
                                        ),
                                        onclick: {
                                            let mut screenshot_shot_text_palette = screenshot_shot_text_palette.clone();
                                            move |_| screenshot_shot_text_palette.set(ScreenshotTextPalette::Gold)
                                        },
                                        span { class: "h-3 w-3 rounded-full bg-amber-200" }
                                        "Gold"
                                    }
                                    button {
                                        class: screenshot_picker_pill_class(
                                            screenshot_text_palette_active == ScreenshotTextPalette::Cyan,
                                        ),
                                        onclick: {
                                            let mut screenshot_shot_text_palette = screenshot_shot_text_palette.clone();
                                            move |_| screenshot_shot_text_palette.set(ScreenshotTextPalette::Cyan)
                                        },
                                        span { class: "h-3 w-3 rounded-full bg-cyan-200" }
                                        "Cyan"
                                    }
                                }
                            }
                            div { class: "mt-4 space-y-2",
                                p { class: "text-[11px] uppercase tracking-[0.22em] text-white/45",
                                    "Background"
                                }
                                div { class: "flex flex-wrap gap-2",
                                    button {
                                        class: if screenshot_shot_theme_active == ScreenshotShotTheme::Lagoon { "inline-flex items-center gap-2 rounded-full border border-white/35 bg-white/12 px-3 py-2 text-sm text-white" } else { "inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/5 px-3 py-2 text-sm text-white/72 hover:text-white hover:border-white/25 transition-colors" },
                                        onclick: {
                                            let mut screenshot_shot_theme = screenshot_shot_theme.clone();
                                            let mut app_settings = app_settings.clone();
                                            move |_| {
                                                screenshot_shot_theme.set(ScreenshotShotTheme::Lagoon);
                                                let mut settings = app_settings();
                                                settings.lyrics_default_theme = "lagoon".to_string();
                                                app_settings.set(settings.clone());
                                                spawn(async move {
                                                    let _ = crate::db::save_settings(settings).await;
                                                });
                                            }
                                        },
                                        span { class: "h-3 w-3 rounded-full bg-[#62bac9]" }
                                        "Lagoon"
                                    }
                                    button {
                                        class: if screenshot_shot_theme_active == ScreenshotShotTheme::Ember { "inline-flex items-center gap-2 rounded-full border border-white/35 bg-white/12 px-3 py-2 text-sm text-white" } else { "inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/5 px-3 py-2 text-sm text-white/72 hover:text-white hover:border-white/25 transition-colors" },
                                        onclick: {
                                            let mut screenshot_shot_theme = screenshot_shot_theme.clone();
                                            let mut app_settings = app_settings.clone();
                                            move |_| {
                                                screenshot_shot_theme.set(ScreenshotShotTheme::Ember);
                                                let mut settings = app_settings();
                                                settings.lyrics_default_theme = "ember".to_string();
                                                app_settings.set(settings.clone());
                                                spawn(async move {
                                                    let _ = crate::db::save_settings(settings).await;
                                                });
                                            }
                                        },
                                        span { class: "h-3 w-3 rounded-full bg-[#df8a71]" }
                                        "Ember"
                                    }
                                    button {
                                        class: if screenshot_shot_theme_active == ScreenshotShotTheme::Midnight { "inline-flex items-center gap-2 rounded-full border border-white/35 bg-white/12 px-3 py-2 text-sm text-white" } else { "inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/5 px-3 py-2 text-sm text-white/72 hover:text-white hover:border-white/25 transition-colors" },
                                        onclick: {
                                            let mut screenshot_shot_theme = screenshot_shot_theme.clone();
                                            let mut app_settings = app_settings.clone();
                                            move |_| {
                                                screenshot_shot_theme.set(ScreenshotShotTheme::Midnight);
                                                let mut settings = app_settings();
                                                settings.lyrics_default_theme = "midnight".to_string();
                                                app_settings.set(settings.clone());
                                                spawn(async move {
                                                    let _ = crate::db::save_settings(settings).await;
                                                });
                                            }
                                        },
                                        span { class: "h-3 w-3 rounded-full bg-[#1f2a44]" }
                                        "Midnight"
                                    }
                                    if screenshot_cover_url.is_some() {
                                        button {
                                            class: if screenshot_shot_theme_active == ScreenshotShotTheme::Cover { "inline-flex items-center gap-2 rounded-full border border-white/35 bg-white/12 px-3 py-2 text-sm text-white" } else { "inline-flex items-center gap-2 rounded-full border border-white/15 bg-white/5 px-3 py-2 text-sm text-white/72 hover:text-white hover:border-white/25 transition-colors" },
                                            onclick: {
                                                let mut screenshot_shot_theme = screenshot_shot_theme.clone();
                                                let mut app_settings = app_settings.clone();
                                                move |_| {
                                                    screenshot_shot_theme.set(ScreenshotShotTheme::Cover);
                                                    let mut settings = app_settings();
                                                    settings.lyrics_default_theme = "cover".to_string();
                                                    app_settings.set(settings.clone());
                                                    spawn(async move {
                                                        let _ = crate::db::save_settings(settings).await;
                                                    });
                                                }
                                            },
                                            if let Some(url) = screenshot_cover_url.clone() {
                                                img {
                                                    class: "h-4 w-4 rounded object-cover",
                                                    src: "{url}",
                                                    alt: "Album art",
                                                }
                                            }
                                            "Cover"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        class: "flex h-full w-full flex-col",
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        div { class: "relative flex-1 overflow-hidden bg-zinc-950 shadow-[0_40px_120px_rgba(0,0,0,0.65)]",
                            if let Some(url) = screenshot_cover_url.clone() {
                                img {
                                    class: "absolute inset-0 h-full w-full object-cover scale-110 opacity-35",
                                    style: "{screenshot_main_backdrop_cover_style}",
                                    src: "{url}",
                                    alt: "{screenshot_song_title}",
                                }
                            }
                            div { class: "{main_backdrop_overlay_class}" }
                            if screenshot_shot_mode_enabled {
                                div { class: "relative z-10 flex h-full min-h-0 w-full items-center justify-center px-4 pb-6 pt-16 md:px-8 md:pb-10 md:pt-20",
                                    div {
                                        id: "{screenshot_shot_card_id}",
                                        class: "relative aspect-square overflow-hidden rounded-[2rem] border border-white/14 shadow-[0_28px_90px_rgba(0,0,0,0.35)]",
                                        style: "{screenshot_shot_card_style}",
                                        if screenshot_shot_theme_active == ScreenshotShotTheme::Cover {
                                            if let Some(url) = screenshot_cover_url.clone() {
                                                img {
                                                    class: "absolute inset-0 h-full w-full object-cover scale-[1.18] opacity-65",
                                                    style: "{screenshot_card_backdrop_cover_style}",
                                                    src: "{url}",
                                                    alt: "{screenshot_song_title}",
                                                }
                                            }
                                        }
                                        div { class: "{screenshot_shot_card_overlay_class}" }
                                        div { class: "relative flex h-full flex-col p-5 md:p-6",
                                            div { class: "flex items-start gap-3",
                                                if let Some(url) = screenshot_cover_url.clone() {
                                                    img {
                                                        class: "h-14 w-14 rounded-2xl object-cover shadow-lg md:h-16 md:w-16",
                                                        src: "{url}",
                                                        alt: "{screenshot_song_title}",
                                                    }
                                                } else {
                                                    div {
                                                        class: "flex h-14 w-14 items-center justify-center rounded-2xl md:h-16 md:w-16",
                                                        style: "{screenshot_shot_fallback_cover_style}",
                                                        Icon {
                                                            name: "music".to_string(),
                                                            class: "h-7 w-7".to_string(),
                                                        }
                                                    }
                                                }
                                                div { class: "min-w-0 flex-1",
                                                    p {
                                                        class: "truncate text-2xl font-semibold leading-tight md:text-[2rem]",
                                                        style: "{screenshot_shot_primary_text_style}",
                                                        "{screenshot_song_title}"
                                                    }
                                                    if let Some(artist) = screenshot_song_artist.clone() {
                                                        p {
                                                            class: "truncate text-lg font-medium md:text-[1.35rem]",
                                                            style: "{screenshot_shot_secondary_text_style}",
                                                            "{artist}"
                                                        }
                                                    }
                                                }
                                            }
                                            div { class: "flex flex-1 items-center py-5 md:py-6",
                                                if screenshot_selected_bars.is_empty() {
                                                    p {
                                                        class: "text-xl font-semibold",
                                                        style: "{screenshot_shot_footer_primary_text_style}",
                                                        "Lyrics unavailable."
                                                    }
                                                } else {
                                                    div { class: "w-full {screenshot_share_spacing_class}",
                                                        for bar in screenshot_selected_bars.iter() {
                                                            p {
                                                                class: "{screenshot_share_lyrics_class}",
                                                                style: "{screenshot_share_lyrics_style}",
                                                                "{bar.text}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            div { class: "flex items-center gap-3 pt-3",
                                                div { class: "flex items-center gap-3",
                                                    img {
                                                        class: "h-8 w-8 rounded-lg",
                                                        src: RUSTYSOUND_MARK,
                                                        alt: "RustySound",
                                                    }
                                                    div {
                                                        p {
                                                            class: "text-sm font-semibold uppercase tracking-[0.22em]",
                                                            style: "{screenshot_shot_footer_primary_text_style}",
                                                            "RustySound"
                                                        }
                                                        p {
                                                            class: "text-xs",
                                                            style: "{screenshot_shot_footer_secondary_text_style}",
                                                            "Shared lyrics"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "relative z-10 mx-auto flex h-full min-h-0 w-full {screenshot_browser_width_class} flex-col px-6 pb-8 pt-24 md:px-12 md:pb-10 md:pt-28",
                                    div { class: "space-y-1 md:max-w-[70%]",
                                        h3 { class: "text-2xl md:text-4xl font-semibold leading-tight text-white",
                                            "{screenshot_song_title}"
                                        }
                                        if let Some(artist) = screenshot_song_artist.clone() {
                                            p { class: "text-sm md:text-base text-white/70",
                                                "{artist}"
                                            }
                                        }
                                    }
                                    div {
                                        id: "{screenshot_scroll_container_id}",
                                        class: "mt-8 flex-1 overflow-y-auto pr-2 md:mt-10",
                                        if screenshot_bars.is_empty() {
                                            p { class: "text-lg text-white/70", "Lyrics unavailable." }
                                        } else {
                                            div { class: "max-w-4xl space-y-4 pb-24 md:space-y-5 md:pb-28",
                                                for (index , bar) in screenshot_bars.iter().enumerate() {
                                                    button {
                                                        id: format!("{screenshot_scroll_container_id}-line-{index}"),
                                                        class: if index >= screenshot_selected_start && index <= screenshot_selected_end { screenshot_selected_line_class } else { screenshot_unselected_line_class },
                                                        onclick: {
                                                            let active_synced_index = active_synced_index;
                                                            let mut screenshot_selection_start = screenshot_selection_start.clone();
                                                            let mut screenshot_selection_count = screenshot_selection_count.clone();
                                                            let mut screenshot_manual_selection = screenshot_manual_selection.clone();
                                                            move |_| {
                                                                if screenshot_manual_selection()
                                                                    && index >= screenshot_selected_start
                                                                    && index <= screenshot_selected_end
                                                                {
                                                                    screenshot_manual_selection.set(false);
                                                                    screenshot_selection_count.set(1);
                                                                    if let Some(active_index) = active_synced_index {
                                                                        screenshot_selection_start.set(active_index);
                                                                    }
                                                                    return;
                                                                }

                                                                screenshot_manual_selection.set(true);
                                                                if index >= screenshot_selected_start
                                                                    && index - screenshot_selected_start < 5
                                                                {
                                                                    screenshot_selection_count.set(index - screenshot_selected_start + 1);
                                                                } else {
                                                                    screenshot_selection_start.set(index);
                                                                    screenshot_selection_count.set(1);
                                                                }
                                                            }
                                                        },
                                                        "{screenshot_bar_label(bar, screenshot_show_timestamps)}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
