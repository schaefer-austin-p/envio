import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages/accounts"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: core }
    AccountsPage {
        id: page
        theme: thm
        core: core
        accounts: core.accounts
        anchors.fill: parent
    }
    UpdatePasswordPage {
        id: update
        theme: thm
        core: core
        account: "ann"
        accountName: "Ann"
        anchors.fill: parent
        visible: false
    }
    SignalSpy { id: updateClosed; target: update; signalName: "closed" }
    SignalSpy { id: wantsPassword; target: page; signalName: "updatePasswordRequested" }

    TestCase {
        name: "Accounts"
        when: windowShown

        function everything(item, out) {
            out.push(item)
            // A ListView's delegates live in its contentItem.
            if (item.contentItem !== undefined && item.contentItem !== null && item.contentItem !== item)
                everything(item.contentItem, out)
            for (const prop of ["data", "children"]) {
                if (item[prop] === undefined) continue
                for (let i = 0; i < item[prop].length; i++) everything(item[prop][i], out)
                break
            }
            return out
        }
        function find(name, from) {
            const hit = everything(from, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function init() {
            core.reset()
            const sheet = find("actionSheet", page)
            tryVerify(function() { return !sheet.visible })
            core.accounts.clear()
            core.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org",
                                   status: "Up to date", state: "ok", unread: 0 })
            wantsPassword.clear()
            updateClosed.clear()
        }
        // A dialog's buttons start stacked until its column is laid out.
        function laidOut(dlg, action) {
            const p = find("primaryAction", dlg.contentItem)
            const q = find("secondaryAction", dlg.contentItem)
            tryVerify(function() { return q.mapToItem(null, 0, 0).y > p.mapToItem(null, 0, 0).y }, 2000)
            return find(action, dlg.contentItem)
        }
        function openActions() {
            // The delegate is created after the model changes.
            // (A row of the cleared model can linger until it is deleted.)
            function live() {
                return everything(page, []).filter(function(o) {
                    return o.objectName === "account-ann" && o.model !== undefined && o.model.key === "ann"
                })
            }
            tryVerify(function() { return live().length >= 1 })
            const row = live()[0]
            mouseClick(row, 20, 20)
            const sheet = find("actionSheet", page)
            tryVerify(function() { return sheet.opened })
            wait(100)
        }

        function test_rows_show_name_address_and_status() {
            compare(find("status", find("account-ann", page)).text, "Up to date")
        }

        function test_remove_asks_when_unsent_work() {
            openActions()
            mouseClick(find("removeRow", page))
            compare(core.called("removeAccount").length, 1)
            compare(core.called("removeAccount")[0].args[0], "ann")
            compare(core.called("removeAccount")[0].args[1], false)

            // The core found unsent work and asked.
            core.confirmRemove("ann", "2 messages are waiting in the Outbox.")
            const dlg = find("removeAsk", page)
            tryVerify(function() { return dlg.opened })
            compare(dlg.message, "2 messages are waiting in the Outbox.")
            compare(dlg.primaryText, "Keep account")
            compare(dlg.secondaryText, "Remove")

            // Keep: nothing more is sent.
            mouseClick(laidOut(dlg, "primaryAction"))
            tryVerify(function() { return !dlg.visible })
            compare(core.called("removeAccount").length, 1)

            // Ask again and remove for real.
            core.confirmRemove("ann", "2 messages are waiting in the Outbox.")
            tryVerify(function() { return dlg.opened })
            mouseClick(laidOut(dlg, "secondaryAction"))
            compare(core.called("removeAccount").length, 2)
            compare(core.called("removeAccount")[1].args[0], "ann")
            compare(core.called("removeAccount")[1].args[1], true)
        }

        function test_update_password_row_asks_for_the_page() {
            openActions()
            mouseClick(find("updatePasswordRow", page))
            compare(wantsPassword.count, 1)
            compare(wantsPassword.signalArguments[0][0], "ann")
        }

        function test_update_password_sends_it() {
            update.visible = true
            const field = find("newPassword", update)
            verify(!find("savePassword", update).enabled)
            field.text = "n3w-secret"
            mouseClick(find("savePassword", update))
            compare(core.called("updatePassword").length, 1)
            compare(core.called("updatePassword")[0].args[0], "ann")
            compare(core.called("updatePassword")[0].args[1], "n3w-secret")
            compare(field.text, "")
            compare(updateClosed.count, 1)
            update.visible = false
        }
    }
}
