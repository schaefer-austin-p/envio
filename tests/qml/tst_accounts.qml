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
                                   status: "Up to date", state: "ok", unread: 0, unreadText: "",
                                   pinService: "", pinHost: "", pinFingerprint: "", pinHex: "", pinText: "" })
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

        function test_update_password_back_clears_the_field() {
            update.visible = true
            const field = find("newPassword", update)
            field.text = "n3w-secret"
            const back = everything(update, []).filter(function(o) { return o.text === "Back" && o.clicked !== undefined })[0]
            verify(back !== undefined)
            back.clicked()
            compare(field.text, "")
            compare(updateClosed.count, 1)
            compare(core.called("updatePassword").length, 0)
            update.visible = false
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

        // A ListView's items are reached by two paths: count each once.
        function certButtons() {
            return everything(page, []).filter(function(o, i, list) {
                return o.objectName === "checkCertificate" && o.visible && list.indexOf(o) === i
            })
        }
        function pinRow() {
            core.accounts.clear()
            core.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org",
                                   status: "Certificate changed", state: "pin", unread: 0, unreadText: "",
                                   pinService: "smtp", pinHost: "smtp.example.org", pinFingerprint: "AB:CD", pinHex: "abcd",
                                   pinText: "The server's certificate is not the one you trusted." })
            function buttons() { return certButtons() }
            // Rows of the cleared model can linger until they are deleted.
            tryVerify(function() { return buttons().length === 1 })
            waitForRendering(page)
            wait(100)
            return buttons()[0]
        }
        function test_pin_row_opens_trust_dialog() {
            const b = pinRow()
            verify(b.height >= 44)
            mouseClick(b)
            const dlg = find("trustDialog", page)
            tryVerify(function() { return dlg.visible })
            compare(find("fingerprint", dlg.contentItem).text, "AB:CD")
            // The title names the server, not the address.
            compare(dlg.host, "smtp.example.org")
            dlg.close()
        }
        // A certificate problem without details (no fingerprint to look at)
        // offers nothing to trust.
        function test_pin_row_without_details_has_no_button() {
            core.accounts.clear()
            core.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org",
                                   status: "The server certificate needs your attention", state: "pin", unread: 0, unreadText: "",
                                   pinService: "", pinHost: "", pinFingerprint: "", pinHex: "", pinText: "" })
            tryVerify(function() {
                return everything(page, []).filter(function(o) { return o.objectName === "status" && o.visible && o.text.indexOf("attention") >= 0 }).length >= 1
            })
            tryVerify(function() { return certButtons().length === 0 })
        }
        function test_trust_sends_accept_certificate() {
            mouseClick(pinRow())
            const dlg = find("trustDialog", page)
            tryVerify(function() { return dlg.visible })
            mouseClick(laidOut(dlg, "secondaryAction"))
            compare(core.called("acceptCertificate").length, 1)
            compare(core.called("acceptCertificate")[0].args, ["ann", "smtp", "abcd"])
        }
        function test_cancel_sends_nothing() {
            mouseClick(pinRow())
            const dlg = find("trustDialog", page)
            tryVerify(function() { return dlg.visible })
            mouseClick(laidOut(dlg, "primaryAction"))
            compare(core.called("acceptCertificate").length, 0)
        }
    }
}
