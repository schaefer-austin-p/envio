import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages"
import "../../qml/pages/inbox"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: core }
    InboxPage {
        id: page
        theme: thm
        core: core
        accounts: core.accounts
        folders: core.folders
        messages: core.messages
        outbox: core.outbox
        anchors.fill: parent
    }
    SignalSpy { id: opened; target: page; signalName: "messageRequested" }

    MockCore { id: shellCore }
    AppShell {
        id: shell
        theme: thm
        core: shellCore
        accounts: shellCore.accounts
        folders: shellCore.folders
        messages: shellCore.messages
        outbox: shellCore.outbox
        anchors.fill: parent
        visible: false
    }

    TestCase {
        name: "Inbox"
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
        function has(name, from) {
            return everything(from, []).some(function(o) { return o.objectName === name })
        }
        function findText(text, from) {
            const hit = everything(from, []).filter(function(o) { return o.text === text && o.visible })
            verify(hit.length > 0, "no item with text " + text)
            return hit[0]
        }
        function message(id, selected) {
            return { id: id, account: "ann", accountName: "Ann", from: "Lucia", subject: "Plans " + id,
                     preview: "I found a train…", date: 0, dateBucket: "time", unread: true,
                     flagged: false, hasAttachments: false, selected: selected === true }
        }
        function init() {
            core.reset()
            core.accounts.clear()
            core.folders.clear()
            core.messages.clear()
            core.outbox.clear()
            core.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org",
                                   status: "Up to date", state: "ok", unread: 3 })
            core.accounts.append({ key: "bob", name: "Bob", email: "bob@example.org",
                                   status: "Up to date", state: "ok", unread: 0 })
            opened.clear()
            find("undoToast", page).hide()
            find("noticeToast", page).hide()
            waitForRendering(page)
        }
        function test_tab_switch_opens_the_list() {
            mouseClick(find("tab-all", page))
            compare(core.called("openList").pop().args, ["inboxes", 0])
            core.reset()
            mouseClick(find("tab-ann", page))
            compare(core.called("listFolders").length, 1)
            compare(core.called("listFolders")[0].args, ["ann"])
            // The Inbox's id comes with the account's folders.
            core.folders.append({ id: 9, account: "ann", name: "Archive", role: "archive", unread: 0, depth: 0 })
            core.folders.append({ id: 7, account: "ann", name: "INBOX", role: "inbox", unread: 2, depth: 0 })
            compare(core.called("openList").length, 1)
            compare(core.called("openList")[0].args, ["folder", 7])
            core.folders.clear()
        }

        function test_scrolling_to_the_end_asks_for_more() {
            for (let i = 1; i <= 40; i++) core.messages.append(message(i))
            const list = find("messageList", page)
            core.reset()
            list.positionViewAtEnd()
            tryVerify(function() { return core.called("more").length > 0 })
        }

        function test_tap_opens_the_message() {
            core.messages.append(message(11))
            waitForRendering(page)
            const row = find("row-11", page)
            mouseClick(row)
            compare(opened.count, 1)
            compare(opened.signalArguments[0][0], 11)
        }

        function test_press_and_hold_starts_selection() {
            core.messages.append(message(11))
            waitForRendering(page)
            const row = find("row-11", page)
            row.pressAndHold()
            compare(core.called("select").pop().args, [11, true])
            // The core marks the row; the bar replaces the top bar.
            core.messages.setProperty(0, "selected", true)
            const bar = find("selectionBar", page)
            tryVerify(function() { return bar.visible })
            compare(find("title", find("selectionBar", page)).text, "selected 1")
            // Back leaves selection.
            mouseClick(find("leaveSelection", page))
            compare(core.called("clearSelection").length, 1)
            core.messages.setProperty(0, "selected", false)
            tryVerify(function() { return !bar.visible })
        }

        function test_archive_acts_on_the_selection() {
            core.messages.append(message(11, true))
            core.messages.append(message(12, true))
            core.messages.append(message(13))
            const bar = find("selectionBar", page)
            tryVerify(function() { return bar.visible })
            compare(find("title", find("selectionBar", page)).text, "selected 2")
            mouseClick(find("act-archive", page))
            compare(core.called("act").pop().args, ["archive", 0])
            mouseClick(find("act-delete", page))
            compare(core.called("act").pop().args, ["delete", 0])
            core.messages.clear()
        }

        function test_move_is_disabled_across_accounts() {
            core.messages.append(message(11, true))
            const other = message(12, true)
            other.account = "bob"
            core.messages.append(other)
            tryVerify(function() { return find("selectionBar", page).visible })
            tryCompare(find("act-move", page), "enabled", false)
            core.messages.setProperty(1, "selected", false)
            tryCompare(find("act-move", page), "enabled", true)
            core.reset()
            mouseClick(find("act-move", page))
            compare(core.called("listFolders").pop().args, ["ann"])
            const sheet = find("moveSheet", page)
            tryVerify(function() { return sheet.opened })
            core.folders.append({ id: 7, account: "ann", name: "INBOX", role: "inbox", unread: 0, depth: 0 })
            core.folders.append({ id: 8, account: "ann", name: "Trips 2026", role: "", unread: 0, depth: 0 })
            // A role name, never the raw path.
            compare(has("folder-7", sheet.contentItem), true)
            compare(findText("Inbox", sheet.contentItem).text, "Inbox")
            mouseClick(find("folder-8", sheet.contentItem))
            compare(core.called("act").pop().args, ["move", 8])
            core.folders.clear()
            core.messages.clear()
        }

        function test_undo_toast_calls_undo() {
            const toast = find("undoToast", page)
            core.undoOffer(5, "Archived “Plans”")
            verify(toast.visible)
            compare(toast.text, "Archived “Plans”")
            mouseClick(findText("Undo", toast))
            compare(core.called("undo").pop().args, [5])
            core.undoOffer(6, "Deleted")
            verify(toast.visible)
            core.undoClosed()
            verify(!toast.visible)
        }

        function test_notice_shows_without_an_action() {
            const toast = find("noticeToast", page)
            core.notice("Could not reach the server")
            verify(toast.visible)
            compare(toast.actionText, "")
        }

        function test_offline_banner_shows() {
            verify(!find("offlineBanner", page).visible)
            core.accounts.setProperty(1, "state", "offline")
            tryVerify(function() { return find("offlineBanner", page).visible })
            core.accounts.setProperty(1, "state", "ok")
            tryVerify(function() { return !find("offlineBanner", page).visible })
        }

        function test_outbox_banner_opens_the_outbox() {
            const banner = find("outboxBanner", page)
            verify(!banner.visible)
            core.outbox.append({ id: "a", account: "ann", subject: "Hi", to: "x@y", status: "waiting", text: "" })
            tryVerify(function() { return banner.visible })
            compare(banner.text, "waiting 1")
            const asked = createTemporaryObject(spyComponent, root, { target: page })
            waitForRendering(page)
            mouseClick(banner)
            compare(asked.count, 1)
            core.outbox.clear()
        }

        Component {
            id: spyComponent
            SignalSpy { signalName: "outboxRequested" }
        }
    }

    TestCase {
        name: "Shell"
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
        function find(name) {
            const hit = everything(shell, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }

        function init() {
            shell.visible = true
            shell.problem = ""
            shellCore.reset()
            shellCore.ready = false
            shellCore.accounts.clear()
        }

        // The shell covers the page above while it is shown.
        function cleanup() { shell.visible = false }

        function test_no_accounts_opens_the_wizard() {
            shellCore.ready = true
            tryVerify(function() { return find("wizardPage").visible })
        }

        function test_accounts_open_the_inbox() {
            shellCore.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org",
                                        status: "Up to date", state: "ok", unread: 0 })
            shellCore.ready = true
            tryVerify(function() { return find("inboxPage").visible })
        }

        function test_store_problem_page_retries() {
            shellCore.storeProblem("The mail cache cannot be opened.")
            const problem = find("problemPage")
            tryVerify(function() { return problem.visible })
            compare(find("problemText").text, "The mail cache cannot be opened.")
            mouseClick(find("tryAgain"))
            compare(shellCore.called("start").length, 1)
            tryVerify(function() { return !problem.visible })
        }

        function test_start_failed_shows_the_problem() {
            shellCore.startFailed("Envio could not start.")
            tryVerify(function() { return find("problemPage").visible })
            compare(find("problemText").text, "Envio could not start.")
        }
    }
}
