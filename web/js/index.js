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
const EMAIL_FRAME_FOOT = "</body></html>";

function isDarkTheme() {
    return document.documentElement.getAttribute("data-theme") === "dark";
}

function buildEmailFrameHead(dark) {
    const textColor = dark ? "#e2e8f0" : "#0f172a";
    const colorScheme = dark ? "dark" : "light";
    return (
        '<!doctype html><html><head><meta charset="utf-8">' +
        '<base target="_blank">' +
        "<style>" +
        "@font-face{font-family:'DM Sans';font-style:normal;font-weight:100 1000;" +
        "font-display:swap;src:url('/fonts/DMSans-Regular.woff2') format('woff2');}" +
        "html{color-scheme:" + colorScheme + ";}" +
        "html,body{margin:0;padding:0;background:transparent;color:" + textColor + ";" +
        "font-family:'DM Sans',system-ui,-apple-system,'Segoe UI',Roboto," +
        "'Helvetica Neue',Arial,sans-serif;font-size:14px;line-height:1.55;" +
        "-webkit-font-smoothing:antialiased;}" +
        "body{overflow:hidden;}img{max-width:100%;height:auto;}" +
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
                iframe.style.height = root.scrollHeight + "px";
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
    iframe.addEventListener("load", resize);
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
        Headway.show();
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
    } catch (e) {
        console.warn("Failed to initialize Crisp chat:", e);
    }
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
