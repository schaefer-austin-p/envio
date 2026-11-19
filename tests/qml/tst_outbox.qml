import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages/inbox"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: core }
    OutboxPage {
        id: page
        theme: thm
        core: core
        outbox: core.outbox
        anchors.fill: parent
    }
    SignalSpy { id: composing; target: page; signalName: "composeOpened" }
    SignalSpy { id: backed; target: page; signalName: "backRequested" }
    SignalSpy { id: checking; target: page; signalName: "accountsRequested" }

    TestCase {
        name: "Outbox"
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
        function find(name, from) {
            const hit = everything(from, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function shown(name) {
            const hit = everything(page, []).filter(function(o) { return o.objectName === name })
            return hit.length > 0 && hit[0].visible
        }
        function item(id, status, text, reason, action) {
            return { id: id, account: "ann", subject: "Invoice " + id, to: "ana@example.org",
                     status: status, text: text === undefined ? "" : text, reason: reason === undefined ? "" : reason,
                     reasonAction: reason === undefined ? "" : (action === undefined ? "Check certificate" : action) }
        }
        function init() {
            core.reset()
            core.outbox.clear()
            composing.clear()
            backed.clear()
            checking.clear()
        }
        function laidOut(dlg, action) {
            const p = find("primaryAction", dlg.contentItem)
            const q = find("secondaryAction", dlg.contentItem)
            tryVerify(function() { return q.mapToItem(null, 0, 0).y > p.mapToItem(null, 0, 0).y }, 2000)
            return find(action, dlg.contentItem)
        }

        function test_outbox_actions_follow_status() {
            core.outbox.append(item("w", "waiting", "Waiting to send"))
            core.outbox.append(item("s", "sending", "Sending"))
            core.outbox.append(item("f", "failed", "The server refused it"))
            core.outbox.append(item("m", "maybesent", "It may have been sent"))
            core.outbox.append(item("n", "sentnotfiled", "Sent, not yet in Sent"))
            waitForRendering(page)
            // Delete follows the status: core refuses it while sending or once sent.
            for (const id of ["w", "f", "m"]) verify(shown("delete-" + id), "delete-" + id)
            verify(!shown("delete-s") && !shown("delete-n"))
            verify(!shown("retry-n") && !shown("edit-n") && !shown("sendAgain-n") && !shown("moveToSent-n"))
            verify(!shown("retry-w") && !shown("edit-w") && !shown("sendAgain-w") && !shown("moveToSent-w"))
            verify(!shown("retry-s") && !shown("edit-s") && !shown("sendAgain-s") && !shown("moveToSent-s"))
            verify(shown("retry-f") && shown("edit-f"))
            verify(!shown("sendAgain-f") && !shown("moveToSent-f"))
            verify(shown("sendAgain-m") && shown("moveToSent-m"))
            verify(!shown("retry-m") && !shown("edit-m"))
            compare(find("status-f", page).text, "The server refused it")

            mouseClick(find("retry-f", page))
            compare(core.called("decideOutbox").pop().args, ["f", "retry"])
            mouseClick(find("sendAgain-m", page))
            compare(core.called("decideOutbox").pop().args, ["m", "sendagain"])
            mouseClick(find("moveToSent-m", page))
            compare(core.called("decideOutbox").pop().args, ["m", "movetosent"])
            mouseClick(find("edit-f", page))
            compare(core.called("editOutbox").pop().args, ["f"])
            compare(composing.count, 1)
        }

        function test_delete_asks_and_cancel_is_the_default() {
            core.outbox.append(item("f", "failed"))
            waitForRendering(page)
            mouseClick(find("delete-f", page))
            const dlg = find("deleteAsk", page)
            tryVerify(function() { return dlg.opened })
            compare(find("primaryAction", dlg.contentItem).text, "Cancel")
            mouseClick(laidOut(dlg, "primaryAction"))
            tryVerify(function() { return !dlg.opened })
            wait(50)  // the deferred signal would have arrived by now
            compare(core.called("decideOutbox").length, 0)
            mouseClick(find("delete-f", page))
            tryVerify(function() { return dlg.opened })
            mouseClick(laidOut(dlg, "secondaryAction"))
            tryVerify(function() { return core.called("decideOutbox").length > 0 })
            compare(core.called("decideOutbox").pop().args, ["f", "delete"])
        }

        function test_a_message_held_by_the_outgoing_certificate_says_why_and_links_to_the_check() {
            const why = "The certificate of the outgoing mail server changed."
            core.outbox.append(item("w", "waiting", "Waiting to be sent", why))
            core.outbox.append(item("x", "waiting", "Waiting to be sent"))
            waitForRendering(page)
            compare(find("reason-w", page).text, why)
            verify(shown("reason-w") && shown("checkCertificate-w"))
            verify(!shown("reason-x") && !shown("checkCertificate-x"))
            verify(shown("delete-w"))
            mouseClick(find("checkCertificate-w", page))
            compare(checking.count, 1)
            compare(find("checkCertificate-w", page).text, "Check certificate")
        }

        function test_the_button_wording_comes_from_the_row() {
            core.outbox.append(item("a", "waiting", "Waiting to be sent", "Sign-in failed.", "Open accounts"))
            waitForRendering(page)
            compare(find("reason-a", page).text, "Sign-in failed.")
            compare(find("checkCertificate-a", page).text, "Open accounts")
            mouseClick(find("checkCertificate-a", page))
            compare(checking.count, 1)
        }

        function test_back_leaves() {
            mouseClick(find("backButton", page))
            compare(backed.count, 1)
        }
    }
}
