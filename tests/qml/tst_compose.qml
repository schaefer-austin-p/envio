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
            mock.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org", status: "", state: "ok", unread: 0 })
            mock.accounts.append({ key: "work", name: "Work", email: "ann@work.example", status: "", state: "ok", unread: 0 })
            sent.clear()
            closed.clear()
            page.reset()
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
            compare(mock.called("closeCompose").length, 0)
            mouseClick(find("moreButton"))
            tryVerify(function() { return shown("discardRow") })
            mouseClick(find("discardRow"))
            tryVerify(function() { return dlg.opened })
            mouseClick(laidOut(dlg, "secondaryAction"))
            compare(mock.called("closeCompose").pop().args, [1, true])
            compare(closed.count, 1)
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

        function test_mailto_waits_for_the_core_and_an_account() {
            shellCore.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org", status: "", state: "ok", unread: 0 })
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
