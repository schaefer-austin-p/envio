import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"
import "../../qml/pages"
import "../../qml/pages/inbox"
import "../../qml/pages/reader"
import "../../qml/pages/compose"

// #228 T6 (and the chip of #266): a draft in the reader has one Edit button;
// a local draft shows in the Drafts list with a "Not uploaded" StatusChip and
// opens Compose at once; a Drafts list's selection bar offers Delete only
// (no Archive, no Move); Compose shows the core's note. With ENVIO_SHOTS_DIR
// set (see tests/qml/main.cpp) the screens are saved as PNGs to compare with
// docs/design/carbon-draft-edit.html and the #266 boards.
Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm; darkMode: "light" }
    MockCore { id: core }
    MockCore { id: readerCore }
    MockCore { id: composeCore }

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
    SignalSpy { id: draftOpened; target: page; signalName: "draftRequested" }

    ReplyBar { id: replyBar; theme: thm; width: 400; y: 800 }
    ReaderMore { id: more; theme: thm; width: 400; y: 800 }
    SignalSpy { id: replyAll; target: more; signalName: "replyAllRequested" }
    SignalSpy { id: moveReq; target: more; signalName: "moveRequested" }
    SignalSpy { id: flagReq; target: more; signalName: "flagRequested" }
    SignalSpy { id: contactReq; target: more; signalName: "contactRequested" }
    SignalSpy { id: editReq; target: replyBar; signalName: "editRequested" }

    Component {
        id: readerComponent
        ReaderPage {
            theme: thm
            core: readerCore
            accounts: readerCore.accounts
            folders: readerCore.folders
        }
    }
    Component {
        id: composeComponent
        ComposePage {
            theme: thm
            core: composeCore
            accounts: composeCore.accounts
            suggestions: composeCore.suggestions
            compose: 1
        }
    }
    Component { id: chipComponent; StatusChip { theme: thm; text: "Not uploaded" } }
    Component { id: spyComponent; SignalSpy {} }
    property var reader: null
    property var composePage: null

    TestCase {
        name: "Drafts"
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
        function all(name, from) {
            return everything(from, []).filter(function(o) { return o.objectName === name })
        }
        function find(name, from) {
            const hit = all(name, from)
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function shown(name, from) {
            const hit = all(name, from)
            return hit.length > 0 && hit[0].visible && hit[0].width > 0
        }
        function row(id, draftId, notUploaded, selected, reason) {
            const local = draftId !== ""
            return { id: id, account: "ann", accountName: "Ann", from: local ? "To: Mara Becker, Ana Ruiz" : "To: Dev Kapoor",
                     fromAddress: local ? "mara@example.org" : "dev@example.org",
                     subject: local ? "Trip plan, first pass" : "Photos from Saturday",
                     preview: local ? "Day one is the coastal path, then…" : "Here are the two from the ridge. The rest…",
                     date: 0, dateBucket: "time", unread: false, flagged: false, hasAttachments: false,
                     selected: selected === true, open: false, kbdFocus: false,
                     draftId: draftId, notUploaded: notUploaded,
                     notUploadedText: notUploaded ? (reason === undefined ? "Not uploaded" : reason) : "", opens: local ? "compose" : "reader" }
        }
        function save(item, name) {
            if (shotsDir === "") return
            // A page has no ground of its own (the window's shows); a grab
            // would be black behind it.
            const ground = Qt.createQmlObject('import QtQuick 2.15; Rectangle { z: -1; anchors.fill: parent }', item)
            ground.color = thm.ground
            waitForRendering(item)
            let done = false
            item.grabToImage(function(res) { res.saveToFile(shotsDir + "/" + name + ".png"); done = true })
            tryVerify(function() { return done })
            ground.destroy()
        }
        function view(draft) {
            return { id: 5, html: "<p>Here are the two from the ridge. The rest are in the album.</p>", blockedImages: 0,
                     from: "me", fromAddress: "lucia@example.org", to: ["Dev Kapoor <dev@example.org>"], cc: [],
                     subject: "Photos from Saturday", date: 1700000000, account: "ann", senderKnown: true,
                     senderVerified: true, attachments: [], isDraft: draft }
        }
        function init() {
            core.reset()
            // Detach the list while its rows go: a ListModel hands a row being
            // removed undefined roles, which only a mock does.
            page.messages = null
            core.messages.clear()
            page.messages = core.messages
            core.accounts.clear()
            core.folders.clear()
            core.listIsDrafts = false
            core.selectionHasLocal = false
            thm.name = "carbon"
            thm.darkMode = "light"
            opened.clear()
            draftOpened.clear()
            flagReq.clear(); replyAll.clear(); moveReq.clear(); contactReq.clear(); editReq.clear()
            readerCore.reset()
            composeCore.reset()
            waitForRendering(page)
        }
        function cleanup() {
            if (root.reader !== null) { root.reader.destroy(); root.reader = null }
            if (root.composePage !== null) { root.composePage.destroy(); root.composePage = null }
        }
        function openReader(draft) {
            root.reader = readerComponent.createObject(root, { messageId: 5, width: 400, height: 700 })
            readerCore.messageOpened(view(draft))
            waitForRendering(root.reader)
            return root.reader
        }

        // StatusChip
        function test_status_chip_is_24_px_tint_a_with_the_text() {
            const chip = chipComponent.createObject(root, { visible: true })
            compare(chip.height, 24)
            compare(chip.color, thm.tintA)
            compare(find("statusChipText", chip).text, "Not uploaded")
            verify(find("statusChipIcon", chip).visible)
            chip.destroy()
        }
        function test_status_chip_ink_is_the_theme_text_in_the_dark_theme() {
            const chip = chipComponent.createObject(root, { visible: true })
            thm.darkMode = "dark"
            compare(chip.color, thm.tintA)
            compare(find("statusChipText", chip).color, thm.text)
            chip.destroy()
        }

        // The Drafts list
        function test_chip_shows_only_for_a_row_that_is_not_uploaded() {
            core.messages.append(row(11, "d1", true))
            core.messages.append(row(12, "", false))
            waitForRendering(page)
            verify(shown("statusChip", find("row-11", page)), "the pseudo-row has the chip")
            verify(!shown("statusChip", find("row-12", page)), "the server row has none")
            compare(find("statusChipText", find("row-11", page)).text, "Not uploaded")
        }
        function test_row_menu_is_not_offered_on_a_local_draft() {
            core.messages.append(row(11, "d1", true))
            waitForRendering(page)
            const r = find("row-11", page)
            mouseClick(r, 40, r.height / 2, Qt.RightButton)
            wait(150)
            verify(!find("rowMenu", page).visible, "a local draft has no server copy to act on")
        }
        function test_chip_shows_the_reason_the_core_gives() {
            core.messages.append(row(11, "d1", true, false, "Server is full"))
            core.messages.append(row(12, "", false))
            waitForRendering(page)
            compare(find("statusChipText", find("row-11", page)).text, "Server is full")
            verify(!shown("statusChip", find("row-12", page)), "a server row has no chip")
        }
        function test_a_long_reason_elides_on_one_line_inside_the_row() {
            const wordy = "The server refused this draft because the mailbox is over its storage limit"
            core.messages.append(row(11, "d1", true, false, wordy))
            waitForRendering(page)
            const r = find("row-11", page)
            const chip = find("statusChip", r)
            const t = find("statusChipText", r)
            verify(chip.width <= r.width, "the chip fits the row")
            verify(t.truncated, "the text is elided")
            compare(t.lineCount, 1)
            compare(t.elide, Text.ElideRight)
        }
        function test_a_long_reason_on_a_bare_chip_elides_at_max_width() {
            const chip = chipComponent.createObject(root, { visible: true, maxWidth: 200,
                text: "A very long reason that cannot fit in two hundred pixels at all" })
            verify(chip !== null)
            verify(chip.width <= 200)
            verify(find("statusChipText", chip).truncated)
            compare(find("statusChipText", chip).lineCount, 1)
            chip.destroy()
        }
        function test_the_reason_ink_is_the_theme_text_in_the_dark_theme() {
            core.messages.append(row(11, "d1", true, false, "Server is full"))
            waitForRendering(page)
            thm.darkMode = "dark"
            compare(find("statusChipText", find("row-11", page)).color, thm.text)
            compare(find("statusChip", find("row-11", page)).color, thm.tintA)
        }
        function test_tapping_a_compose_row_asks_for_the_draft_not_the_reader() {
            core.messages.append(row(-1, "d1", true))
            waitForRendering(page)
            mouseClick(find("row--1", page))
            compare(draftOpened.count, 1)
            compare(draftOpened.signalArguments[0][0], "d1")
            compare(opened.count, 0)
        }
        function test_tapping_a_server_drafts_row_opens_the_reader() {
            core.messages.append(row(12, "", false))
            waitForRendering(page)
            mouseClick(find("row-12", page))
            compare(opened.count, 1)
            compare(opened.signalArguments[0][0], 12)
            compare(draftOpened.count, 0)
        }
        function test_in_selection_mode_a_compose_row_selects() {
            core.messages.append(row(-1, "d1", true))
            core.messages.append(row(12, "", false, true))
            waitForRendering(page)
            mouseClick(find("row--1", page))
            compare(core.called("select").pop().args, [-1, true])
            compare(draftOpened.count, 0)
        }

        // The selection bar
        function test_selection_bar_in_a_drafts_list_has_no_archive_or_move() {
            core.listIsDrafts = true
            core.messages.append(row(12, "", false, true))
            tryVerify(function() { return find("selectionBar", page).visible })
            verify(!shown("act-archive", page))
            verify(!shown("act-move", page))
            verify(shown("act-delete", page))
            verify(shown("act-more", page))
        }
        function test_selection_bar_with_a_local_draft_offers_only_delete() {
            core.listIsDrafts = true
            core.selectionHasLocal = true
            core.messages.append(row(-1, "d1", true, true))
            tryVerify(function() { return find("selectionBar", page).visible })
            verify(!shown("act-archive", page))
            verify(!shown("act-move", page))
            verify(!shown("act-more", page))
            verify(shown("act-delete", page))
        }
        function test_selection_bar_in_the_inbox_keeps_archive_and_move() {
            core.messages.append(row(12, "", false, true))
            tryVerify(function() { return find("selectionBar", page).visible })
            verify(shown("act-archive", page))
            verify(shown("act-move", page))
            verify(shown("act-more", page))
        }

        // The reader
        function test_reply_bar_of_a_draft_has_edit_and_no_reply_or_forward() {
            replyBar.isDraft = true
            waitForRendering(replyBar)
            verify(shown("edit", replyBar))
            verify(!shown("reply", replyBar))
            verify(!shown("forward", replyBar))
            mouseClick(find("edit", replyBar))
            compare(editReq.count, 1)
            replyBar.isDraft = false
        }
        function test_reply_bar_of_a_message_has_reply_and_forward_and_no_edit() {
            replyBar.isDraft = false
            waitForRendering(replyBar)
            verify(shown("reply", replyBar))
            verify(shown("forward", replyBar))
            verify(!shown("edit", replyBar))
        }
        function test_more_of_a_draft_has_no_reply_all_and_no_move() {
            more.isDraft = true
            compare(all("moreReplyAll", more).length, 0)
            compare(all("moreMove", more).length, 0)
            find("moreFlag", more).clicked()
            compare(flagReq.count, 1)
            compare(flagReq.signalArguments[0][0], true)
            find("moreUnflag", more).clicked()
            compare(flagReq.signalArguments[1][0], false)
            find("moreContact", more).clicked()
            compare(contactReq.count, 1)
            compare(replyAll.count, 0)
            compare(moveReq.count, 0)
            more.isDraft = false
        }
        function test_more_of_a_message_still_has_move_and_reply_all() {
            more.isDraft = false
            find("moreMove", more).clicked()
            compare(moveReq.count, 1)
            find("moreReplyAll", more).clicked()
            compare(replyAll.count, 1)
        }
        function test_reader_of_a_draft_has_no_archive_and_keeps_delete() {
            const r = openReader(true)
            verify(!shown("act-archive", r))
            verify(shown("act-delete", r))
            verify(shown("edit", r))
            verify(!shown("reply", r))
            verify(!shown("forward", r))
            save(r, "reader-draft-" + thm.name)
        }
        function test_reader_of_a_message_keeps_archive() {
            const r = openReader(false)
            verify(shown("act-archive", r))
            verify(shown("reply", r))
            verify(!shown("edit", r))
        }
        function test_edit_starts_an_edit_compose_for_this_message() {
            const r = openReader(true)
            const spy = createTemporaryObject(spyComponent, root, { target: r, signalName: "composeOpened" })
            mouseClick(find("edit", r))
            compare(readerCore.called("startCompose").pop().args, ["edit", 5, "ann", ""])
            compare(spy.count, 1)
        }

        // Compose
        function draftOf(subject, body) {
            return { account: "ann", draftId: "d1", to: ["Mara Becker <mara@example.org>"], cc: [], bcc: [],
                     subject: subject, body: body, inReplyTo: "", references: [], attachments: [] }
        }
        function test_compose_shows_the_note_when_there_is_one() {
            root.composePage = composeComponent.createObject(root, { width: 400, height: 700 })
            composeCore.composeReady(1, draftOf("Club newsletter, November", "Welcome back, everyone."), "",
                                     "Editing a copy. The original stays in Drafts.")
            waitForRendering(root.composePage)
            verify(shown("composeNote", root.composePage))
            compare(find("composeNote", root.composePage).text, "Editing a copy. The original stays in Drafts.")
            compare(find("composeNote", root.composePage).kind, "info")
            save(root.composePage, "compose-copy-" + thm.name)
        }
        function test_compose_hides_the_note_when_it_is_empty() {
            root.composePage = composeComponent.createObject(root, { width: 400, height: 700 })
            composeCore.composeReady(1, draftOf("", ""), "", "")
            waitForRendering(root.composePage)
            verify(!shown("composeNote", root.composePage))
        }
        function test_compose_note_goes_when_the_page_is_reused() {
            root.composePage = composeComponent.createObject(root, { width: 400, height: 700 })
            composeCore.composeReady(1, draftOf("", ""), "", "Editing a copy.")
            verify(shown("composeNote", root.composePage))
            root.composePage.reset()
            verify(!shown("composeNote", root.composePage))
        }

        // Screenshots (no assertions beyond the grab) to compare with the
        // boards: carbon-draft-edit, carbon-draft-refused, airmail-draft-refused.
        function test_zz_shots_of_the_drafts_list_in_both_themes() {
            core.messages.append(row(12, "", false))
            core.messages.append(row(-1, "d1", true))
            core.messages.append(row(13, "", false))
            for (const name of ["carbon", "airmail"]) {
                thm.name = name
                waitForRendering(page)
                save(page, "drafts-list-" + name)
            }
        }
        function test_zz_shots_of_the_reader_and_compose_in_airmail() {
            thm.name = "airmail"
            const r = openReader(true)
            save(r, "reader-draft-airmail")
            root.composePage = composeComponent.createObject(root, { width: 400, height: 700 })
            composeCore.composeReady(1, draftOf("Club newsletter, November", "Welcome back, everyone."), "",
                                     "Editing a copy. The original stays in Drafts.")
            save(root.composePage, "compose-copy-airmail")
        }
    }
}
