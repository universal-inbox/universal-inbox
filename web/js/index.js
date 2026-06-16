import { Crisp } from "crisp-sdk-web";

import "flyonui/dist/collapse";
import "flyonui/dist/tabs";
import "flyonui/dist/overlay";
import "flyonui/dist/tooltip";
import flatpickr from "flatpickr";
export { flatpickr };

// Mount sandboxed email-body iframes (.ui-email-frame-host) entirely from JS.
// Dioxus only renders a `<div class="ui-email-frame-host" data-html="…">`
// placeholder; this code creates the actual `<iframe>` and sets its srcdoc /
// sandbox / load handler. Keeping the heavy srcdoc string off the Dioxus VDOM
// avoids a `Dropped(ValueDroppedError)` panic in long Gmail threads (20+
// messages) where re-rendering many large `srcdoc` attributes raced with
// Dioxus' Callback machinery.
// Font defaults inside the iframe match the rest of the app (DM Sans). They're
// applied at the lowest cascade specificity (`html, body` selectors with no
// !important) so any email that specifies its own font — via inline `style`
// attributes on tables/cells, `<font face=…>`, or its own `<style>` block —
// overrides them. The default text color tracks the parent app theme so
// unstyled emails stay readable in both light and dark modes; emails that
// declare their own color win via the same low-specificity cascade.
// HTML emails are almost always authored for a light (white) background — they
// set dark text colors (e.g. color:#333) but often declare no background of
// their own, relying on the mail client's default white. On the dark app theme
// that dark text lands on the dark canvas and disappears. `applyCanvasTheme`
// (below) handles this after load: emails that DO declare their own opaque
// background are trusted as-authored; background-less emails get a dark-mode
// contrast pass that lifts only their unreadable (dark-on-dark) text to a
// readable lightness while preserving hue.
const EMAIL_FRAME_FOOT = "</body></html>";

function isDarkTheme() {
    return document.documentElement.getAttribute("data-theme") === "dark";
}

function buildEmailFrameHead(dark) {
    // Mirror the app theme tokens (see web/css/universal-inbox.css):
    //   text   → --ui-base text color, surface → --ui-surface, border → --ui-border.
    const textColor = dark ? "#e2e8f0" : "#0f172a";
    const surfaceColor = dark ? "#111827" : "#ffffff";
    const borderColor = dark ? "#1e293b" : "#e2e8f0";
    const colorScheme = dark ? "dark" : "light";
    return (
        '<!doctype html><html><head><meta charset="utf-8">' +
        '<base target="_blank">' +
        "<style>" +
        "@font-face{font-family:'DM Sans';font-style:normal;font-weight:100 1000;" +
        "font-display:swap;src:url('/fonts/DMSans-Regular.woff2') format('woff2');}" +
        "html{color-scheme:" + colorScheme + ";}" +
        // The root paints the iframe canvas — its background covers the entire
        // scrollable area, including horizontal overflow. Filling it with the
        // app surface color keeps the background continuous behind a wide email
        // (the body stays transparent so an email's own background still wins
        // over the surface within its own width).
        "html{background:" + surfaceColor + ";}" +
        "html,body{margin:0;padding:0;color:" + textColor + ";" +
        "font-family:'DM Sans',system-ui,-apple-system,'Segoe UI',Roboto," +
        "'Helvetica Neue',Arial,sans-serif;font-size:14px;line-height:1.55;" +
        "-webkit-font-smoothing:antialiased;}" +
        "body{background:transparent;}" +
        // Emails with fixed-width layouts (wide tables, hard pixel widths) can be
        // wider than the preview pane. `overflow-x:auto` on the root gives the
        // user a horizontal scrollbar to reach the full content instead of
        // silently clipping it. `overflow-y:hidden` keeps height auto-sized by
        // the resize handler (the preview pane itself scrolls vertically).
        "html{overflow-x:auto;overflow-y:hidden;}img{max-width:100%;height:auto;}" +
        // Match the preview pane's custom scrollbar (#notification-preview-details
        // in the app CSS): thin, transparent track, --ui-border thumb, 3px radius —
        // so the email's horizontal scrollbar aligns with the pane's vertical one.
        "html{scrollbar-width:thin;scrollbar-color:" + borderColor + " transparent;}" +
        "html::-webkit-scrollbar{width:5px;height:5px;}" +
        "html::-webkit-scrollbar-track{background:transparent;}" +
        "html::-webkit-scrollbar-thumb{background:" + borderColor + ";border-radius:3px;}" +
        "</style>" +
        "</head><body>"
    );
}

function buildEmailIframe(host) {
    const html = host.dataset.html || "";
    const srcdoc = buildEmailFrameHead(isDarkTheme()) + html + EMAIL_FRAME_FOOT;

    // Re-render path: when Dioxus reuses the host div for a new notification,
    // it just rewrites `data-html`. Keep the existing iframe and swap its
    // srcdoc so we don't lose the load handler or flash a fresh reflow.
    const existing = host.querySelector(":scope > iframe.ui-email-frame");
    if (existing) {
        if (existing.srcdoc !== srcdoc) {
            existing.srcdoc = srcdoc;
        }
        return;
    }

    host.dataset.uiEmailFrameMounted = "true";
    const iframe = document.createElement("iframe");
    iframe.className = "ui-email-frame";
    iframe.setAttribute(
        "sandbox",
        "allow-same-origin allow-popups allow-popups-to-escape-sandbox",
    );
    iframe.setAttribute("referrerpolicy", "no-referrer");
    iframe.setAttribute("loading", "lazy");
    iframe.srcdoc = srcdoc;

    const resize = () => {
        try {
            const root = iframe.contentDocument?.documentElement;
            if (root) {
                let height = root.scrollHeight;
                // When the email overflows horizontally, a scrollbar sits at the
                // bottom of the iframe viewport and would overlap the last rows of
                // content (the iframe has no vertical scroll — its height tracks
                // the content). Reserve the scrollbar's height so it clears the body.
                if (root.scrollWidth > root.clientWidth) {
                    const scrollbar =
                        (iframe.contentWindow?.innerHeight ?? 0) - root.clientHeight;
                    height += scrollbar > 0 ? scrollbar : 16;
                }
                iframe.style.height = height + "px";
            }
        } catch (_) {
            // Cross-origin frames or detached iframes — ignore.
        }
    };

    // Relative luminance (0–255 scale) of a CSS `rgb()/rgba()` color string, or
    // null if it can't be parsed. Used both to read a readable text color off a
    // background and to decide whether the email's own text is too dark to read
    // on the dark canvas.
    const luminance = (color) => {
        const rgb = color && color.match(/\d+(\.\d+)?/g);
        if (!rgb || rgb.length < 3) return null;
        const [r, g, b] = rgb.map(Number);
        return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    // Lift a too-dark color to a readable lightness for the dark canvas while
    // keeping its hue and saturation — so grayscale body text becomes light gray
    // and a dark-orange link stays orange but legible, instead of flattening
    // every fixed color to one gray. Returns a CSS `rgb(...)` string.
    const lightenForDark = (color) => {
        const rgb = color && color.match(/\d+(\.\d+)?/g);
        if (!rgb || rgb.length < 3) return "#e2e8f0";
        let [r, g, b] = rgb.map(Number).map((v) => v / 255);
        const max = Math.max(r, g, b);
        const min = Math.min(r, g, b);
        let h = 0;
        const d = max - min;
        const l = (max + min) / 2;
        const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
        if (d !== 0) {
            if (max === r) h = ((g - b) / d) % 6;
            else if (max === g) h = (b - r) / d + 2;
            else h = (r - g) / d + 4;
            h *= 60;
            if (h < 0) h += 360;
        }
        const targetL = Math.max(l, 0.8); // floor lightness so it reads on dark
        const c = (1 - Math.abs(2 * targetL - 1)) * s;
        const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
        const m = targetL - c / 2;
        let rr = 0,
            gg = 0,
            bb = 0;
        if (h < 60) [rr, gg, bb] = [c, x, 0];
        else if (h < 120) [rr, gg, bb] = [x, c, 0];
        else if (h < 180) [rr, gg, bb] = [0, c, x];
        else if (h < 240) [rr, gg, bb] = [0, x, c];
        else if (h < 300) [rr, gg, bb] = [x, 0, c];
        else [rr, gg, bb] = [c, 0, x];
        const to = (v) => Math.round((v + m) * 255);
        return `rgb(${to(rr)}, ${to(gg)}, ${to(bb)})`;
    };

    // The `buildEmailFrameHead` fallback fills the canvas with the app surface so
    // unstyled emails read on theme. But most HTML emails wrap their content in a
    // bgcolor'd table (commonly white) that's narrower than a wide, overflowing
    // layout — leaving the app surface exposed in the horizontal-scroll gap (e.g.
    // a white email on the dark-theme surface). Detect the email's own background
    // and paint the canvas with it so the background extends across the full
    // content width, then pick a readable default text color from its luminance
    // (so a light email stays dark-on-light even under the dark app theme).
    //
    // Emails that declare NO opaque background are a special case: they were
    // authored for the client's default white, so their dark text disappears on
    // the dark canvas. We keep the dark canvas and run a contrast pass that lifts
    // only the text that's actually unreadable (dark-on-dark) to a readable
    // lightness, leaving self-styled "light island" content (elements with their
    // own opaque background) and already-light text untouched.
    const applyCanvasTheme = () => {
        try {
            const doc = iframe.contentDocument;
            if (!doc || !doc.body) return;
            const opaque = (c) => !!c && c !== "transparent" && c !== "rgba(0, 0, 0, 0)";
            const root = doc.documentElement;
            let bg = getComputedStyle(doc.body).backgroundColor;
            if (!opaque(bg)) {
                // Fall back to the widest opaque top-level block — the email's
                // main background-bearing container.
                let widest = 0;
                for (const el of doc.body.children) {
                    const c = getComputedStyle(el).backgroundColor;
                    if (opaque(c) && el.scrollWidth > widest) {
                        widest = el.scrollWidth;
                        bg = c;
                    }
                }
            }
            if (opaque(bg)) {
                // Self-styled email — honor its background and pick a readable
                // default text color from it. Trust the email's own colors.
                root.style.background = bg;
                const l = luminance(bg);
                if (l != null) {
                    const textColor = l > 140 ? "#0f172a" : "#e2e8f0";
                    root.style.color = textColor;
                    doc.body.style.color = textColor;
                }
                return;
            }

            // Background-less email. In light mode the app-surface fallback is
            // light, so the email's dark text already reads — nothing to do.
            if (!isDarkTheme()) return;

            // Dark mode + no declared background: the canvas stays the dark app
            // surface but the email's text was authored for white. Lift only the
            // unreadable (dark) text to a readable lightness, preserving hue.
            const hasOpaqueBgAncestor = (el) => {
                for (let n = el.parentElement; n && n !== root; n = n.parentElement) {
                    if (opaque(getComputedStyle(n).backgroundColor)) return true;
                }
                return false;
            };
            const walker = doc.createTreeWalker(doc.body, NodeFilter.SHOW_ELEMENT);
            let node = walker.currentNode;
            while (node) {
                const hasOwnText = Array.from(node.childNodes).some(
                    (n) => n.nodeType === 3 && n.textContent.trim() !== "",
                );
                if (hasOwnText && !hasOpaqueBgAncestor(node)) {
                    const l = luminance(getComputedStyle(node).color);
                    if (l != null && l < 140) {
                        node.style.setProperty(
                            "color",
                            lightenForDark(getComputedStyle(node).color),
                            "important",
                        );
                    }
                }
                node = walker.nextNode();
            }
        } catch (_) {
            // Cross-origin frames or detached iframes — ignore.
        }
    };
    // Do NOT call `resize` synchronously after `appendChild`: at that point
    // the iframe still hosts an empty `about:blank` document (which already
    // reports `readyState === "complete"`), so measuring its `scrollHeight`
    // would set the iframe to the empty-iframe default (~150px in Chromium,
    // ~8px in Firefox) and the user sees a collapsed frame until the real
    // `srcdoc` finishes parsing. The `load` event below fires once the
    // `srcdoc` content has actually loaded; the listener is attached before
    // `appendChild`, so it cannot have already fired.
    iframe.addEventListener("load", () => {
        applyCanvasTheme();
        resize();
    });
    host.appendChild(iframe);
}

if (typeof window !== "undefined" && typeof MutationObserver !== "undefined") {
    const scan = (root) => {
        if (!root || !root.querySelectorAll) return;
        if (root.matches?.(".ui-email-frame-host")) {
            buildEmailIframe(root);
        }
        root.querySelectorAll?.(".ui-email-frame-host").forEach(
            buildEmailIframe,
        );
    };
    const observer = new MutationObserver((records) => {
        for (const r of records) {
            if (r.type === "attributes") {
                if (
                    r.target.nodeType === 1 &&
                    r.target.matches?.(".ui-email-frame-host")
                ) {
                    buildEmailIframe(r.target);
                }
            } else {
                r.addedNodes?.forEach((n) => {
                    if (n.nodeType === 1) scan(n);
                });
            }
        }
    });
    const refreshAllIframes = () => {
        document
            .querySelectorAll(".ui-email-frame-host")
            .forEach(buildEmailIframe);
    };
    const themeObserver = new MutationObserver(refreshAllIframes);
    const start = () => {
        scan(document.body);
        observer.observe(document.body, {
            childList: true,
            subtree: true,
            attributes: true,
            attributeFilter: ["data-html"],
        });
        themeObserver.observe(document.documentElement, {
            attributes: true,
            attributeFilter: ["data-theme"],
        });
    };
    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", start);
    } else {
        start();
    }
}

// Flyonui collapse component hooks
export function init_flyonui_collapse_element(element) {
    if (typeof window.$hsCollapseCollection === "object") {
        if (
            element &&
            !window.$hsCollapseCollection.find(
                (el) => el?.element?.el === element,
            )
        ) {
            new HSCollapse(element);
        }
    }
}

export function forget_flyonui_collapse_element(element) {
    if (typeof window.$hsCollapseCollection === "object") {
        window.$hsCollapseCollection = window.$hsCollapseCollection.filter(
            (el) => el?.element?.el !== element,
        );
    }
}

// Flyonui tabs component hooks
export function init_flyonui_tabs_element(element) {
    if (typeof window.$hsTabsCollection === "object") {
        if (
            element &&
            !window.$hsTabsCollection.find((el) => el?.element?.el === element)
        ) {
            new HSTabs(element);
        }
    }
}

export function forget_flyonui_tabs_element(element) {
    if (typeof window.$hsTabsCollection === "object") {
        window.$hsTabsCollection = window.$hsTabsCollection.filter(
            (el) => el?.element?.el !== element,
        );
    }
}

// Flyonui modal component hooks
export function init_flyonui_modal(element) {
    if (typeof window.$hsOverlayCollection === "object") {
        if (
            element &&
            !window.$hsOverlayCollection.find(
                (el) => el?.element?.el === element,
            )
        ) {
            new HSOverlay(element);
        }
    }
}

export function forget_flyonui_modal(element) {
    if (typeof window.$hsOverlayCollection === "object") {
        window.$hsOverlayCollection = window.$hsOverlayCollection.filter(
            (el) => el?.element?.el !== element,
        );
    }
}

export function open_flyonui_modal(target) {
    HSOverlay.open(target);
}

export function close_flyonui_modal(target) {
    HSOverlay.close(target);
}

export function has_flyonui_modal_opened() {
    if (typeof window.$hsOverlayCollection === "object") {
        return (
            window.$hsOverlayCollection.filter(
                (el) =>
                    !el?.element?.el.classList.contains(
                        el?.element?.hiddenClass,
                    ),
            ).length > 0
        );
    }
}

// Flyonui tooltip component hooks
export function init_flyonui_tooltip_element(element) {
    if (!element) return;
    // HSTooltip.autoInit() seeds $hsTooltipCollection on window.load. If a
    // Dioxus component mounts before that fires, the array is undefined and
    // HSTooltip's constructor would crash. Seed it ourselves to be safe.
    if (!Array.isArray(window.$hsTooltipCollection)) {
        window.$hsTooltipCollection = [];
    }
    if (
        !window.$hsTooltipCollection.find((el) => el?.element?.el === element)
    ) {
        new HSTooltip(element);
    }
}

export function forget_flyonui_tooltip_element(element) {
    if (!element || !Array.isArray(window.$hsTooltipCollection)) return;
    const entry = window.$hsTooltipCollection.find(
        (el) => el?.element?.el === element,
    );
    if (entry?.element?.destroy) {
        entry.element.destroy();
    } else {
        window.$hsTooltipCollection = window.$hsTooltipCollection.filter(
            (el) => el?.element?.el !== element,
        );
    }
}

export function init_headway() {
    if (typeof Headway === "object") {
        Headway.init({
            selector: "#ui-changelog",
            account: "7Xr08y",
        });
    }
}

export function show_headway() {
    if (typeof Headway === "object") {
        // Defer to the next tick so the click that triggered this finishes
        // bubbling first. Headway attaches a document-level click handler to
        // close the popin on outside clicks; opening synchronously from a click
        // handler races that handler (it closes the popin we just opened, so it
        // never appears). By the time this timeout fires, the click has settled
        // and Headway's close handler has already run as a no-op.
        setTimeout(() => {
            try {
                Headway.show();
            } catch (e) {
                console.warn("Failed to show Headway changelog:", e);
            }
        }, 0);
    }
}

export function init_crisp(
    website_id,
    user_email,
    user_email_signature,
    user_nickname,
    user_avatar,
    user_id,
) {
    try {
        Crisp.configure(website_id, {
            autoload: false,
            sessionMerge: true,
        });
        if (!!user_id) {
            Crisp.setTokenId(user_id);
        }
        if (!!user_email) {
            Crisp.user.setEmail(user_email, user_email_signature);
        }
        if (!!user_nickname) {
            Crisp.user.setNickname(user_nickname);
        }
        if (!!user_avatar) {
            Crisp.user.setAvatar(user_avatar);
        }

        Crisp.load();

        if (!!user_id) {
            Crisp.session.setData({
                user_id: user_id,
            });
        }

        // Hide Crisp's default floating launcher — it overlaps the notification
        // preview pane's action buttons (bottom-right). The chat is opened from a
        // dedicated "Support" button in the sidebar via `open_crisp_chat()`.
        // Re-hide whenever the user closes the chat so no bubble lingers.
        Crisp.chat.hide();
        Crisp.chat.onChatClosed(() => {
            Crisp.chat.hide();
        });
    } catch (e) {
        console.warn("Failed to initialize Crisp chat:", e);
    }
}

export function open_crisp_chat() {
    if (typeof Crisp === "undefined") {
        return;
    }
    // Defer to the next tick so the click that triggered this finishes bubbling
    // first. Crisp closes the chat on outside clicks; opening synchronously from
    // a click handler races that handler, which closes the chat we just opened
    // (it flashes open then shut on the first click). By the time this timeout
    // fires, the click has settled. The default launcher is hidden (see
    // `init_crisp`), so show the widget then open the conversation window.
    setTimeout(() => {
        try {
            Crisp.chat.show();
            Crisp.chat.open();
        } catch (e) {
            console.warn("Failed to open Crisp chat:", e);
        }
    }, 0);
}

export function unload_crisp() {
    try {
        Crisp.setTokenId();
        Crisp.session.reset();
    } catch (e) {
        console.warn("Failed to unload Crisp chat:", e);
    }
}

export function is_crisp_chat_opened() {
    try {
        return Crisp.chat.isChatOpened();
    } catch (e) {
        console.warn("Failed to check Crisp chat state:", e);
        return false;
    }
}
