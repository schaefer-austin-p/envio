import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages"
import "../../qml/pages/compose"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: mock }
    MockCore { id: shellCore }

    ComposePage {
        id: page
        theme: thm
        core: mock
        accounts: mock.accounts
        suggestions: mock.suggestions
        compose: 1
        anchors.fill: parent
    }
    SignalSpy { id: sent; target: page; signalName: "sent" }
    SignalSpy { id: closed; target: page; signalName: "closed" }

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
        width: 400
        height: 700
        visible: false
    }

    TestCase {
        name: "Compose"
        when: windowShown

        readonly property var reply: ({
            account: "ann", draftId: "", to: ["Bob <bob@example.org>"], cc: [], bcc: [],
            subject: "Re: Plans", body: "Hello", inReplyTo: "m1", references: ["m0", "m1"],
            attachments: []
        })

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
        function find(name, within) {
            return everything(within === undefined ? page : within, []).filter(function(o) { return o.objectName === name })[0]
        }
        function shown(name) {
            const o = find(name)
            return o !== undefined && o.visible && o.width > 0 && o.height > 0
        }
        function lastDraft() {
            const c = mock.called("draftChanged")
            verify(c.length > 0, "draftChanged was called")
            return JSON.parse(c[c.length - 1].args[1])
        }

        function init() {
            mock.reset()
            mock.accounts.clear()
            mock.suggestions.clear()
            mock.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "" })
            mock.accounts.append({ key: "work", name: "Work", display: "Work", label: "", email: "ann@work.example", status: "", state: "ok", unread: 0, unreadText: "" })
            sent.clear()
            closed.clear()
            page.reset()
        }

        function test_body_and_quote_take_the_body_roles_data() {
            return [{ tag: "carbon", airmail: false }, { tag: "airmail", airmail: true }]
        }
        function test_body_and_quote_take_the_body_roles(data) {
            thm.name = data.airmail ? "airmail" : "carbon"
            mock.composeReady(1, reply, "On Monday Bob wrote:\n> hi")
            const input = find("textBlockInput")
            compare(input.font.family, thm.fontBody)
            compare(input.font.pixelSize, thm.bodySize)
            const quote = find("quoteText")
            compare(quote.font.family, thm.fontBody)
            compare(quote.font.pixelSize, thm.bodySize - 2)
            compare(quote.lineHeight, thm.bodyLineHeight)
            thm.name = "carbon"
        }

        function test_ready_fills_fields_and_folds_quote() {
            mock.composeReady(1, reply, "On Monday Bob wrote:\n> hi")
            compare(find("to").entries, ["Bob <bob@example.org>"])
            compare(find("subjectField").text, "Re: Plans")
            compare(find("bodyField").text, "Hello")
            verify(!shown("ccField"), "Cc is hidden while empty")
            verify(shown("quoteShow"))
            verify(!shown("quoteText"))
            mouseClick(find("quoteShow"))
            verify(shown("quoteText"))
            compare(find("quoteText").text, "On Monday Bob wrote:\n> hi")
            compare(mock.called("draftChanged").length, 0, "filling is not an edit")
            mock.composeReady(1, { account: "ann", to: [], cc: ["c@example.org"], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            verify(shown("ccField"), "a filled Cc shows")
            verify(!shown("quoteShow"), "no quote, no fold")
        }

        function test_every_edit_sends_draft_changed() {
            mock.composeReady(1, reply, "")
            find("subjectField").text = "Plans"
            compare(lastDraft().subject, "Plans")
            find("bodyField").text = "Hello again"
            compare(lastDraft().body, "Hello again")
            const to = find("to")
            to.input.text = "carol@example.org"
            to.commit()
            compare(lastDraft().to, ["Bob <bob@example.org>", "carol@example.org"])
            compare(to.input.text, "")
            waitForRendering(page)
            mouseClick(find("to-remove-0"))
            compare(lastDraft().to, ["carol@example.org"])
            const d = lastDraft()
            compare(d.account, "ann")
            compare(d.inReplyTo, "m1")
            compare(d.references, ["m0", "m1"])
            for (const c of mock.called("draftChanged")) compare(c.args[0], 1)
        }

        function test_comma_commits_a_typed_address() {
            mock.composeReady(1, reply, "")
            const to = find("to")
            to.input.text = "dave@example.org,"
            compare(to.entries, ["Bob <bob@example.org>", "dave@example.org"])
        }

        function test_typing_asks_for_suggestions() {
            mock.composeReady(1, reply, "")
            find("to").input.text = "bo"
            tryVerify(function() { return mock.called("autocomplete").length > 0 }, 2000)
            compare(mock.called("autocomplete").pop().args, ["bo"])
        }

        function test_choosing_a_suggestion_adds_a_chip() {
            mock.composeReady(1, { account: "ann", to: [], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            mock.suggestions.append({ name: "Bob", address: "bob@example.org", fromContacts: true, lastSeen: 0, query: "bo" })
            mock.suggestions.append({ name: "Bo", address: "bo@example.org", fromContacts: false, lastSeen: 100, query: "bo" })
            find("to").input.text = "bo"
            tryVerify(function() { return shown("suggestion-1") }, 2000)
            compare(find("suggestionAddress-0").text, "bob@example.org")
            compare(find("suggestionNote-0").text, "from Contacts")
            verify(find("suggestionNote-1").text.indexOf("written to ") === 0, find("suggestionNote-1").text)
            waitForRendering(page)
            mouseClick(find("suggestion-0"))
            compare(find("to").entries, ["Bob <bob@example.org>"])
            compare(find("to").input.text, "")
            compare(lastDraft().to, ["Bob <bob@example.org>"])
            verify(!shown("suggestion-0"), "the list closes")
        }

        function test_suggestion_address_uses_the_address_font_data() {
            return [{ tag: "carbon", airmail: false }, { tag: "airmail", airmail: true }]
        }
        function test_suggestion_address_uses_the_address_font(data) {
            thm.name = data.airmail ? "airmail" : "carbon"
            mock.composeReady(1, { account: "ann", to: [], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            mock.suggestions.append({ name: "Bob", address: "bob@example.org", fromContacts: true, lastSeen: 0, query: "bo" })
            find("to").input.text = "bo"
            tryVerify(function() { return shown("suggestion-0") }, 2000)
            compare(find("suggestionAddress-0").font.family, thm.fontAddress)
        }

        function test_an_answer_to_an_earlier_query_is_not_shown() {
            mock.composeReady(1, { account: "ann", to: [], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            // The answer to "ada" is still in the model when "b" is typed.
            mock.suggestions.append({ name: "Ada", address: "ada@example.org", fromContacts: true, lastSeen: 0, query: "ada" })
            find("to").input.text = "b"
            waitForRendering(page)
            wait(50)
            verify(!shown("suggestion-0"), "Ada does not answer b")
            // The answer to "b" arrives and replaces it.
            mock.suggestions.clear()
            mock.suggestions.append({ name: "Bob", address: "bob@example.org", fromContacts: true, lastSeen: 0, query: "b" })
            tryVerify(function() { return shown("suggestion-0") }, 2000)
            compare(find("suggestionAddress-0").text, "bob@example.org")
        }

        function test_only_the_field_that_asked_shows_suggestions() {
            mock.composeReady(1, { account: "ann", to: [], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            find("showCc").clicked()
            waitForRendering(page)
            mock.suggestions.append({ name: "Bob", address: "bob@example.org", fromContacts: true, lastSeen: 0, query: "bo" })
            find("to").input.text = "bo"
            tryVerify(function() { return shown("suggestion-0") }, 2000)
            // Cc starts asking: the list is not To's any more.
            find("ccField").input.text = "c"
            waitForRendering(page)
            verify(!find("to").asking, "To stopped asking")
            verify(find("ccField").asking)
            verify(!shown("suggestion-0"), "Cc's text is not what the rows answer")
        }

        // #138: Cc Bcc sits in the To row, Subject is a row like it.
        function test_cc_bcc_button_is_in_the_to_row() {
            mock.composeReady(1, { account: "ann", to: [], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            waitForRendering(page)
            const btn = find("showCc")
            verify(shown("showCc"))
            let inTo = false
            for (let p = btn.parent; p; p = p.parent) if (p === find("to")) inTo = true
            verify(inTo, "the button is inside the To row")
            verify(btn.height >= 44)
            verify(!shown("ccField"))
            mouseClick(btn)
            waitForRendering(page)
            verify(shown("ccField") && shown("bccField"), "Cc and Bcc show")
            verify(!shown("showCc"), "the button hides")
        }

        function test_subject_is_an_inline_row_without_a_box() {
            mock.composeReady(1, { account: "ann", to: [], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            waitForRendering(page)
            compare(find("subjectField").field.framed, false)
            const label = find("subjectLabel")
            verify(label !== undefined, "the label is in the row")
            // Same column as From and To: text starts at the page margin, the
            // label column is the page's.
            compare(label.mapToItem(page, 0, 0).x + label.leftPadding, find("fromLabel").mapToItem(page, 0, 0).x)
            compare(label.width, page.labelColumn)
            mouseClick(label)
            verify(find("subjectField").input.activeFocus, "tapping the row focuses the input")
            const n = mock.called("draftChanged").length
            find("subjectField").input.text = "Hi"
            verify(mock.called("draftChanged").length > n, "editing marks the draft edited")
        }

        // Phone shot of a reply: "Subject" ran into "Re: …", and the From address,
        // the To chip and the subject text started at three different x.
        function test_header_values_share_one_column_after_the_labels() {
            mock.composeReady(1, { account: "ann", to: ["Bob <bob@example.org>"], cc: [], bcc: [],
                                   subject: "Re: FW: a long subject that fills the whole row and more",
                                   body: "", inReplyTo: "", references: [], attachments: [] }, "")
            waitForRendering(page)
            const col = page.labelColumn
            verify(col >= thm.gu(9.5), "at least margin + 52 px label + gap, as designed")
            compare(find("fromValue").mapToItem(page, 0, 0).x, col)
            compare(find("to-remove-0").mapToItem(page, 0, 0).x, col)
            const input = find("subjectField").input
            compare(input.mapToItem(page, input.leftPadding, 0).x, col)
            for (const name of ["subjectLabel", "fromLabel"]) {
                const label = find(name)
                const end = label.mapToItem(page, 0, 0).x + label.leftPadding + label.contentWidth
                verify(end + thm.gu(1) <= col, name + " keeps a gap before its value")
            }
        }

        // #138 review: with Cc Bcc showing, a long chip uses what the first line
        // has left (line minus the button minus a small input), input beside it.
        function test_long_chip_uses_the_line_beside_cc_bcc_at_360() {
            root.width = 360
            mock.composeReady(1, { account: "ann", to: ["Bartholomew Longname <bartholomew.longname@example.org>"], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            waitForRendering(page)
            const to = find("to")
            const chip = find("to-remove-0")
            const input = to.input.mapToItem(chip.parent, 0, 0)
            verify(shown("showCc"))
            verify(chip.width >= to.width - page.labelColumn - thm.gu(10) - find("showCc").width - 2, "chip is " + chip.width + " wide")
            compare(input.y, chip.y, "the input is on the chip's line")
            verify(chip.x + chip.width + to.input.width <= chip.parent.width + 1)
            verify(to.height <= find("fromRow").height + 2, "one line")
            root.width = 400
        }

        function test_typed_text_is_in_the_draft() {
            mock.composeReady(1, reply, "")
            find("to").input.text = "dave@exa"
            compare(lastDraft().to, ["Bob <bob@example.org>", "dave@exa"])
        }

        function test_refused_send_keeps_the_page() {
            mock.composeReady(1, reply, "")
            mouseClick(find("sendButton"))
            const c = mock.called("send")
            compare(c.length, 1)
            compare(c[0].args[0], 1)
            mock.composeRefused(1, "Add a recipient first.")
            tryVerify(function() { return shown("composeError") })
            compare(find("composeError").text, "Add a recipient first.")
            compare(sent.count, 0)
            compare(closed.count, 0)
            mock.composeRefused(2, "someone else's")
            compare(find("composeError").text, "Add a recipient first.")
        }

        function test_queued_closes_the_page() {
            mock.composeReady(1, reply, "")
            mock.queued(2, "Sent")
            compare(sent.count, 0, "another compose's queue is not ours")
            mock.queued(1, "Sent")
            compare(sent.count, 1)
            compare(sent.signalArguments[0][0], "Sent")
        }

        function test_send_appends_the_quote_unless_removed() {
            mock.composeReady(1, reply, "> old")
            mouseClick(find("sendButton"))
            compare(JSON.parse(mock.called("send").pop().args[1]).body, "Hello\n\n> old")
            mock.composeRefused(1, "Try again.")
            mouseClick(find("quoteShow"))
            mouseClick(find("quoteRemove"))
            verify(!shown("quoteText"))
            compare(lastDraft().body, "Hello", "removing the quote is an edit")
            mouseClick(find("sendButton"))
            compare(JSON.parse(mock.called("send").pop().args[1]).body, "Hello")
        }

        function test_typed_but_uncommitted_address_is_sent() {
            mock.composeReady(1, reply, "")
            find("to").input.text = "last@example.org"
            mouseClick(find("sendButton"))
            compare(JSON.parse(mock.called("send").pop().args[1]).to, ["Bob <bob@example.org>", "last@example.org"])
        }

        function test_bar_close_and_send_data() {
            return [{ tag: "carbon", airmail: false }, { tag: "airmail", airmail: true }]
        }
        function test_bar_close_and_send(data) {
            thm.name = data.airmail ? "airmail" : "carbon"
            mock.composeReady(1, reply, "")
            compare(find("topBar").height, thm.barHeight)
            const close = find("backButton")
            compare(close.iconName, "close")
            verify(close.width >= thm.touch && close.height >= thm.touch)
            const send = find("sendButton")
            compare(send.iconName, "send")
            compare(send.background.height, thm.compactControlHeight)
            const bar = find("topBar")
            const top = send.mapToItem(bar, 0, 0).y
            verify(top >= 0 && top + send.height <= bar.height,
                   "send target " + top + ".." + (top + send.height) + " in bar " + bar.height)
            verify(send.mapToItem(bar, send.width, 0).x <= bar.width - thm.gu(1.5), "right inset")
            verify(send.width >= thm.touch && send.height >= thm.touch)
            compare(find("title", find("topBar")).font.pixelSize, thm.barTitleCompactFont.pixelSize)
        }

        function test_back_keeps_the_draft() {
            mock.composeReady(1, reply, "")
            mouseClick(find("backButton"))
            compare(mock.called("closeCompose").pop().args, [1, false])
            compare(closed.count, 1)
        }

        function laidOut(dlg, action) {
            const p = find("primaryAction", dlg.contentItem)
            const q = find("secondaryAction", dlg.contentItem)
            tryVerify(function() { return q.mapToItem(null, 0, 0).y > p.mapToItem(null, 0, 0).y }, 2000)
            return find(action, dlg.contentItem)
        }

        // The discard sheet rests above a keyboard that covers the screen's
        // bottom (simulated rectangle: the offscreen platform has no keyboard).
        function test_discard_sheet_sits_above_the_keyboard() {
            mock.composeReady(1, reply, "")
            const sheet = find("discardSheet")
            sheet.keyboard = Qt.rect(0, 450, 400, 250)
            mouseClick(find("moreButton"))
            tryVerify(function() { return sheet.opened })
            tryVerify(function() { return sheet.height > 0 })
            verify(sheet.y + sheet.height <= 450, "sheet bottom " + (sheet.y + sheet.height))
            verify(sheet.y >= 0)
            sheet.close()
            tryVerify(function() { return !sheet.visible })
            sheet.keyboard = Qt.rect(0, 0, 0, 0)
        }

        function test_discard_asks_then_discards() {
            mock.composeReady(1, reply, "")
            mouseClick(find("moreButton"))
            tryVerify(function() { return shown("discardRow") })
            mouseClick(find("discardRow"))
            const dlg = find("discardAsk")
            tryVerify(function() { return dlg.opened })
            compare(find("primaryAction", dlg.contentItem).text, "Keep editing")
            mouseClick(laidOut(dlg, "primaryAction"))
            tryVerify(function() { return !dlg.opened })
            wait(50)  // the deferred signal would have arrived by now
            compare(mock.called("closeCompose").length, 0)
            mouseClick(find("moreButton"))
            tryVerify(function() { return shown("discardRow") })
            mouseClick(find("discardRow"))
            tryVerify(function() { return dlg.opened })
            mouseClick(laidOut(dlg, "secondaryAction"))
            // The signal comes from the event loop, after the dialog closed.
            tryCompare(closed, "count", 1)
            compare(mock.called("closeCompose").pop().args, [1, true])
        }

        function test_attached_shows_a_chip() {
            mock.composeReady(1, reply, "")
            mock.attached(2, "/p/other.pdf", "other.pdf", 10)
            verify(!shown("attachment-0"), "another compose's file")
            mock.attached(1, "/p/a.pdf", "a.pdf", 2048)
            verify(shown("attachment-0"))
            compare(find("attachmentName-0").text, "a.pdf")
            compare(find("attachmentSize-0").text, "2.0 kB")
            compare(lastDraft().attachments, ["/p/a.pdf"])
            mouseClick(find("attachmentRemove-0"))
            verify(!shown("attachment-0"))
            compare(lastDraft().attachments, [])
        }

        function test_filled_attachments_show_as_chips() {
            const d = JSON.parse(JSON.stringify(reply))
            d.attachments = ["/cache/forward-1/report.pdf"]
            mock.composeReady(1, d, "")
            verify(shown("attachment-0"))
            compare(find("attachmentName-0").text, "report.pdf")
        }

        function test_picked_files_are_attached() {
            mock.composeReady(1, reply, "")
            page.pick(["/home/a.txt", "/home/b.txt"])
            const c = mock.called("attach")
            compare(c.length, 2)
            compare(c[0].args, [1, "/home/a.txt"])
            compare(c[1].args, [1, "/home/b.txt"])
        }

        function test_attach_button_imports() {
            mock.composeReady(1, reply, "")
            let asked = 0
            page.importFiles = function() { asked++ }
            mouseClick(find("attachButton"))
            compare(asked, 1)
        }

        function test_attach_failed_says_so() {
            mock.composeReady(1, reply, "")
            mock.attachFailed(1, "Could not read that file.")
            tryVerify(function() { return find("composeToast").visible })
            compare(find("composeToast").text, "Could not read that file.")
        }

        function test_from_picks_the_account() {
            mock.composeReady(1, reply, "")
            compare(find("fromValue").text, "ann@example.org")
            mouseClick(find("fromRow"))
            tryVerify(function() { return shown("account-work") })
            mouseClick(find("account-work"))
            compare(find("fromValue").text, "ann@work.example")
            compare(lastDraft().account, "work")
        }

        function test_from_sheet_keeps_the_sender_name_and_adds_the_label() {
            mock.accounts.setProperty(1, "label", "Office")
            mock.composeReady(1, reply, "")
            mouseClick(find("fromRow"))
            tryVerify(function() { return shown("account-work") })
            const text = find("accountText-work")
            compare(text.children[0].text, "Work")
            compare(text.children[1].text, "Office \u00b7 ann@work.example")
            mock.accounts.setProperty(1, "label", "")
            mouseClick(find("account-work"))
            tryVerify(function() { return !shown("account-work") })
        }

        // Phone shot: the accounts in "Send from" sat at the sheet's left
        // edge and at the top of their rows.
        function test_from_sheet_rows_line_up_with_its_title() {
            mock.composeReady(1, reply, "")
            mouseClick(find("fromRow"))
            tryVerify(function() { return shown("account-work") })
            waitForRendering(page)
            const row = find("account-work")
            const text = find("accountText-work")
            // The row has an icon: text from 60 px (icon 24 at 20, gap 16).
            compare(text.mapToItem(row, 0, 0).x, thm.gu(7.5))
            verify(text.mapToItem(row, 0, 0).x + text.width <= row.width - thm.gu(2.5))
            const top = text.mapToItem(row, 0, 0).y
            const bottom = row.height - top - text.height
            verify(Math.abs(top - bottom) <= 1, "text sits " + top + " from the top, " + bottom + " from the bottom")
            // Close the sheet, so it doesn't cover the next test's clicks.
            mouseClick(row)
            tryVerify(function() { return !shown("account-work") })
        }

        function chevronIn(item) {
            return everything(item, []).filter(function(o) { return o.objectName === "chevron" })[0]
        }
        function test_from_row_shows_a_down_chevron_when_it_opens_a_picker() {
            mock.composeReady(1, reply, "")
            const row = find("fromRow")
            const c = chevronIn(row)
            verify(c !== undefined)
            verify(c.visible)
            verify(c.width > 0)
            compare(c.name, "down")
            compare(c.color, thm.secondary)
            verify(c.mapToItem(row, 0, 0).x + c.width <= row.width)
        }
        function test_from_row_has_no_chevron_with_one_account() {
            mock.accounts.remove(1)
            mock.composeReady(1, reply, "")
            tryVerify(function() { return chevronIn(find("fromRow")) === undefined })
        }

        // Phone shots: the From label and the suggestion text sat at x=0, and the
        // To row kept a second, empty line below a chip.
        function test_from_and_suggestions_keep_the_page_margin() {
            mock.composeReady(1, { account: "ann", to: ["Bob <bob@example.org>"], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            waitForRendering(page)
            const margin = thm.gu(2)
            compare(find("fromLabel").mapToItem(page, 0, 0).x, margin)
            mock.suggestions.append({ name: "Cy", address: "cy@example.org", fromContacts: false, lastSeen: 100, query: "cy" })
            find("to").input.text = "cy"
            tryVerify(function() { return shown("suggestion-0") }, 2000)
            waitForRendering(page)
            compare(find("suggestionAddress-0").mapToItem(page, 0, 0).x, margin)
            compare(find("suggestionNote-0").mapToItem(page, 0, 0).x, margin)
        }

        // #145: Enter made a chip but the cursor and keyboard went away, so a
        // second address typed next went nowhere.
        function test_enter_keeps_focus_in_the_field() {
            mock.composeReady(1, { account: "ann", to: [], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            waitForRendering(page)
            const to = find("to")
            to.input.forceActiveFocus()
            verify(to.input.activeFocus)
            // On the phone the platform drops focus once Enter is handled
            // (offscreen it does not): do the same after the field's own
            // handler, so the field has to take focus back.
            const drop = function() { to.input.focus = false }
            to.input.accepted.connect(drop)
            keyClick("a"); keyClick("@"); keyClick("e"); keyClick("x")
            keyClick(Qt.Key_Return)
            to.input.accepted.disconnect(drop)
            compare(to.entries, ["a@ex"])
            compare(to.input.text, "")
            tryVerify(function() { return to.input.activeFocus }, 1000, "the field keeps focus after Enter")
            keyClick("b")
            compare(to.input.text, "b")
            compare(to.entries, ["a@ex"], "the refocus did not commit the new text")
        }

        // Enter on an empty field is the user dismissing the keyboard: it must
        // not be forced back open.
        function test_enter_on_an_empty_field_does_not_take_focus_back() {
            mock.composeReady(1, { account: "ann", to: [], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            waitForRendering(page)
            const to = find("to")
            to.input.forceActiveFocus()
            const drop = function() { to.input.focus = false }
            to.input.accepted.connect(drop)
            keyClick(Qt.Key_Return)
            to.input.accepted.disconnect(drop)
            wait(100)
            compare(to.entries, [])
            verify(!to.input.activeFocus, "an empty Enter does not refocus")
        }

        function test_recipient_row_is_one_line_beside_a_chip() {
            mock.composeReady(1, { account: "ann", to: ["Bob <bob@example.org>"], cc: [], bcc: [], subject: "", body: "",
                                   inReplyTo: "", references: [], attachments: [] }, "")
            waitForRendering(page)
            const to = find("to")
            const one = find("fromRow").height
            // One touch row plus the hairline, not two rows.
            verify(to.height <= one + 2, "to row is " + to.height + " high, a line is " + one)
        }

        function test_mailto_waits_for_the_core_and_an_account() {
            shellCore.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "" })
            shell.visible = true
            tryVerify(function() { return shell.accountCount === 1 })
            shell.openMailto("mailto:bob@example.org?subject=Hi")
            compare(shellCore.called("startCompose").length, 0, "the core is not ready")
            shellCore.ready = true
            tryVerify(function() { return shellCore.called("startCompose").length === 1 })
            compare(shellCore.called("startCompose")[0].args, ["mailto", 0, "", "mailto:bob@example.org?subject=Hi"])
            shell.openMailto("mailto:again@example.org")
            compare(shellCore.called("startCompose").length, 2, "a ready core starts at once")
            shell.visible = false
        }
    }
}
