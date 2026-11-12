import QtQuick 2.15
import QtTest 1.2
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
    AppearancePage { id: appearance; theme: thm; core: core; anchors.fill: parent; visible: false }
    ImagesPage { id: images; theme: thm; core: core; anchors.fill: parent; visible: false }
    SyncPage { id: sync; theme: thm; accounts: core.accounts; anchors.fill: parent; visible: false }
    AboutPage { id: about; theme: thm; anchors.fill: parent; visible: false }
    SettingsPage { id: settings; theme: thm; core: core; anchors.fill: parent; visible: false }
    SignalSpy { id: toAppearance; target: settings; signalName: "appearanceRequested" }
    SignalSpy { id: toSync; target: settings; signalName: "syncRequested" }
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
            for (const key of ["appearance", "sync", "images", "accounts", "about"]) {
                const c = find("chevron", find("row-" + key, settings))
                verify(c.visible, key + " has a chevron")
                verify(c.width > 0)
            }
            settings.visible = false
        }
        function test_the_airmail_settings_chevron_is_red() {
            core.theme = "airmail"
            compare(find("chevron", find("row-sync", settings)).color, thm.chevron)
            core.theme = "carbon"
        }
        function test_acting_at_once_rows_have_no_chevron() {
            // Light/Dark/System and the sync mode act at once.
            core.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "" })
            verify(all("chevron", find("darkMode-dark", appearance)).length === 0)
            verify(all("chevron", find("whileOpen-ann", sync)).length === 0)
        }

        function init() {
            core.theme = "carbon"
            core.darkMode = "system"
            core.imagesAlways = false
            core.defaultAccount = ""
            core.allowedSenders = []
            core.accounts.clear()
            for (const p of [appearance, images, sync, about, settings]) p.visible = false
            for (const s of [toAppearance, toSync, toImages, toAccounts, toAbout]) s.clear()
            core.reset()
        }

        function test_theme_card_saves_and_applies() {
            appearance.visible = true
            verify(find("themeCard-carbon", appearance).checked)
            mouseClick(find("themeCard-airmail", appearance))
            compare(core.called("saveSettings").length, 1)
            compare(core.called("saveSettings")[0].args, ["airmail", "system", false, ""])
            // The core answers with the settings; the look changes at once.
            core.theme = "airmail"
            verify(thm.airmail)
            verify(find("themeCard-airmail", appearance).checked)
            verify(!find("themeCard-carbon", appearance).checked)
        }

        function test_dark_mode_choice_saves() {
            appearance.visible = true
            mouseClick(find("darkMode-dark", appearance))
            compare(core.called("saveSettings")[0].args, ["carbon", "dark", false, ""])
            core.darkMode = "dark"
            verify(thm.dark)
            verify(find("darkMode-dark", appearance).checked)
            mouseClick(find("darkMode-light", appearance))
            compare(core.called("saveSettings")[1].args, ["carbon", "light", false, ""])
        }

        function test_images_always_saves() {
            images.visible = true
            verify(find("images-ask", images).checked)
            mouseClick(find("images-always", images))
            compare(core.called("saveSettings")[0].args, ["carbon", "system", true, ""])
            core.imagesAlways = true
            verify(find("images-always", images).checked)
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
            core.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "",
                                   pinService: "", pinHost: "", pinFingerprint: "", pinHex: "", pinText: "" })
            sync.visible = true
            tryVerify(function() { return all("whileOpen-ann", sync).length === 1 })
            verify(find("whileOpen-ann", sync).checked)
            compare(find("keepFor-ann", sync).text, "Last 30 days")
        }

        function test_about_shows_version_and_commit() {
            about.visible = true
            compare(find("version", about).text, BuildInfo.version)
            compare(find("commit", about).text, BuildInfo.commit)
        }

        function test_rows_lead_to_the_pages() {
            settings.visible = true
            mouseClick(find("row-appearance", settings))
            mouseClick(find("row-sync", settings))
            mouseClick(find("row-images", settings))
            mouseClick(find("row-accounts", settings))
            mouseClick(find("row-about", settings))
            for (const s of [toAppearance, toSync, toImages, toAccounts, toAbout]) compare(s.count, 1)
        }
    }
}
