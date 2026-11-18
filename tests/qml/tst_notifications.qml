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
    Theme {
        id: thm
        name: core.theme
        darkMode: core.darkMode
    }
    NotificationsPage { id: page; theme: thm; core: core; accounts: core.accounts; anchors.fill: parent }

    TestCase {
        name: "Notifications"
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
        function all(name) {
            return everything(page, []).filter(function(o, i, list) {
                return o.objectName === name && list.indexOf(o) === i
            })
        }
        function find(name) {
            const hit = all(name)
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function account(key, notify) {
            core.accounts.append({ key: key, name: key, display: key, label: "", email: key + "@example.org", status: "", state: "ok",
                                   unread: 0, unreadText: "", notify: notify })
        }

        function init() {
            core.notifyShowText = false
            core.accounts.clear()
            core.reset()
        }

        function test_toggling_an_account_writes_its_setting() {
            account("ann", true)
            account("bob", false)
            tryVerify(function() { return all("notify-ann").length === 1 })
            tryVerify(function() { return all("notify-bob").length === 1 })
            // The Column places the rows on the next frame.
            tryVerify(function() { return find("notifyRow-bob").y > find("notifyRow-ann").y })
            verify(find("notify-ann").checked)
            verify(!find("notify-bob").checked)
            mouseClick(find("notify-ann"))
            compare(core.called("setNotify").length, 1)
            compare(core.called("setNotify")[0].args, ["ann", false])
            mouseClick(find("notify-bob"))
            compare(core.called("setNotify")[1].args, ["bob", true])
        }

        function test_a_failed_save_puts_the_switch_back() {
            account("ann", true)
            tryVerify(function() { return all("notify-ann").length === 1 })
            mouseClick(find("notify-ann"))
            verify(!find("notify-ann").checked)
            // The core could not save: it says so, and the model is unchanged.
            core.notice("not saved")
            verify(find("notify-ann").checked)
        }

        function test_the_text_switch_defaults_off() {
            account("ann", true)
            tryVerify(function() { return all("notify-ann").length === 1 })
            verify(find("notifyText").enabled)
            verify(!find("notifyText").checked)
            verify(find("notifyTextCaption").visible)
            mouseClick(find("notifyText"))
            compare(core.called("setNotifyText")[0].args, [true])
            core.notifyShowText = true
            verify(find("notifyText").checked)
        }

        function test_the_background_help_names_the_package_envio_aps() {
            account("ann", true)
            tryVerify(function() { return all("notify-ann").length === 1 })
            verify(find("backgroundHelp").visible)
            verify(page.exemptCommand.indexOf("'envio.aps'") >= 0)
            verify(page.exemptCommand.indexOf("lifecycle-exempt-appids") >= 0)
            verify(page.exemptCommand.indexOf("TODO") < 0)
            // The shown text only adds break points.
            compare(find("exemptCommand").text.replace(/\u200b/g, ""), page.exemptCommand)
        }

        function test_all_accounts_off_dims_the_text_switch_and_hides_the_help() {
            account("ann", false)
            account("bob", false)
            tryVerify(function() { return all("notify-bob").length === 1 })
            verify(!find("notifyText").enabled)
            verify(find("notifyTextOff").visible)
            verify(!find("notifyTextCaption").visible)
            verify(!find("backgroundHelp").visible)
            verify(find("noNotifications").visible)
            // One account on brings it all back.
            core.accounts.setProperty(1, "notify", true)
            tryVerify(function() { return find("notifyText").enabled })
            verify(find("backgroundHelp").visible)
            verify(!find("noNotifications").visible)
        }
    }
}
