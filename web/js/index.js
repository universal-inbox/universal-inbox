import { Crisp } from "crisp-sdk-web";
import Nango from "@nangohq/frontend";

export function auth_provider(
    nangoHost,
    publicKey,
    configKey,
    connectionId,
    oauthUserScopes,
) {
    return new Nango({
        host: nangoHost,
        publicKey: publicKey,
        debug: true,
    }).auth(configKey, connectionId, { user_scope: oauthUserScopes });
}

import "flyonui/dist/dropdown";
import "flyonui/dist/collapse";
import "flyonui/dist/tabs";
import "flyonui/dist/overlay";
import "flyonui/dist/select";
import "flyonui/dist/tooltip";
import flatpickr from "flatpickr";
export { flatpickr };

// Flyonui dropdown component hooks
export function init_flyonui_dropdown_element(element) {
    if (typeof window.$hsDropdownCollection === "object") {
        if (
            element &&
            !window.$hsDropdownCollection.find(
                (el) => el?.element?.el === element,
            )
        ) {
            new HSDropdown(element);
        }
    }
}

export function forget_flyonui_dropdown_element(element) {
    if (typeof window.$hsDropdownCollection === "object") {
        window.$hsDropdownCollection = window.$hsDropdownCollection.filter(
            (el) => el?.element?.el !== element,
        );
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

// Flyonui select component hooks
export function init_flyonui_select_element(element) {
    if (typeof window.$hsSelectCollection === "object") {
        if (
            element &&
            !window.$hsSelectCollection.find(
                (el) => el?.element?.el === element,
            )
        ) {
            new HSSelect(element);
        }
    }
}

export function forget_flyonui_select_element(element) {
    if (typeof window.$hsSelectCollection === "object") {
        window.$hsSelectCollection = window.$hsSelectCollection.filter(
            (el) => el?.element?.el !== element,
        );
    }
}

export function destroy_flyonui_select_element(element) {
    const select_element = HSSelect.getInstance(element);
    if (select_element) {
        select_element.destroy();
    }
}

export function get_flyonui_selected_remote_value(element) {
    const select_element = HSSelect.getInstance(element);
    if (select_element) {
        const val_field = select_element.apiFieldsMap.val;
        const selected_value = select_element.value;
        return select_element.remoteOptions.find(
            (opt) => opt[val_field] == selected_value,
        );
    }
}

// Flyonui tooltip component hooks
export function init_flyonui_tooltip_element(element) {
    if (typeof window.$hsTooltipCollection === "object") {
        if (
            element &&
            !window.$hsTooltipCollection.find(
                (el) => el?.element?.el === element,
            )
        ) {
            new HSTooltip(element);
        }
    }
}

export function forget_flyonui_tooltip_element(element) {
    if (typeof window.$hsTooltipCollection === "object") {
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
}

export function unload_crisp() {
    Crisp.setTokenId();
    Crisp.session.reset();
}

export function is_crisp_chat_opened() {
    return Crisp.chat.isChatOpened();
}
