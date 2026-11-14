import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/platform/stampTint.js" as StampTint
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
        suggestions: shellCore.suggestions
        searchResults: shellCore.searchResults
        uriSource: ""
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
            return { id: id, account: "ann", accountName: "Ann", from: "Lucia", fromAddress: "lucia@example.org", subject: "Plans " + id,
                     preview: "I found a train…", date: 0, dateBucket: "time", unread: true,
                     flagged: false, hasAttachments: false, selected: selected === true }
        }
        function init() {
            core.reset()
            core.accounts.clear()
            core.folders.clear()
            core.messages.clear()
            core.outbox.clear()
            core.olderRow = 0
            thm.name = "carbon"
            core.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org",
                                   status: "Up to date", state: "ok", unread: 3, unreadText: "3" })
            core.accounts.append({ key: "bob", name: "Bob", email: "bob@example.org",
                                   status: "Up to date", state: "ok", unread: 0, unreadText: "" })
            opened.clear()
            page.pendingInbox = ""
            find("undoToast", page).hide()
            find("noticeToast", page).hide()
            waitForRendering(page)
        }
        function test_folder_group_rows_have_a_chevron_but_leaf_rows_do_not() {
            core.folders.append({ id: 9, account: "ann", name: "Archive", role: "archive", unread: 0, depth: 0 })
            mouseClick(find("menuButton", page))
            const ann = find("account-ann", page)
            const bob = find("account-bob", page)
            tryVerify(function() { return find("chevron", ann).visible })
            verify(find("chevron", bob).visible)
            // The open group points down, a closed one right.
            compare(find("chevron", ann).name, "down")
            compare(find("chevron", bob).name, "forward")
            verify(!has("chevron", find("folder-9", page)))
            verify(!has("chevron", find("outboxRow", page)))
            // Close the drawer again: it is the ancestor that has a closeRequested signal.
            let drawer = ann
            while (drawer.closeRequested === undefined) drawer = drawer.parent
            drawer.visible = false
        }

        function shows(text, from) {
            return everything(from, []).some(function(o) { return o.text === text && o.visible })
        }
        function drawerOf(item) {
            let drawer = item
            while (drawer.closeRequested === undefined) drawer = drawer.parent
            return drawer
        }

        function test_drawer_lists_the_open_account_on_every_opening() {
            mouseClick(find("menuButton", page))
            const drawer = drawerOf(find("account-ann", page))
            compare(core.called("listFolders").length, 1)
            drawer.visible = false
            compare(core.called("listFolders").length, 1)
            drawer.visible = true
            compare(core.called("listFolders").length, 2)
            compare(core.called("listFolders")[1].args, [drawer.expanded])
            drawer.visible = false
        }

        function test_drawer_reports_its_account_while_open() {
            mouseClick(find("menuButton", page))
            const drawer = drawerOf(find("account-ann", page))
            const last = function() { return core.called("drawerAccount").pop().args[0] }
            compare(last(), drawer.expanded)
            drawer.expand("bob")
            compare(last(), "bob")
            drawer.visible = false
            compare(last(), "")
            drawer.expand("ann")
        }

        function test_unified_count_is_the_sum_of_the_accounts() {
            core.accounts.setProperty(0, "unread", 4)
            core.accounts.setProperty(1, "unread", 1)
            mouseClick(find("menuButton", page))
            const drawer = drawerOf(find("account-ann", page))
            const row = find("unifiedRow", page)
            tryVerify(function() { return shows("5", row) })
            core.accounts.setProperty(0, "unread", 0)
            tryVerify(function() { return shows("1", row) })
            compare(drawer.unreadTotal, 1)
            drawer.visible = false
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

        // #130: the end of the list loads older mail by itself; the core holds
        // the once-per-page guard, the view only reports reaching the end.
        function test_reaching_the_end_asks_for_older_mail_once() {
            for (let i = 1; i <= 40; i++) core.messages.append(message(i))
            core.olderRow = 1
            const list = find("messageList", page)
            list.positionViewAtBeginning()
            waitForRendering(page)
            core.reset()
            list.positionViewAtEnd()
            tryVerify(function() { return core.called("loadOlderNearEnd").length === 1 })
            wait(200)
            compare(core.called("loadOlderNearEnd").length, 1, "one scroll asks once")
            compare(core.called("loadOlder").length, 0, "the tap path is separate")
        }

        // The row turns Ready while the list already sits at its end: the
        // footer grows the list, and the one request still goes out.
        function test_the_row_turning_ready_at_the_end_asks_once() {
            for (let i = 1; i <= 40; i++) core.messages.append(message(i))
            const list = find("messageList", page)
            list.positionViewAtEnd()
            waitForRendering(page)
            wait(100)
            core.reset()
            core.olderRow = 1
            tryVerify(function() { return core.called("loadOlderNearEnd").length === 1 })
            wait(200)
            compare(core.called("loadOlderNearEnd").length, 1)
        }

        function test_the_row_turning_ready_mid_list_asks_nothing() {
            for (let i = 1; i <= 40; i++) core.messages.append(message(i))
            const list = find("messageList", page)
            list.positionViewAtBeginning()
            waitForRendering(page)
            wait(100)
            core.reset()
            core.olderRow = 1
            wait(200)
            compare(core.called("loadOlderNearEnd").length, 0)
        }

        function test_hostile_sender_and_subject_fetch_nothing() {
            const evil = 'Hi <img src="http://example.invalid/x.png">'
            netRequests.reset()
            const m = message(12)
            m.from = evil
            m.subject = evil
            m.preview = evil
            m.accountName = evil
            core.messages.append(m)
            waitForRendering(page)
            wait(500)
            compare(netRequests.count(), 0)
        }

        function test_stamp_tint_uses_from_address() {
            const a = "ann@example.org"
            let b = "bob@example.org"
            for (let i = 0; StampTint.isBlue(b) === StampTint.isBlue(a) && i < 50; i++)
                b = "bob" + i + "@example.org"
            verify(StampTint.isBlue(b) !== StampTint.isBlue(a))
            const rows = [{ id: 31, addr: a }, { id: 32, addr: b }]
            for (const r of rows) {
                const m = message(r.id)
                m.fromAddress = r.addr
                core.messages.append(m)
            }
            thm.name = "airmail"
            waitForRendering(page)
            for (const r of rows) {
                const stamp = find("stamp", find("row-" + r.id, page)).item
                verify(stamp !== null)
                compare(stamp.blue, StampTint.isBlue(r.addr))
            }
            thm.name = "carbon"
        }

        function test_rows_follow_the_theme() {
            core.messages.append(message(21))
            core.messages.append({ id: 22, account: "ann", accountName: "Ann", from: "Ann", fromAddress: "ann@example.org", subject: "Read",
                                   preview: "", date: 0, dateBucket: "time", unread: false,
                                   flagged: false, hasAttachments: false, selected: false })
            waitForRendering(page)
            const names = ["carbon", "airmail"]
            for (let i = 0; i < 2; i++) {
                thm.name = names[i]
                const air = names[i] === "airmail"
                const row = find("row-21", page)
                compare(thm.showUnreadDot, !air)
                compare(find("unreadDot", row).visible, !air)
                compare(find("stamp", row).item !== null, air)
                compare(find("rowSender", row).font.weight, thm.unreadWeight)
                compare(find("rowSender", find("row-22", page)).font.weight, Font.Normal)
            }
            thm.name = "carbon"
        }

        function in_each_look(body) {
            for (const look of ["carbon", "airmail"]) {
                thm.name = look
                core.reset()
                core.olderRow = 0
                body(look)
            }
            thm.name = "carbon"
        }

        function test_older_row_ready_taps_load_older() {
            core.messages.append(message(11))
            waitForRendering(page)
            in_each_look(function(look) {
                core.olderRow = 1
                const tap = find("olderTap", page)
                verify(tap.visible, look)
                compare(tap.height, thm.touch, look)
                compare(tap.width, find("messageList", page).width, look)
                mouseClick(tap)
                compare(core.called("loadOlder").length, 1, look)
                // The Ui pages the new rows in itself: the view never asks.
                compare(core.called("more").length, 0, look)
            })
        }

        function test_older_row_loading_is_not_tappable() {
            core.messages.append(message(11))
            waitForRendering(page)
            in_each_look(function(look) {
                core.olderRow = 2
                const tap = find("olderTap", page)
                verify(!tap.visible, look)
                verify(find("olderLoading", page).visible, look)
                compare(find("olderLoading", page).text, "Loading older messages…", look)
                mouseClick(find("olderLoading", page))
                compare(core.called("loadOlder").length, 0, look)
            })
        }

        function test_older_row_done_shows_text() {
            core.messages.append(message(11))
            waitForRendering(page)
            in_each_look(function(look) {
                core.olderRow = 3
                verify(find("olderDone", page).visible, look)
                compare(find("olderDone", page).text, "No older messages", look)
                compare(find("olderDone", page).color, thm.secondary, look)
                verify(!find("olderTap", page).visible, look)
                mouseClick(find("olderDone", page))
                compare(core.called("loadOlder").length, 0, look)
            })
        }

        function test_older_row_hidden_has_no_height() {
            core.messages.append(message(11))
            waitForRendering(page)
            in_each_look(function(look) {
                core.olderRow = 0
                const row = find("olderTap", page).parent
                compare(row.height, 0, look)
                verify(!row.visible, look)
            })
        }

        function test_tap_opens_the_message() {
            core.messages.append(message(11))
            waitForRendering(page)
            const row = find("row-11", page)
            mouseClick(row)
            compare(opened.count, 1)
            compare(opened.signalArguments[0][0], 11)
        }

        function test_sideways_swipe_does_not_open_the_message() {
            core.messages.append(message(11))
            waitForRendering(page)
            const row = find("row-11", page)
            const far = Qt.styleHints.startDragDistance * 3
            opened.clear()
            mousePress(row, 20, row.height / 2)
            mouseMove(row, 20 + far / 2, row.height / 2)
            mouseMove(row, 20 + far, row.height / 2)
            mouseRelease(row, 20 + far, row.height / 2)
            compare(opened.count, 0)
        }

        function test_tap_with_a_little_jitter_still_opens_the_message() {
            core.messages.append(message(12))
            waitForRendering(page)
            const row = find("row-12", page)
            opened.clear()
            mousePress(row, 20, row.height / 2)
            mouseMove(row, 21, row.height / 2)
            mouseRelease(row, 21, row.height / 2)
            compare(opened.count, 1)
            compare(opened.signalArguments[0][0], 12)
        }

        function test_vertical_drag_on_a_row_scrolls_without_opening() {
            for (let i = 1; i <= 40; i++) core.messages.append(message(i))
            waitForRendering(page)
            const list = find("messageList", page)
            const row = find("row-1", page)
            const d = Qt.styleHints.startDragDistance * 4
            opened.clear()
            const y0 = row.height - 4
            mousePress(row, 20, y0)
            mouseMove(row, 20 + 2, y0 - d / 2)
            mouseMove(row, 20 + 3, y0 - d)
            mouseMove(row, 20 + 4, y0 - d * 2)
            mouseRelease(row, 20 + 4, y0 - d * 2)
            verify(list.contentY > 0, "contentY " + list.contentY)
            compare(opened.count, 0)
        }

        function test_real_press_and_hold_selects_the_row() {
            core.messages.append(message(11))
            waitForRendering(page)
            const row = find("row-11", page)
            mousePress(row, 20, row.height / 2)
            wait(Qt.styleHints.mousePressAndHoldInterval + 300)
            mouseRelease(row, 20, row.height / 2)
            compare(core.called("select").pop().args, [11, true])
            compare(opened.count, 0)
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
            core.undoClosed(6)
            verify(!toast.visible)
        }

        function test_an_older_offer_closing_leaves_the_newer_toast() {
            const toast = find("undoToast", page)
            core.undoOffer(7, "Archived “A”")
            core.undoOffer(8, "Archived “B”")
            core.undoClosed(7)
            verify(toast.visible)
            compare(toast.text, "Archived “B”")
            core.undoClosed(8)
            verify(!toast.visible)
        }

        function test_removing_the_selected_accounts_tab_shows_the_unified_inbox() {
            page.showAccount("bob")
            core.reset()
            core.accounts.remove(1)
            compare(page.tab, "")
            compare(page.unified, true)
            compare(core.called("openList").pop().args, ["inboxes", 0])
        }

        function test_removing_another_account_keeps_the_tab() {
            page.showAccount("ann")
            core.reset()
            core.accounts.remove(1)
            compare(page.tab, "ann")
            compare(core.called("openList").length, 0)
        }

        function test_notice_shows_without_an_action() {
            const toast = find("noticeToast", page)
            core.notice("Could not reach the server")
            verify(toast.visible)
            compare(toast.actionText, "")
        }

        function test_leaving_a_tab_drops_its_pending_inbox() {
            mouseClick(find("tab-ann", page))
            compare(page.pendingInbox, "ann")
            mouseClick(find("tab-all", page))
            compare(page.pendingInbox, "")
            core.reset()
            // The Folders reply for the abandoned tab arrives late.
            core.folders.append({ id: 7, account: "ann", name: "INBOX", role: "inbox", unread: 2, depth: 0 })
            wait(50)
            compare(core.called("openList").length, 0)
            compare(page.unified, true)
            core.folders.clear()
        }

        function test_reply_without_an_inbox_stops_waiting() {
            mouseClick(find("tab-bob", page))
            core.reset()
            core.folders.append({ id: 9, account: "bob", name: "Archive", role: "archive", unread: 0, depth: 0 })
            tryCompare(page, "pendingInbox", "")
            // A later listFolders (the drawer's, the move sheet's) must not switch the list.
            core.folders.append({ id: 8, account: "bob", name: "INBOX", role: "inbox", unread: 0, depth: 0 })
            wait(50)
            compare(core.called("openList").length, 0)
            core.folders.clear()
        }

        // The chip's own icon, or null when it has none showing.
        function chipIcon(chip) {
            const hit = everything(chip, []).filter(function(o) { return o.objectName === "chipIcon" && o.visible })
            return hit.length > 0 ? hit[0] : null
        }
        function drawerIcon(row) {
            const hit = everything(row, []).filter(function(o) { return o.objectName === "offlineIcon" && o.visible })
            return hit.length > 0 ? hit[0] : null
        }

        function test_offline_account_tab_shows_the_icon_and_name() {
            core.accounts.setProperty(1, "state", "offline")
            const bob = find("tab-bob", page)
            tryVerify(function() { return chipIcon(bob) !== null })
            compare(chipIcon(bob).name, "offline")
            compare(chipIcon(bob).color, thm.accent)
            compare(bob.Accessible.name, "Bob, offline")
            const unread = core.accounts.get(1).unreadText
            core.accounts.setProperty(1, "unreadText", "3")
            compare(bob.Accessible.name, "Bob  3, offline")
            core.accounts.setProperty(1, "unreadText", unread)
            core.accounts.setProperty(1, "state", "ok")
        }

        function test_online_tabs_and_all_have_no_icon() {
            core.accounts.setProperty(1, "state", "offline")
            tryVerify(function() { return chipIcon(find("tab-bob", page)) !== null })
            compare(chipIcon(find("tab-all", page)), null)
            compare(chipIcon(find("tab-ann", page)), null)
            core.accounts.setProperty(1, "state", "ok")
        }

        function test_selected_carbon_tab_draws_the_icon_in_its_text_colour() {
            core.accounts.setProperty(1, "state", "offline")
            const bob = find("tab-bob", page)
            tryVerify(function() { return chipIcon(bob) !== null })
            compare(chipIcon(bob).color, thm.accent)
            mouseClick(bob)
            tryVerify(function() { return bob.selected })
            // A selected Carbon chip is filled with the primary colour.
            compare(chipIcon(bob).color, thm.primaryText)
            thm.name = "airmail"
            compare(chipIcon(bob).color, thm.accent)
            mouseClick(find("tab-all", page))
            core.accounts.setProperty(1, "state", "ok")
        }

        function test_tab_icon_follows_the_account_state() {
            const bob = find("tab-bob", page)
            compare(chipIcon(bob), null)
            const plain = bob.implicitWidth
            core.accounts.setProperty(1, "state", "offline")
            tryVerify(function() { return chipIcon(bob) !== null })
            fuzzyCompare(bob.implicitWidth - plain, bob.iconSpace, 0.5, "the icon widens the chip by its space, once")
            core.accounts.setProperty(1, "state", "ok")
            tryVerify(function() { return chipIcon(bob) === null })
            compare(bob.implicitWidth, plain)
            compare(bob.Accessible.name, "Bob")
        }

        function test_drawer_row_shows_the_icon_for_an_offline_account() {
            core.accounts.setProperty(1, "state", "offline")
            mouseClick(find("menuButton", page))
            const ann = find("account-ann", page)
            const bob = find("account-bob", page)
            tryVerify(function() { return drawerIcon(bob) !== null })
            compare(drawerIcon(bob).name, "offline")
            compare(drawerIcon(ann), null)
            verify(bob.Accessible.name.indexOf(", offline") > 0, bob.Accessible.name)
            verify(ann.Accessible.name.indexOf("offline") < 0, ann.Accessible.name)
            core.accounts.setProperty(1, "state", "ok")
            tryVerify(function() { return drawerIcon(bob) === null })
            let drawer = ann
            while (drawer.closeRequested === undefined) drawer = drawer.parent
            drawer.visible = false
        }

        function test_outbox_banner_shows_while_an_account_is_offline() {
            const outbox = find("outboxBanner", page)
            core.accounts.setProperty(1, "state", "offline")
            verify(!outbox.visible)
            core.outbox.append({ id: "a", account: "ann", subject: "Hi", to: "x@y", status: "waiting", text: "" })
            tryVerify(function() { return outbox.visible })
            compare(outbox.text, "waiting 1")
            verify(!has("offlineBanner", page))
            core.outbox.clear()
            core.accounts.setProperty(1, "state", "ok")
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

        function test_a_sent_message_is_not_waiting() {
            const banner = find("outboxBanner", page)
            core.outbox.append({ id: "n", account: "ann", subject: "Hi", to: "x@y", status: "sentnotfiled", text: "" })
            waitForRendering(page)
            verify(!banner.visible)
            core.outbox.append({ id: "w", account: "ann", subject: "Hi", to: "x@y", status: "waiting", text: "" })
            tryVerify(function() { return banner.visible })
            compare(banner.text, "waiting 1")
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
            shell.retrying = false
            shell.rootKind = ""
            shellCore.reset()
            shellCore.ready = false
            shellCore.accounts.clear()
            shellCore.outbox.clear()
        }
        function has(name) {
            return everything(shell, []).some(function(o) { return o.objectName === name })
        }
        function addAccount() {
            shellCore.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org",
                                        status: "Up to date", state: "ok", unread: 0, unreadText: "" })
        }

        // The shell covers the page above while it is shown.
        function cleanup() { shell.visible = false }

        function test_no_accounts_opens_the_wizard() {
            shellCore.ready = true
            tryVerify(function() { return find("wizardPage").visible })
        }

        function test_accounts_open_the_inbox() {
            shellCore.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org",
                                        status: "Up to date", state: "ok", unread: 0, unreadText: "" })
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
            // The page stays, busy, until the core is ready.
            verify(problem.visible)
            verify(problem.busy)
            verify(!find("tryAgain").enabled)
            shellCore.ready = true
            tryVerify(function() { return !problem.visible })
        }

        function test_store_recovering_by_itself_clears_the_problem() {
            addAccount()
            shellCore.storeProblem("The mail cache cannot be opened.")
            const problem = find("problemPage")
            tryVerify(function() { return problem.visible })
            shellCore.ready = true
            tryVerify(function() { return !problem.visible })
            tryVerify(function() { return find("inboxPage").visible })
        }

        function test_a_retry_that_keeps_failing_keeps_the_page() {
            shellCore.storeProblem("The mail cache cannot be opened.")
            const problem = find("problemPage")
            tryVerify(function() { return problem.visible })
            mouseClick(find("tryAgain"))
            verify(problem.busy)
            shellCore.storeProblem("Still cannot open it.")
            verify(problem.visible)
            verify(!problem.busy)
            compare(find("problemText").text, "Still cannot open it.")
            verify(find("tryAgain").enabled)
        }

        function test_trust_prompt_outside_the_wizard_is_ignored() {
            addAccount()
            shellCore.ready = true
            tryVerify(function() { return find("inboxPage").visible })
            tryVerify(function() { return !has("trustDialog") })
            shellCore.trustPrompt("ann@example.org", "imap", "imap.example.org", "T", "SHA-256 AB", "ab", "The server changed.")
            wait(50)
            verify(!has("trustDialog"))
            compare(shellCore.called("acceptCertificate").length, 0)
            compare(shellCore.called("acceptSetupCertificate").length, 0)
        }

        function test_a_later_ready_keeps_the_open_outbox() {
            addAccount()
            shellCore.outbox.append({ id: "a", account: "ann", subject: "Hi", to: "x@y", status: "waiting", text: "" })
            shellCore.ready = true
            tryVerify(function() { return find("inboxPage").visible })
            tryVerify(function() { return find("outboxBanner").visible })
            waitForRendering(shell)
            mouseClick(find("outboxBanner"))
            tryVerify(function() { return has("outboxPage") && find("outboxPage").visible })
            // The worker restarts.
            shellCore.ready = false
            shellCore.ready = true
            wait(50)
            verify(find("outboxPage").visible)
        }

        function test_the_root_changes_when_the_accounts_do() {
            addAccount()
            shellCore.ready = true
            tryVerify(function() { return find("inboxPage").visible })
            shellCore.accounts.clear()
            tryVerify(function() { return find("wizardPage").visible })
        }

        function test_start_failed_shows_the_problem() {
            shellCore.startFailed("Envio could not start.")
            tryVerify(function() { return find("problemPage").visible })
            compare(find("problemText").text, "Envio could not start.")
        }
    }
}
