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
            core.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org",
                                   status: "Up to date", state: "ok", unread: 0, unreadText: "",
                                   pinService: "", pinHost: "", pinTitle: "", pinFingerprint: "", pinHex: "", pinText: "" })
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
            tryVerify(function() { return core.called("removeAccount").length === 2 })
            compare(core.called("removeAccount")[1].args[0], "ann")
            compare(core.called("removeAccount")[1].args[1], true)
        }

        function test_rename_row_saves_the_label_and_the_row_text_follows() {
            core.accounts.setProperty(0, "label", "Home")
            openActions()
            mouseClick(find("renameRow", page))
            const sheet = find("renameSheet", page)
            tryVerify(function() { return sheet.opened })
            const field = find("labelField", sheet)
            compare(field.text, "Home", "prefilled with the current label")
            compare(find("labelHint", sheet).text, "Leave empty to show the default.")
            field.text = "Work"
            mouseClick(find("renameSave", sheet))
            compare(core.called("setAccountLabel").length, 1)
            compare(core.called("setAccountLabel")[0].args[0], "ann")
            compare(core.called("setAccountLabel")[0].args[1], "Work")
            tryVerify(function() { return !sheet.visible })
            // The core answers; the row shows the label, not the sender name.
            compare(find("accountLabel", find("account-ann", page)).text, "Ann")
            core.accounts.setProperty(0, "display", "Work")
            core.accounts.setProperty(0, "label", "Work")
            compare(find("accountLabel", find("account-ann", page)).text, "Work")
        }

        function test_rename_field_has_focus_and_a_length_limit() {
            openActions()
            mouseClick(find("renameRow", page))
            const sheet = find("renameSheet", page)
            tryVerify(function() { return sheet.opened })
            const input = find("labelField", sheet).input
            tryVerify(function() { return input.activeFocus })
            compare(input.maximumLength, 40)
            mouseClick(find("renameCancel", sheet))
            tryVerify(function() { return !sheet.visible })
        }

        function test_rename_is_hidden_for_a_broken_account() {
            core.accounts.setProperty(0, "state", "broken")
            openActions()
            verify(!find("renameRow", page).visible)
            verify(find("removeRow", page).visible)
            find("actionSheet", page).close()
        }

        function test_rename_cancel_sends_nothing() {
            openActions()
            mouseClick(find("renameRow", page))
            const sheet = find("renameSheet", page)
            tryVerify(function() { return sheet.opened })
            wait(300)
            mouseClick(find("renameCancel", sheet))
            tryVerify(function() { return !sheet.visible })
            compare(core.called("setAccountLabel").length, 0)
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
            core.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org",
                                   status: "Certificate changed", state: "pin", unread: 0, unreadText: "",
                                   pinService: "smtp", pinHost: "smtp.example.org", pinTitle: "Can't verify the outgoing mail server (SMTP)", pinFingerprint: "AB:CD", pinHex: "abcd",
                                   pinText: "The server's certificate is not the one you trusted." })
            function buttons() { return certButtons() }
            // Rows of the cleared model can linger until they are deleted.
            tryVerify(function() { return buttons().length === 1 })
            waitForRendering(page)
            wait(100)
            return buttons()[0]
        }
        // Core text: "...\nOld fingerprint: {old}\nNew fingerprint: {new}"; a
        // fingerprint is one 95-character word with no break opportunity.
        function test_a_changed_pin_status_wraps_inside_the_fingerprint() {
            const fp1 = "03:10:17:24:31:38:45:52:59:66:73:80:87:94:01:08:15:22:29:36:43:50:57:64:71:78:85:92:99:06:13:20"  // digits only: no break after ":"
            const fp2 = "02:27:4C:71:96:BB:E0:05:2A:4F:74:99:BE:E3:08:2D:52:77:9C:C1:E6:0B:30:55:7A:9F:C4:E9:0E:33:58:7D"
            core.accounts.clear()
            core.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org",
                                   status: "The server's certificate changed.\nOld fingerprint: " + fp1 + "\nNew fingerprint: " + fp2,
                                   state: "pin", unread: 0, unreadText: "",
                                   pinService: "", pinHost: "", pinTitle: "", pinFingerprint: "", pinHex: "", pinText: "" })
            function statuses() {
                return everything(page, []).filter(function(o) { return o.objectName === "status" && o.visible && o.text.indexOf("Old fingerprint") >= 0 })
            }
            tryVerify(function() { return statuses().length >= 1 })
            const st = statuses()[0]
            waitForRendering(page)
            verify(st.contentWidth <= st.width, "contentWidth " + st.contentWidth + " > width " + st.width)
            verify(st.lineCount >= 4, "lineCount " + st.lineCount)
            verify(!st.truncated, "status is truncated")
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
            compare(dlg.title, "Can't verify the outgoing mail server (SMTP)")
            dlg.close()
        }
        // A certificate problem without details (no fingerprint to look at)
        // offers nothing to trust.
        function test_pin_row_without_details_has_no_button() {
            core.accounts.clear()
            core.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org",
                                   status: "The server certificate needs your attention", state: "pin", unread: 0, unreadText: "",
                                   pinService: "", pinHost: "", pinTitle: "", pinFingerprint: "", pinHex: "", pinText: "" })
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
            tryVerify(function() { return core.called("acceptCertificate").length === 1 })
            compare(core.called("acceptCertificate")[0].args, ["ann", "smtp", "abcd"])
        }
        function test_cancel_sends_nothing() {
            mouseClick(pinRow())
            const dlg = find("trustDialog", page)
            tryVerify(function() { return dlg.visible })
            mouseClick(laidOut(dlg, "primaryAction"))
            wait(50)  // the deferred signal would have arrived by now
            compare(core.called("acceptCertificate").length, 0)
        }
    }
}
