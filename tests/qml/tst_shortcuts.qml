import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages"

// The wide layout's keyboard shortcuts (two-pane board, frame 7): the
// ShortcutLayer's signals, and what the shell does with them.
Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    Component { id: coreComponent; MockCore {} }
    Component {
        id: shellComponent
        AppShell {
            width: 400
            height: 700
            uriSource: ""
        }
    }

    TestCase {
        name: "Shortcuts"
        when: windowShown

        readonly property var ann: ({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "" })
        readonly property var composeDraft: ({ account: "ann", to: ["Bob <bob@example.org>"], cc: [], bcc: [], subject: "Plans",
            body: "Hello", inReplyTo: "", references: [], attachments: [] })

        function message(id) {
            return { id: id, account: "ann", accountName: "Ann", from: "Lucia", fromAddress: "lucia@example.org", subject: "Plans " + id,
                     preview: "I found a train", date: 0, dateBucket: "time", unread: true,
                     flagged: false, hasAttachments: false, selected: false, open: false, kbdFocus: false,
                     draftId: "", opens: "reader", notUploaded: false, notUploadedText: "" }
        }
        function find(item, name) {
            if (item.objectName === name) return item
            const kids = item.children || []
            for (let i = 0; i < kids.length; i++) {
                const hit = find(kids[i], name)
                if (hit !== null) return hit
            }
            if (item.contentItem && item.contentItem !== item) return find(item.contentItem, name)
            return null
        }
        function findByName(item, name) {
            if (item.objectName === name) return item
            const kids = []
            if (item.contentItem) kids.push(item.contentItem)
            const list = item.data !== undefined ? item.data : item.children
            for (let i = 0; list && i < list.length; i++) kids.push(list[i])
            for (const k of kids) {
                const f = findByName(k, name)
                if (f !== undefined) return f
            }
            return undefined
        }
        // A shell of the given width (gu) with rows 7, 8 and 9.
        function boot(gu) {
            const c = coreComponent.createObject(root)
            c.accounts.append(ann)
            const s = shellComponent.createObject(root, {
                theme: thm, core: c, accounts: c.accounts, folders: c.folders,
                messages: c.messages, outbox: c.outbox, suggestions: c.suggestions,
                searchResults: c.searchResults
            })
            c.ready = true
            for (const id of [7, 8, 9]) c.messages.append(message(id))
            s.width = thm.gu(gu)
            tryVerify(function() { return s.depth === 1 })
            return { core: c, shell: s }
        }
        function row(b, id) {
            tryVerify(function() { return find(b.shell, "row-" + id) !== null })
            return find(b.shell, "row-" + id)
        }
        function openedReader(b, id) {
            row(b, id).clicked()
            const reader = b.shell.readerItem
            verify(reader !== null, "no reader item")
            b.core.messageOpened({ id: id, html: "<p>Hello</p>", blockedImages: 0, from: "Lucia",
                fromAddress: "lucia@example.org", to: ["me@example.org"], cc: [], subject: "Plans " + id, date: 1700000000,
                account: "ann", senderKnown: false, senderVerified: true, attachments: [] })
            tryVerify(function() { return reader.ready })
            return reader
        }
        // An item that takes the keyboard, as a field or the reader body does.
        function focused(b, qml) {
            const it = Qt.createQmlObject(qml, b.shell)
            it.forceActiveFocus()
            verify(it.activeFocus)
            return it
        }

        function test_down_moves_focus_row_not_open_row() {
            const b = boot(100)
            openedReader(b, 7)
            b.shell.shortcuts.downKey.activated()
            b.shell.shortcuts.downKey.activated()
            compare(b.core.called("stepFocus").length, 2)
            compare(b.core.called("stepFocus")[0].args, [1])
            compare(b.core.focusMessage, 8)
            compare(b.core.openMessage, 7)
            b.shell.shortcuts.upKey.activated()
            compare(b.core.called("stepFocus")[2].args, [-1])
            compare(b.core.focusMessage, 7)
            compare(b.core.called("setOpenMessage").length, 1)
            b.shell.destroy()
        }

        function test_focus_row_draws_outline_and_hover_fill() {
            const b = boot(100)
            openedReader(b, 7)
            b.shell.shortcuts.downKey.activated()
            b.shell.shortcuts.downKey.activated()
            const focus = row(b, 8)
            tryVerify(function() { return find(focus, "focusOutline").visible })
            const outline = find(focus, "focusOutline")
            compare(outline.border.width, 2)
            compare(outline.border.color, thm.text)
            compare(focus.background.color, thm.hover)
            // The open row keeps its selected fill and the accent bar, no outline.
            const open = row(b, 7)
            verify(!find(open, "focusOutline").visible)
            compare(open.background.color, thm.selected)
            verify(find(open, "openBar").visible)
            // The focus row is not the open row: no bar.
            verify(!find(focus, "openBar").visible)
            b.shell.destroy()
        }

        function test_enter_opens_focused_row() {
            const b = boot(100)
            b.shell.shortcuts.downKey.activated()
            b.shell.shortcuts.downKey.activated()
            compare(b.core.focusMessage, 8)
            b.shell.shortcuts.openKey.activated()
            compare(b.core.openMessage, 8)
            compare(b.core.called("setOpenMessage")[0].args, [8])
            b.shell.destroy()
        }

        function test_delete_acts_on_open_message_not_selection() {
            const b = boot(100)
            openedReader(b, 7)
            b.core.messages.setProperty(1, "selected", true)
            b.shell.shortcuts.deleteKey.activated()
            const calls = b.core.called("actOn")
            compare(calls.length, 1)
            compare(calls[0].args, [[7], "delete", 0])
            compare(b.core.called("act").length, 0)
            b.shell.destroy()
        }

        function test_delete_without_an_open_message_does_nothing() {
            const b = boot(100)
            b.shell.shortcuts.deleteKey.activated()
            compare(b.core.called("actOn").length, 0)
            b.shell.destroy()
        }

        function test_ctrl_n_starts_compose() {
            const b = boot(100)
            verify(b.shell.shortcuts.newKey.enabled)
            b.shell.shortcuts.newKey.activated()
            compare(b.core.called("startCompose")[0].args, ["new", 0, "ann", ""])
            compare(b.shell.paneCompose, 1)
            // With compose in the pane the key does nothing more.
            b.shell.shortcuts.newKey.activated()
            compare(b.core.called("startCompose").length, 1)
            b.shell.destroy()
        }

        function test_ctrl_r_replies() {
            const b = boot(100)
            openedReader(b, 7)
            b.shell.shortcuts.replyKey.activated()
            compare(b.core.called("startCompose")[0].args, ["reply", 7, "ann", ""])
            b.shell.destroy()
        }

        function test_esc_closes_reader_then_nothing() {
            const b = boot(100)
            openedReader(b, 7)
            b.shell.shortcuts.closeKey.activated()
            compare(b.core.openMessage, 0)
            const calls = b.core.called("setOpenMessage").length
            b.shell.shortcuts.closeKey.activated()
            compare(b.core.called("setOpenMessage").length, calls)
            compare(b.core.called("closeCompose").length, 0)
            b.shell.destroy()
        }

        function test_esc_on_compose_shows_discard_sheet() {
            const b = boot(100)
            b.shell.composeOpened(4)
            b.core.composeReady(4, composeDraft, "", "")
            const page = find(b.shell, "composePage")
            verify(page !== null)
            const sheet = findByName(page, "discardSheet")
            verify(!sheet.opened)
            b.shell.shortcuts.closeKey.activated()
            tryVerify(function() { return sheet.opened })
            // Nothing closed yet: the sheet asks first.
            compare(b.core.called("closeCompose").length, 0)
            compare(b.shell.paneCompose, 4)
            b.shell.destroy()
        }

        function test_delete_ignored_while_editable_field_focused() {
            const b = boot(100)
            openedReader(b, 7)
            focused(b, "import QtQuick 2.15; TextInput { readOnly: false }")
            b.shell.shortcuts.deleteKey.activated()
            b.shell.shortcuts.closeKey.activated()
            b.shell.shortcuts.helpKey.activated()
            compare(b.core.called("actOn").length, 0)
            compare(b.core.openMessage, 7)
            verify(!b.shell.shortcutsDialog.opened)
            b.shell.destroy()
        }

        function test_delete_and_esc_work_while_readonly_textedit_focused() {
            const b = boot(100)
            openedReader(b, 7)
            focused(b, "import QtQuick 2.15; TextEdit { readOnly: true }")
            b.shell.shortcuts.deleteKey.activated()
            compare(b.core.called("actOn")[0].args, [[7], "delete", 0])
            b.shell.shortcuts.closeKey.activated()
            compare(b.core.openMessage, 0)
            b.shell.destroy()
        }

        function test_question_mark_opens_dialog() {
            const b = boot(100)
            verify(!b.shell.shortcutsDialog.opened)
            b.shell.shortcuts.helpKey.activated()
            tryVerify(function() { return b.shell.shortcutsDialog.opened })
            // While it is up the layer is off (Esc closes the dialog).
            verify(!b.shell.shortcuts.active)
            b.shell.destroy()
        }

        // Every key the layer owns is off while any sheet or dialog is up, so
        // none of them reaches the message behind it (and Esc closes the sheet).
        function checkKeysOff(b, what) {
            const k = b.shell.shortcuts
            verify(!k.active, what + ": layer active")
            for (const key of [k.upKey, k.downKey, k.openKey, k.deleteKey, k.newKey, k.replyKey, k.closeKey, k.helpKey])
                verify(!key.enabled, what + ": a key is still enabled")
        }
        function test_keys_off_under_a_sheet() {
            const b = boot(100)
            b.shell.composeOpened(4)
            b.core.composeReady(4, composeDraft, "", "")
            const sheet = findByName(find(b.shell, "composePage"), "discardSheet")
            b.shell.shortcuts.closeKey.activated()
            tryVerify(function() { return sheet.opened })
            checkKeysOff(b, "sheet")
            sheet.close()
            tryVerify(function() { return !sheet.visible })
            verify(b.shell.shortcuts.active)
            verify(b.shell.shortcuts.deleteKey.enabled)
            b.shell.destroy()
        }
        function test_keys_off_under_a_dialog() {
            const b = boot(100)
            b.shell.shortcuts.helpKey.activated()
            tryVerify(function() { return b.shell.shortcutsDialog.opened })
            checkKeysOff(b, "dialog")
            b.shell.shortcutsDialog.close()
            tryVerify(function() { return !b.shell.shortcutsDialog.visible })
            verify(b.shell.shortcuts.active)
            b.shell.destroy()
        }
        function test_keys_come_back_when_a_sheet_is_destroyed_open() {
            const b = boot(100)
            b.shell.composeOpened(4)
            b.core.composeReady(4, composeDraft, "", "")
            const sheet = findByName(find(b.shell, "composePage"), "discardSheet")
            b.shell.shortcuts.closeKey.activated()
            tryVerify(function() { return sheet.opened })
            verify(!b.shell.shortcuts.active)
            b.core.queued(4, "Queued")
            tryCompare(b.shell, "paneCompose", 0)
            tryVerify(function() { return b.shell.shortcuts.active })
            b.shell.destroy()
        }

        function test_enter_on_a_local_draft_row_opens_its_compose() {
            const b = boot(100)
            const d = message(-5)
            d.draftId = "d-1"
            d.opens = "compose"
            b.core.messages.insert(0, d)
            b.shell.shortcuts.downKey.activated()
            compare(b.core.focusMessage, -5)
            b.shell.shortcuts.openKey.activated()
            compare(b.core.called("startCompose")[0].args, ["draft", 0, "", "d-1"])
            compare(b.core.called("setOpenMessage").length, 0)
            b.shell.destroy()
        }

        function manyRows(b, n) {
            for (let i = 0; i < n; i++) b.core.messages.append(message(100 + i))
        }
        function test_focus_scrolls_into_view_going_down_and_up() {
            const b = boot(100)
            manyRows(b, 40)
            const list = find(b.shell, "messageList")
            tryVerify(function() { return list.height > 0 })
            for (let i = 0; i < 30; i++) { b.shell.shortcuts.downKey.activated(); wait(10) }
            const at = 29
            tryVerify(function() { return list.contentY > 0 })
            const row = list.itemAtIndex(at)
            verify(row !== null, "the focused row has no delegate")
            verify(row.y >= list.contentY - 1 && row.y + row.height <= list.contentY + list.height + 1, "focused row is out of view")
            for (let i = 0; i < 30; i++) { b.shell.shortcuts.upKey.activated(); wait(10) }
            tryCompare(list, "contentY", 0)
            b.shell.destroy()
        }
        function test_a_row_update_does_not_move_the_list() {
            const b = boot(100)
            manyRows(b, 40)
            const list = find(b.shell, "messageList")
            tryVerify(function() { return list.height > 0 })
            b.shell.shortcuts.downKey.activated()
            list.contentY = 200
            b.core.messages.setProperty(0, "unread", false)
            b.core.messages.setProperty(0, "preview", "changed")
            wait(50)
            compare(list.contentY, 200)
            b.shell.destroy()
        }

        function test_shortcuts_inactive_when_narrow() {
            const b = boot(70)
            verify(!b.shell.shortcuts.active)
            verify(!b.shell.shortcuts.downKey.enabled)
            verify(!b.shell.shortcuts.deleteKey.enabled)
            verify(!b.shell.shortcuts.helpKey.enabled)
            b.shell.width = thm.gu(100)
            verify(b.shell.shortcuts.active)
            verify(b.shell.shortcuts.downKey.enabled)
            b.shell.destroy()
        }
    }
}
