import QtQuick 2.15
import QtTest 1.2
import QtQuick.Window 2.15
import Envio 1.0
import "../../qml/platform"
import "../../qml/pages/settings"

Item {
    id: root
    width: 400
    height: 700

    MockCore { id: core }
    // As Main binds it: the look follows the core's settings at once.
    Theme {
        id: thm
        name: core.theme
        darkMode: core.darkMode
    }
    AppearancePage { id: appearance; theme: thm; core: core; fullscreenAvailable: true; anchors.fill: parent; visible: false }
    // The page where the platform has no fullscreen (the desktop).
    AppearancePage { id: appearanceDesktop; theme: thm; core: core; anchors.fill: parent; visible: false }
    // The window's visibility, through the platform hook (a stand-in for ut/).
    WindowMode { id: mode; source: Qt.resolvedUrl("StubFullscreen.qml"); fullscreen: core.fullscreen }
    // Where the platform file does not load (the desktop: no Suru).
    WindowMode { id: desktopMode; source: Qt.resolvedUrl("NoSuchPlatform.qml"); fullscreen: core.fullscreen }
    ImagesPage { id: images; theme: thm; core: core; anchors.fill: parent; visible: false }
    SyncPage { id: sync; theme: thm; core: core; accounts: core.accounts; anchors.fill: parent; visible: false }
    AboutPage { id: about; theme: thm; anchors.fill: parent; visible: false }
    SettingsPage { id: settings; theme: thm; core: core; anchors.fill: parent; visible: false }
    SignalSpy { id: toAppearance; target: settings; signalName: "appearanceRequested" }
    SignalSpy { id: toSync; target: settings; signalName: "syncRequested" }
    SignalSpy { id: toNotifications; target: settings; signalName: "notificationsRequested" }
    SignalSpy { id: toImages; target: settings; signalName: "imagesRequested" }
    SignalSpy { id: toAccounts; target: settings; signalName: "accountsRequested" }
    SignalSpy { id: toAbout; target: settings; signalName: "aboutRequested" }

    TestCase {
        name: "Settings"
        when: windowShown

        function everything(item, out) {
            out.push(item)
            if (item.contentItem !== undefined && item.contentItem !== null && item.contentItem !== item)
                everything(item.contentItem, out)
            for (const prop of ["data", "children"]) {
                if (item[prop] === undefined) continue
                for (let i = 0; i < item[prop].length; i++) everything(item[prop][i], out)
                break
            }
            return out
        }
        function all(name, from) {
            // A Flickable's data is its contentItem's: dedupe the two paths.
            return everything(from, []).filter(function(o, i, list) {
                return o.objectName === name && list.indexOf(o) === i
            })
        }
        function find(name, from) {
            const hit = all(name, from)
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function test_settings_rows_each_show_a_chevron() {
            settings.visible = true
            for (const key of ["appearance", "sync", "notifications", "images", "accounts", "about"]) {
                const c = find("chevron", find("row-" + key, settings))
                verify(c.visible, key + " has a chevron")
                verify(c.width > 0)
            }
            settings.visible = false
        }
        function test_settings_chevron_is_secondary_in_every_look() {
            settings.visible = true
            for (const look of ["carbon", "airmail"]) {
                for (const dark of ["light", "dark"]) {
                    core.theme = look
                    core.darkMode = dark
                    compare(find("chevron", find("row-sync", settings)).color, thm.secondary, look + " " + dark)
                }
            }
            settings.visible = false
        }
        function test_shortcuts_row_opens_dialog() {
            settings.visible = true
            const dialog = settings.shortcuts
            verify(!dialog.opened)
            mouseClick(find("shortcutsRow", settings))
            tryVerify(function() { return dialog.opened })
            // Six rows, each a sentence and its key chips.
            compare(dialog.rows.length, 6)
            compare(dialog.rows[3].text, "New message")
            compare(dialog.rows[3].keys, ["Ctrl", "N"])
            compare(dialog.rows[5].keys, ["Esc"])
            compare(all("shortcutRow", dialog.contentItem).length, 6)
            verify(all("keyChip", dialog.contentItem).length >= 8)
            // Close is the outlined action; there is no filled one.
            verify(!find("primaryAction", dialog.contentItem).visible)
            const close = find("secondaryAction", dialog.contentItem)
            verify(close.visible)
            // Design review: rules under the rows, a compact Close at the right,
            // a narrow centred card.
            compare(all("shortcutRule", dialog.contentItem).length, 6)
            verify(close.width < dialog.contentItem.width / 2)
            compare(close.x + close.width, dialog.contentItem.width)
            verify(dialog.width <= thm.gu(45) + 1)
            verify(Math.abs(dialog.x - (dialog.parent.width - dialog.width) / 2) < 1)
            dialog.close()
            settings.visible = false
        }
        function test_acting_at_once_rows_have_no_chevron() {
            // Light/Dark/System and the sync mode act at once.
            core.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "" })
            verify(all("chevron", find("darkMode-dark", appearance)).length === 0)
            verify(all("chevron", find("whileOpen-ann", sync)).length === 0)
        }

        function init() {
            core.theme = "carbon"
            core.darkMode = "system"
            core.imagesAlways = false
            core.defaultAccount = ""
            core.syncDays = 30
            core.allowedSenders = []
            core.accounts.clear()
            for (const p of [appearance, images, sync, about, settings]) p.visible = false
            for (const s of [toAppearance, toSync, toNotifications, toImages, toAccounts, toAbout]) s.clear()
            core.reset()
        }

        function test_theme_card_saves_and_applies() {
            appearance.visible = true
            verify(find("themeCard-carbon", appearance).selected)
            mouseClick(find("themeCard-airmail", appearance))
            compare(core.called("saveSettings").length, 1)
            compare(core.called("saveSettings")[0].args, ["airmail", "system", false, ""])
            // The core answers with the settings; the look changes at once.
            core.theme = "airmail"
            verify(thm.airmail)
            verify(find("themeCard-airmail", appearance).selected)
            verify(!find("themeCard-carbon", appearance).selected)
        }

        function test_dark_mode_choice_saves() {
            appearance.visible = true
            mouseClick(find("darkMode-dark", appearance))
            compare(core.called("saveSettings")[0].args, ["carbon", "dark", false, ""])
            core.darkMode = "dark"
            verify(thm.dark)
            verify(find("darkMode-dark", appearance).selected)
            mouseClick(find("darkMode-light", appearance))
            compare(core.called("saveSettings")[1].args, ["carbon", "light", false, ""])
        }

        function test_fullscreen_row_toggles_the_setting() {
            appearance.visible = true
            const sw = find("fullscreenSwitch", appearance)
            verify(!sw.checked)
            mouseClick(sw)
            compare(core.called("chooseFullscreen")[0].args, [true])
            // The core answers with the settings; the switch follows.
            core.fullscreen = true
            verify(sw.checked)
            mouseClick(sw)
            compare(core.called("chooseFullscreen")[1].args, [false])
            core.fullscreen = false
            verify(!sw.checked)
        }

        function test_fullscreen_setting_drives_the_window_visibility() {
            core.fullscreen = false
            verify(mode.available)
            compare(mode.visibility, Window.AutomaticVisibility)
            core.fullscreen = true
            compare(mode.visibility, Window.FullScreen)
            core.fullscreen = false
            compare(mode.visibility, Window.AutomaticVisibility)
        }

        function test_fullscreen_is_hidden_and_inert_where_the_platform_has_none() {
            appearanceDesktop.visible = true
            verify(!find("fullscreenRow", appearanceDesktop).visible)
            // The platform file is missing: the window is left alone.
            core.fullscreen = true
            verify(!desktopMode.available)
            compare(desktopMode.visibility, Window.AutomaticVisibility)
            core.fullscreen = false
        }

        function test_images_always_saves() {
            images.visible = true
            verify(find("images-ask", images).selected)
            mouseClick(find("images-always", images))
            compare(core.called("saveSettings")[0].args, ["carbon", "system", true, ""])
            core.imagesAlways = true
            verify(find("images-always", images).selected)
            mouseClick(find("images-ask", images))
            compare(core.called("saveSettings")[1].args, ["carbon", "system", false, ""])
        }

        function test_removing_an_allowed_sender_saves() {
            core.allowedSenders = ["ada@example.org", "eve@example.org"]
            images.visible = true
            tryVerify(function() { return all("remove-ada@example.org", images).length === 1 })
            waitForRendering(images)
            // Both rows are laid out below each other, not one over the other.
            tryVerify(function() {
                return find("remove-eve@example.org", images).mapToItem(images, 0, 0).y
                    > find("remove-ada@example.org", images).mapToItem(images, 0, 0).y
            })
            mouseClick(find("remove-ada@example.org", images))
            compare(core.called("disallowImagesFrom").length, 1)
            compare(core.called("disallowImagesFrom")[0].args, ["ada@example.org"])
        }

        function test_no_allowed_senders_says_so() {
            images.visible = true
            verify(find("noSenders", images).visible)
            core.allowedSenders = ["ada@example.org"]
            verify(!find("noSenders", images).visible)
        }

        function test_sync_says_what_checking_means() {
            core.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "",
                                   pinService: "", pinHost: "", pinTitle: "", pinFingerprint: "", pinHex: "", pinText: "" })
            sync.visible = true
            tryVerify(function() { return all("whileOpen-ann", sync).length === 1 })
            verify(find("whileOpen-ann", sync).selected)
        }

        function test_sync_heading_shows_the_label_not_the_sender_name() {
            core.accounts.append({ key: "ann", name: "Ann", display: "Work", label: "Work", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "",
                                   pinService: "", pinHost: "", pinTitle: "", pinFingerprint: "", pinHex: "", pinText: "" })
            sync.visible = true
            tryVerify(function() { return all("syncAccount-ann", sync).length === 1 })
            compare(find("syncAccount-ann", sync).text, "Work")
        }

        function test_sync_window_choice_calls_set_sync_window() {
            sync.visible = true
            const days = { "window-30": 30, "window-90": 90, "window-365": 365, "window-all": 0 }
            for (const name in days) {
                core.reset()
                mouseClick(find(name, sync))
                compare(core.called("setSyncWindow").length, 1, name)
                compare(core.called("setSyncWindow")[0].args, [days[name]], name)
            }
            compare(find("window-all", sync).text, "All mail")
            compare(find("window-365", sync).text, "Last year")
        }

        function test_sync_window_shows_the_current_choice() {
            sync.visible = true
            const days = { "window-30": 30, "window-90": 90, "window-365": 365, "window-all": 0 }
            for (const current in days) {
                core.syncDays = days[current]
                for (const name in days) compare(find(name, sync).selected, name === current, current + " " + name)
            }
        }

        // Qt turns `checkable` on once `checked` is set, so a tap used to
        // clear a selected row under its binding.
        function test_tapping_the_selected_choice_keeps_it_checked() {
            sync.visible = true
            mouseClick(find("window-30", sync))
            verify(find("window-30", sync).selected)
            core.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "",
                                   pinService: "", pinHost: "", pinTitle: "", pinFingerprint: "", pinHex: "", pinText: "" })
            tryVerify(function() { return all("whileOpen-ann", sync).length === 1 })
            mouseClick(find("whileOpen-ann", sync))
            verify(find("whileOpen-ann", sync).selected)
            appearance.visible = true
            mouseClick(find("darkMode-system", appearance))
            verify(find("darkMode-system", appearance).selected)
            mouseClick(find("themeCard-carbon", appearance))
            verify(find("themeCard-carbon", appearance).selected)
        }

        // The test build is a development build: it names its source and
        // build time, and wasn't installed as a click.
        function test_about_shows_version_and_build() {
            about.visible = true
            compare(find("version", about).text, BuildInfo.version)
            verify(BuildInfo.dev)
            verify(BuildInfo.source.length > 0)
            verify(BuildInfo.built > 0)
            compare(BuildInfo.installed, 0)
            const build = find("build", about)
            verify(build.visible)
            const lines = build.text.split("\n")
            compare(lines.length, 4)
            compare(lines[1], BuildInfo.source)
            verify(lines[2].indexOf(about.when(BuildInfo.built)) >= 0, lines[2])
            verify(lines[3].indexOf("not as a click") >= 0, lines[3])
        }

        function test_rows_lead_to_the_pages() {
            settings.visible = true
            mouseClick(find("row-appearance", settings))
            mouseClick(find("row-sync", settings))
            mouseClick(find("row-notifications", settings))
            mouseClick(find("row-images", settings))
            mouseClick(find("row-accounts", settings))
            mouseClick(find("row-about", settings))
            for (const s of [toAppearance, toSync, toNotifications, toImages, toAccounts, toAbout]) compare(s.count, 1)
        }
    }
}
