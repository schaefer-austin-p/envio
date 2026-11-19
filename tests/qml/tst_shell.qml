import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages"
import "../../qml/bindle/Platform" as Platform

// The app shell's compose wiring (which page opens, which one a late
// Queued closes, when a mailto: link starts) and platform/Uri.qml.
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
    Component { id: uriComponent; Platform.Uri { sourceFile: ""; schemes: ["mailto"] } }

    SignalSpy { id: links; signalName: "opened" }

    TestCase {
        name: "Shell"
        when: windowShown

        readonly property var ann: ({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "" })

        function message(id) {
            return { id: id, account: "ann", accountName: "Ann", from: "Lucia", fromAddress: "lucia@example.org", subject: "Plans " + id,
                     preview: "I found a train", date: 0, dateBucket: "time", unread: true,
                     flagged: false, hasAttachments: false, selected: false, open: false }
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
        // A booted shell of the given width (gu) with three messages.
        function bootWide(gu) {
            const b = boot()
            b.core.messages.append(message(7))
            b.core.messages.append(message(8))
            b.shell.width = thm.gu(gu)
            tryVerify(function() { return b.shell.depth === 1 })
            return b
        }

        // A shell on its own core, with an account unless told otherwise.
        function boot(withAccount) {
            const c = coreComponent.createObject(root)
            if (withAccount !== false) c.accounts.append(ann)
            const s = shellComponent.createObject(root, {
                theme: thm, core: c, accounts: c.accounts, folders: c.folders,
                messages: c.messages, outbox: c.outbox, suggestions: c.suggestions,
                searchResults: c.searchResults
            })
            c.ready = true
            return { core: c, shell: s }
        }

        // Depth-first search through data (popups included) and contentItem.
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

        // #329: Discard popped the page, and with it the dialog, from inside
        // the dialog button's onClicked, which segfaulted on the phone.
        function test_discarding_a_draft_pops_the_page_and_the_app_lives() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.composeOpened(1)
            compare(b.shell.depth, 2)
            const page = b.shell.currentPage
            b.core.composeReady(1, { account: "ann", draftId: "", to: [], cc: [], bcc: [], subject: "Hi", body: "Hello", inReplyTo: "", references: [], attachments: [] }, "", "")
            tryVerify(function() { return Math.abs(page.x) < 0.01 && page.opacity === 1 })
            tryVerify(function() { const m = findByName(page, "moreButton"); return m && m.width > 0 })
            mouseClick(findByName(page, "moreButton"))
            tryVerify(function() { const r = findByName(page, "discardRow"); return r && r.visible && r.width > 0 })
            mouseClick(findByName(page, "discardRow"))
            const ask = findByName(page, "discardAsk")
            tryVerify(function() { return ask.opened })
            const button = findByName(ask, "secondaryAction")
            // While the click runs the dialog must still be alive: run the
            // garbage collector and note the page's depth from inside it.
            let depthInClick = -1
            button.clicked.connect(function() { gc(); depthInClick = b.shell.depth })
            // The dialog must be shut before the signal that pops its page:
            // closing a popup whose page is going segfaulted on the phone
            // (#329, in QtQml's deferred execution; offscreen it does not).
            let openedAtSignal = true
            ask.secondaryActivated.connect(function() { openedAtSignal = ask.opened })
            tryVerify(function() { return button.width > 0 })
            mouseClick(button)
            tryVerify(function() { return b.shell.depth === 1 }, 3000)
            compare(openedAtSignal, false, "the dialog was closed before its signal")
            compare(depthInClick, 2, "the page was still on the stack during the click")
            compare(b.core.called("closeCompose").pop().args, [1, true])
            gc()
            wait(50)
            compare(b.shell.currentPage.objectName, "inboxPage")
            b.shell.destroy()
        }

        function test_new_message_pushes_compose() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.newMessage()
            compare(b.core.called("startCompose")[0].args, ["new", 0, "ann", ""])
            compare(b.shell.depth, 2)
            compare(b.shell.currentPage.compose, 1)
            b.shell.destroy()
        }

        // #228: a tap on a local draft's row starts Compose on that draft.
        function test_tapping_a_local_draft_row_starts_a_draft_compose() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.currentPage.draftRequested("d1")
            compare(b.core.called("startCompose")[0].args, ["draft", 0, "", "d1"])
            compare(b.shell.depth, 2)
            compare(b.shell.currentPage.compose, 1)
            b.shell.destroy()
        }

        function test_queued_closes_its_own_page_with_the_cores_text() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.composeOpened(1)
            compare(b.shell.depth, 2)
            b.core.queued(1, "Sent to the Outbox")
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.destroy()
        }

        function test_a_late_queued_does_not_close_another_compose() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.composeOpened(1)
            b.shell.composeOpened(2)
            compare(b.shell.depth, 3)
            // Compose 1 was sent; its Queued arrives after 2 was opened.
            b.core.queued(1, "Sent")
            wait(100)
            compare(b.shell.depth, 3)
            compare(b.shell.currentPage.compose, 2)
            // Back on compose 1 later: it is done and goes by itself.
            b.core.queued(2, "Sent")
            tryVerify(function() { return b.shell.depth === 1 }, 3000)
            b.shell.destroy()
        }

        function test_an_oauth_redirect_offered_to_the_uri_item_reaches_the_core() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.redirectUri.offer("envio-oauth:/cb?code=abc")
            compare(b.core.called("oauthRedirect").length, 1)
            compare(b.core.called("oauthRedirect")[0].args[0], "envio-oauth:/cb?code=abc")
            // Other schemes are not the redirect's.
            b.shell.redirectUri.offer("https://example.org/")
            compare(b.core.called("oauthRedirect").length, 1)
            b.shell.destroy()
        }
        function test_an_oauth_redirect_waits_until_the_core_is_ready() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.core.ready = false
            b.shell.redirectUri.offer("envio-oauth:/cb?code=abc")
            compare(b.core.called("oauthRedirect").length, 0)
            b.core.ready = true
            compare(b.core.called("oauthRedirect").length, 1)
            b.shell.destroy()
        }

        function test_mailto_waits_for_the_inbox_not_the_wizard() {
            const b = boot(false)
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.openMailto("mailto:bob@example.org")
            b.core.accounts.append(ann)
            tryVerify(function() { return b.shell.accountCount === 1 })
            wait(50)
            compare(b.core.called("startCompose").length, 0, "the wizard's Options step comes first")
            b.shell.wizardClosed()
            compare(b.core.called("startCompose").length, 1)
            compare(b.core.called("startCompose")[0].args, ["mailto", 0, "", "mailto:bob@example.org"])
            b.shell.destroy()
        }

        // What Content Hub hands over: items with file:// URLs.
        function newTransfer() {
            return { finalized: 0, finalize: function() { this.finalized++ }, items: [
                { url: "file:///home/u/.cache/hub/a%20b.pdf" },
                { url: "file:///home/u/.cache/hub/c.png" }
            ] }
        }
        readonly property var draft: ({ account: "ann", to: [], cc: [], bcc: [], subject: "", body: "",
                                        inReplyTo: "", references: [], attachments: [] })

        function test_a_shared_transfer_opens_compose_with_its_files() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.shares.accept(newTransfer())
            compare(b.core.called("startCompose")[0].args, ["new", 0, "ann", ""])
            compare(b.shell.depth, 2)
            compare(b.shell.currentPage.compose, 1)
            b.core.composeReady(1, draft, "", "")
            compare(b.core.called("attach").map(function(c) { return c.args }),
                    [[1, "/home/u/.cache/hub/a b.pdf"], [1, "/home/u/.cache/hub/c.png"]])
            b.shell.destroy()
        }

        function test_files_are_attached_only_after_composeReady() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.shares.accept(newTransfer())
            wait(50)
            compare(b.core.called("attach").length, 0, "the worker does not know the compose yet")
            b.core.composeReady(2, draft, "", "")
            compare(b.core.called("attach").length, 0, "another compose's ready")
            b.core.composeReady(1, draft, "", "")
            compare(b.core.called("attach").length, 2)
            b.core.composeReady(1, draft, "", "")
            compare(b.core.called("attach").length, 2, "attached once")
            b.shell.destroy()
        }

        function test_the_transfer_is_finalized_when_every_file_settled() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            const t = newTransfer()
            b.shell.shares.accept(t)
            b.core.composeReady(1, draft, "", "")
            b.core.attached(1, "/d/a b.pdf", "a b.pdf", 3)
            compare(t.finalized, 0, "one file is still being copied")
            b.core.attachFailed(1, "unreadable")
            compare(t.finalized, 1)
            b.core.attached(1, "/d/other.pdf", "other.pdf", 3)
            compare(t.finalized, 1, "once")
            b.shell.destroy()
        }

        function test_a_picked_file_does_not_finalize_the_transfer_early() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            const t = newTransfer()
            b.shell.shares.accept(t)
            b.core.composeReady(1, draft, "", "")
            // The user attaches one more file while the shared ones copy.
            b.shell.currentPage.pick(["/p/picked.pdf"])
            b.core.attached(1, "/d/picked.pdf", "picked.pdf", 3)
            b.core.attached(1, "/d/a b.pdf", "a b.pdf", 3)
            compare(t.finalized, 0, "one shared file is still being copied")
            b.core.attached(1, "/d/c.png", "c.png", 3)
            compare(t.finalized, 1)
            b.shell.destroy()
        }

        function test_the_pickers_transfer_is_finalized_once_its_files_settled() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.composeOpened(1)
            b.core.composeReady(1, draft, "", "")
            const t = newTransfer()
            b.shell.currentPage.pick(["/p/a.pdf", "/p/b.png"], t)
            compare(t.finalized, 0, "the copies are not made yet")
            b.core.attached(1, "/d/a.pdf", "a.pdf", 3)
            compare(t.finalized, 0, "one file is still being copied")
            b.core.attachFailed(1, "unreadable")
            compare(t.finalized, 1)
            b.core.attached(1, "/d/other.pdf", "other.pdf", 3)
            compare(t.finalized, 1, "once")
            b.shell.destroy()
        }

        function test_a_picker_transfer_with_no_files_is_finalized_at_once() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.composeOpened(1)
            b.core.composeReady(1, draft, "", "")
            const t = newTransfer()
            b.shell.currentPage.pick([], t)
            compare(t.finalized, 1)
            b.shell.destroy()
        }

        function test_a_refused_compose_finalizes_and_attaches_nothing() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            const t = newTransfer()
            b.shell.shares.accept(t)
            b.core.composeRefused(1, "no account")
            compare(t.finalized, 1)
            b.core.composeReady(1, draft, "", "")
            compare(b.core.called("attach").length, 0)
            b.shell.destroy()
        }

        function test_back_before_composeReady_attaches_nothing_and_finalizes() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            const t = newTransfer()
            b.shell.shares.accept(t)
            b.shell.currentPage.leave()
            compare(t.finalized, 1)
            b.core.composeReady(1, draft, "", "")
            compare(b.core.called("attach").length, 0, "no orphan draft")
            b.shell.destroy()
        }

        function test_a_transfer_without_files_is_finalized_at_once() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            const t = { finalized: 0, finalize: function() { this.finalized++ }, items: [] }
            b.shell.shares.accept(t)
            compare(t.finalized, 1)
            b.shell.destroy()
        }

        function test_the_hook_offers_envio_to_share_and_open_with() {
            // "share" is the share sheets, "destination" the File Manager's
            // "Open with another app".
            const xhr = new XMLHttpRequest()
            xhr.open("GET", Qt.resolvedUrl("../../envio.content-hub"), false)
            xhr.send()
            const hook = JSON.parse(xhr.responseText)
            verify(hook.share.length > 0)
            for (const type of hook.share)
                verify(hook.destination.indexOf(type) >= 0, type)
            // "all" is what the hub knows for a file of any type (an archive,
            // a file with no extension): without it the File Manager does not
            // offer Envio for those.
            verify(hook.destination.indexOf("all") >= 0)
        }

        function test_the_share_receiver_compiles() {
            // Compiled, never created: creating it registers a Content Hub
            // handler on the bus. (A missing Lomiri.Content is an Error too.)
            const c = Qt.createComponent("../../qml/platform/ut/ShareReceiver.qml")
            tryVerify(function() { return c.status !== Component.Loading })
            verify(c.status === Component.Ready, c.errorString())
            c.destroy()
        }

        function test_a_share_before_the_core_is_ready_waits_for_it() {
            const c = coreComponent.createObject(root)
            c.accounts.append(ann)
            const s = shellComponent.createObject(root, {
                theme: thm, core: c, accounts: c.accounts, folders: c.folders,
                messages: c.messages, outbox: c.outbox, suggestions: c.suggestions,
                searchResults: c.searchResults
            })
            s.shares.accept(newTransfer())
            compare(c.called("startCompose").length, 0)
            c.ready = true
            c.composeReady(1, draft, "", "")
            tryVerify(function() { return c.called("attach").length === 2 })
            compare(c.called("startCompose").length, 1)
            s.destroy()
        }

        function test_a_share_without_an_account_shows_the_wizard_and_waits() {
            const b = boot(false)
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.shares.accept(newTransfer())
            compare(b.shell.currentPage.objectName, "wizardPage")
            compare(b.core.called("startCompose").length, 0)
            b.core.accounts.append(ann)
            tryVerify(function() { return b.shell.accountCount === 1 })
            wait(50)
            compare(b.core.called("attach").length, 0, "the wizard's Options step comes first")
            b.shell.wizardClosed()
            b.core.composeReady(1, draft, "", "")
            compare(b.core.called("attach").length, 2)
            b.shell.destroy()
        }

        function test_an_empty_transfer_opens_nothing() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.shares.accept({ items: [] })
            compare(b.core.called("startCompose").length, 0)
            b.shell.destroy()
        }

        // bindle's Uri defaults to no schemes; without this the shell would
        // silently ignore mailto: links.
        function test_shell_registers_the_mailto_scheme() {
            const b = boot()
            verify(b.shell.uriSchemes.indexOf("mailto") >= 0)
            b.shell.destroy()
        }

        function test_uri_offers_only_mailto_links() {
            const u = uriComponent.createObject(root, { arguments: [] })
            links.target = u
            links.clear()
            u.offer("mailto:bob@example.org")
            u.offer("MAILTO:ann@example.org")
            u.offer("https://example.org/")
            u.offer("file:///etc/passwd")
            u.offer("xmailto:bob@example.org")
            compare(links.count, 2)
            compare(links.signalArguments[0][0], "mailto:bob@example.org")
            compare(links.signalArguments[1][0], "MAILTO:ann@example.org")
            u.destroy()
        }

        function test_uri_scans_the_command_line() {
            links.clear()
            const u = uriComponent.createObject(root, {
                arguments: ["envio", "--flag", "mailto:cmd@example.org", "/tmp/x", "http://example.org"]
            })
            links.target = u
            links.clear()
            // The scan runs later, so a handler set up after creation hears it.
            tryVerify(function() { return links.count === 1 })
            compare(links.signalArguments[0][0], "mailto:cmd@example.org")
            u.destroy()
        }

        // Two pages never show together: once the arriving page starts to
        // show, the page that leaves is already hidden, and it comes back
        // whole on Back. Checked against the arriving page's first frame,
        // not a wall-clock limit, so a slow machine can't make it flaky.
        function test_the_leaving_page_hides_at_once() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            const inbox = b.shell.currentPage
            tryCompare(inbox, "opacity", 1)
            inbox.settingsRequested()
            const settings = b.shell.currentPage
            compare(settings.objectName, "settingsPage")
            tryVerify(function() { return settings.opacity > 0 })
            verify(inbox.opacity === 0 || !inbox.visible, "the leaving page shows alongside")
            tryCompare(settings, "opacity", 1)
            tryVerify(function() { return Math.abs(settings.x) < 0.01 })
            settings.backRequested()
            tryVerify(function() { return inbox.opacity > 0 })
            verify(settings.opacity === 0 || !settings.visible, "the leaving page shows alongside")
            tryCompare(inbox, "opacity", 1)
            verify(inbox.visible)
            tryVerify(function() { return Math.abs(inbox.x) < 0.01 })
            compare(b.shell.depth, 1)
            b.shell.destroy()
        }

        function test_search_and_settings_push_their_pages() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.currentPage.searchRequested(7)
            compare(b.shell.depth, 2)
            compare(b.shell.currentPage.objectName, "searchPage")
            compare(b.shell.currentPage.folderId, 7)
            b.shell.currentPage.backRequested()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.currentPage.settingsRequested()
            compare(b.shell.currentPage.objectName, "settingsPage")
            b.shell.currentPage.appearanceRequested()
            compare(b.shell.currentPage.objectName, "appearancePage")
            b.shell.currentPage.backRequested()
            tryVerify(function() { return b.shell.currentPage.objectName === "settingsPage" })
            b.shell.currentPage.accountsRequested()
            compare(b.shell.depth, 3)
            b.shell.destroy()
        }

        function test_narrow_shows_the_stack() {
            const b = bootWide(70)
            compare(b.shell.wide, false)
            compare(b.shell.depth, 1)
            b.shell.destroy()
        }

        function test_wide_shows_list_and_empty_pane() {
            const b = bootWide(100)
            compare(b.shell.wide, true)
            const empty = find(b.shell, "emptyReader")
            verify(empty !== null, "no empty pane")
            tryVerify(function() { return empty.visible })
            compare(empty.shortcutHint, qsTr("Or press %1 to write one."))
            b.shell.destroy()
        }

        function test_tap_row_opens_in_pane_without_push() {
            const b = bootWide(100)
            b.shell.currentPage.messageRequested(7)
            compare(b.shell.depth, 1)
            compare(b.core.openMessage, 7)
            compare(b.core.messages.get(0).open, true)
            compare(b.core.messages.get(1).open, false)
            compare(find(b.shell, "readerPage") !== null, true)
            b.shell.destroy()
        }

        function test_narrow_tap_row_still_pushes() {
            const b = bootWide(70)
            b.shell.currentPage.messageRequested(7)
            compare(b.shell.depth, 2)
            compare(b.core.called("setOpenMessage").length, 0)
            b.shell.destroy()
        }

        function test_hysteresis() {
            const b = bootWide(80)
            compare(b.shell.wide, false)
            b.shell.width = thm.gu(90)
            compare(b.shell.wide, true)
            b.shell.width = thm.gu(88)
            compare(b.shell.wide, true)
            b.shell.width = thm.gu(85)
            compare(b.shell.wide, false)
            b.shell.width = thm.gu(88)
            compare(b.shell.wide, false)
            b.shell.destroy()
        }

        function test_close_clears_the_open_message() {
            const b = bootWide(100)
            b.shell.currentPage.messageRequested(7)
            find(b.shell, "readerPage").backRequested()
            compare(b.core.openMessage, 0)
            compare(b.core.called("setOpenMessage").pop().args, [0])
            b.shell.destroy()
        }

        function test_settings_covers_both_panes() {
            const b = bootWide(100)
            b.shell.currentPage.settingsRequested()
            compare(b.shell.depth, 2)
            compare(b.shell.currentPage.objectName, "settingsPage")
            tryCompare(b.shell.currentPage, "width", b.shell.frameArea.width)
            verify(!find(b.shell, "emptyReader").visible, "the pane shows over the settings page")
            b.shell.destroy()
        }

        function test_folder_change_clears_open() {
            const b = bootWide(100)
            b.shell.currentPage.messageRequested(7)
            compare(b.core.openMessage, 7)
            b.shell.currentPage.showFolder(3, "Archive")
            // The core clears it (Rust's open_list); the shell does not call setOpenMessage(0) too.
            compare(b.core.openMessage, 0)
            compare(b.core.called("setOpenMessage").length, 1)
            tryVerify(function() { return find(b.shell, "emptyReader").visible })
            b.shell.destroy()
        }

        function test_retapping_the_shown_folder_keeps_open() {
            const b = bootWide(100)
            b.shell.currentPage.showFolder(3, "Archive")
            b.shell.currentPage.messageRequested(7)
            compare(b.core.openMessage, 7)
            b.shell.currentPage.showFolder(3, "Archive")
            compare(b.core.called("openList").length, 2)
            compare(b.core.openMessage, 7)
            verify(find(b.shell, "readerPage") !== null, "the reader closed")
            b.shell.destroy()
        }

        function test_pane_shows_compose_while_paneCompose_set() {
            const b = bootWide(100)
            b.shell.currentPage.messageRequested(7)
            b.shell.paneCompose = 4
            compare(find(b.shell, "readerPage"), null)
            const page = find(b.shell, "composePage")
            verify(page !== null, "no compose in the pane")
            compare(page.compose, 4)
            b.shell.paneCompose = 0
            verify(find(b.shell, "readerPage") !== null, "the reader does not return")
            b.shell.currentPage.showFolder(3, "Archive")
            verify(find(b.shell, "emptyReader") !== null)
            b.shell.destroy()
        }

        function test_empty_pane_text_icon_and_keys() {
            const b = bootWide(100)
            const empty = find(b.shell, "emptyReader")
            compare(find(empty, "emptyIcon").name, "mailopen")
            const title = find(empty, "emptyTitle")
            compare(title.font.pixelSize, 18)
            compare(title.font.family, thm.fontUi)
            verify(!title.text.endsWith("."), "the title has a full stop")
            verify(empty.shortcutHint.endsWith("."), "the hint has no full stop")
            let keys = []
            ;(function walk(it) {
                if (it.objectName === "keyChip") keys.push(it.text)
                for (let i = 0; i < it.children.length; i++) walk(it.children[i])
            })(empty)
            compare(keys, [qsTr("Ctrl"), "N"])
            b.shell.destroy()
        }

        function test_hairline_between_list_and_pane() {
            const b = bootWide(100)
            const line = find(b.shell, "paneHairline")
            verify(line !== null && line.visible, "no hairline")
            compare(line.width, 1)
            compare(line.x, thm.gu(50))
            compare(line.color, thm.hairline)
            b.shell.destroy()
        }
        function test_wide_list_ends_at_the_hairline() {
            const b = bootWide(100)
            const line = find(b.shell, "paneHairline")
            const page = b.shell.currentPage
            const right = page.mapToItem(b.shell, page.width, 0).x
            verify(right <= line.x, "list right edge " + right + " is past the hairline at " + line.x)
            const stack = find(b.shell, "stack")
            verify(stack !== null, "no stack")
            verify(stack.width <= line.x, "stack is " + stack.width + " wide, hairline at " + line.x)
            b.shell.destroy()
        }
        function test_wide_pane_is_opaque() {
            const b = bootWide(100)
            const pane = find(b.shell, "pane")
            const bg = find(pane, "paneBackground")
            verify(bg !== null, "pane has no background")
            compare(bg.color, thm.ground)
            compare(bg.opacity, 1)
            compare(bg.width, pane.width)
            b.shell.destroy()
        }
        function test_wide_search_keeps_the_full_width() {
            const b = bootWide(100)
            b.shell.currentPage.searchRequested(0)
            tryVerify(function() { return b.shell.depth === 2 })
            tryVerify(function() { return b.shell.currentPage.width >= thm.gu(100) - 2 },
                      1000, "search is " + b.shell.currentPage.width + " wide")
            b.shell.destroy()
        }
        function test_no_hairline_when_narrow() {
            const b = bootWide(70)
            verify(!find(b.shell, "paneHairline").visible)
            b.shell.destroy()
        }

        function test_second_compose_closes_the_first() {
            const b = bootWide(100)
            b.shell.composeOpened(1)
            b.core.composeReady(1, { to: [], cc: [], bcc: [], subject: "one", body: "", account: "ann", attachments: [] }, "", "")
            b.core.reset()
            b.shell.composeOpened(2)
            compare(b.core.called("closeCompose").map(function(c) { return c.args }), [[1, false]])
            compare(b.shell.paneCompose, 2)
            const page = find(b.shell, "composePage")
            compare(page.compose, 2)
            verify(!page.filled, "the new compose shows the old one's fields")
            b.core.reset()
            page.edited()
            compare(b.core.called("draftChanged").length, 0)
            b.shell.destroy()
        }

        readonly property var composeDraft: ({ account: "ann", to: ["Bob <bob@example.org>"], cc: [], bcc: [], subject: "Plans",
            body: "Hello", inReplyTo: "", references: [], attachments: ["/tmp/a.pdf"] })

        // Compose 4 open in the pane at 100 gu, edited, with an attachment.
        function wideCompose() {
            const b = bootWide(100)
            b.shell.composeOpened(4)
            b.core.composeReady(4, composeDraft, "On Monday Bob wrote:\n> hi", "")
            const page = find(b.shell, "composePage")
            find(page, "subjectField").text = "Plans, edited"
            find(page, "bodyField").text = "Hello again"
            b.core.reset()
            return b
        }

        function test_narrowing_moves_the_compose_to_the_stack() {
            const b = wideCompose()
            b.shell.width = thm.gu(70)
            compare(b.shell.wide, false)
            compare(b.shell.paneCompose, 0)
            compare(b.shell.depth, 2)
            const page = b.shell.currentPage
            compare(page.objectName, "composePage")
            compare(page.compose, 4)
            verify(page.visible)
            // The full width of the window, not a sliver of the old pane.
            verify(page.width >= thm.gu(70) - 2, "compose is " + page.width + " wide")
            const sent = JSON.parse(page.draftJson())
            compare(sent.subject, "Plans, edited")
            compare(sent.to, ["Bob <bob@example.org>"])
            compare(sent.body, "Hello again\n\nOn Monday Bob wrote:\n> hi")
            compare(sent.attachments, ["/tmp/a.pdf"])
            compare(b.core.called("closeCompose").length, 0)
            compare(b.core.called("draftChanged").length, 0)
            b.shell.width = thm.gu(45)
            verify(b.shell.currentPage.width >= thm.gu(45) - 2, "compose is " + b.shell.currentPage.width + " wide at 45 gu")
            b.shell.destroy()
        }

        function test_compose_content_survives_narrow_then_wide() {
            const b = wideCompose()
            b.shell.width = thm.gu(70)
            b.shell.width = thm.gu(100)
            compare(b.shell.wide, true)
            const page = b.shell.currentPage
            compare(page.objectName, "composePage")
            compare(page.compose, 4)
            verify(page.width >= thm.gu(50), "compose is " + page.width + " wide")
            compare(JSON.parse(page.draftJson()).subject, "Plans, edited")
            compare(JSON.parse(page.draftJson()).attachments, ["/tmp/a.pdf"])
            compare(b.core.called("closeCompose").length, 0)
            b.shell.destroy()
        }

        function test_narrowing_while_sending_keeps_send_disabled() {
            const b = wideCompose()
            find(b.shell, "composePage").send()
            compare(b.core.called("send").length, 1)
            b.shell.width = thm.gu(70)
            const page = b.shell.currentPage
            compare(page.sending, true)
            compare(find(page, "sendButton").enabled, false)
            b.shell.destroy()
        }

        function test_narrowing_keeps_the_compose_error() {
            const b = wideCompose()
            find(b.shell, "composePage").error = "No recipient"
            b.shell.width = thm.gu(70)
            compare(b.shell.currentPage.error, "No recipient")
            b.shell.destroy()
        }

        function test_grid_unit_change_rechecks_the_layout() {
            const b = boot()
            b.shell.width = thm.gu(70)
            compare(b.shell.wide, false)
            // Pretend the last check saw a wide window; only the signal of the
            // grid unit (a Suru change on the phone) can correct it here.
            b.shell._wide = true
            compare(b.shell.wide, true)
            thm.gridUnitChanged()
            compare(b.shell.wide, false)
            b.shell.destroy()
        }

        function test_the_pane_composes_picker_transfer_is_finalized() {
            const b = bootWide(100)
            b.shell.composeOpened(1)
            b.core.composeReady(1, draft, "", "")
            const t = newTransfer()
            find(b.shell, "composePage").pick(["/p/a.pdf", "/p/b.png"], t)
            compare(t.finalized, 0, "the copies are not made yet")
            b.core.attached(1, "/d/a.pdf", "a.pdf", 3)
            compare(t.finalized, 0, "one file is still being copied")
            b.core.attachFailed(1, "unreadable")
            compare(t.finalized, 1)
            b.shell.destroy()
        }

        function test_an_expanded_quote_stays_expanded_after_narrowing() {
            const b = bootWide(100)
            b.shell.composeOpened(1)
            b.core.composeReady(1, draft, "On Monday Bob wrote:\n> hi", "")
            find(find(b.shell, "composePage"), "quoteShow").clicked()
            verify(find(find(b.shell, "composePage"), "quoteText").visible)
            b.shell.width = thm.gu(70)
            verify(find(b.shell.currentPage, "quoteText").visible, "the quote folded again")
            b.shell.destroy()
        }

        function test_back_from_the_moved_compose_returns_to_the_list() {
            const b = wideCompose()
            b.shell.width = thm.gu(70)
            b.shell.currentPage.leave()
            tryCompare(b.shell, "depth", 1)
            compare(b.core.called("closeCompose").map(function(c) { return c.args }), [[4, false]])
            b.shell.destroy()
        }

        function test_pane_compose_is_never_narrower_than_36_gu() {
            const b = wideCompose()
            for (const w of [90, 88, 86, 85, 70, 45]) {
                b.shell.width = thm.gu(w)
                const pane = find(b.shell, "pane")
                if (b.shell.wide) verify(pane.width >= thm.gu(36), "pane is " + pane.width + " at " + w + " gu")
                else compare(b.shell.paneCompose, 0)
                compare(b.shell.wide, w >= 86)
            }
            b.shell.destroy()
        }

        function test_second_compose_while_narrowed_goes_on_the_stack() {
            const b = wideCompose()
            b.shell.width = thm.gu(70)
            b.shell.composeOpened(5)
            compare(b.shell.depth, 3)
            compare(b.shell.currentPage.compose, 5)
            verify(b.shell.currentPage.visible)
            compare(b.core.called("closeCompose").length, 0)
            b.shell.destroy()
        }

        function test_narrowing_clears_the_open_message() {
            const b = bootWide(100)
            b.shell.currentPage.messageRequested(7)
            compare(b.core.openMessage, 7)
            b.shell.width = thm.gu(80)
            compare(b.shell.wide, false)
            compare(b.core.openMessage, 0)
            compare(b.core.messages.get(0).open, false)
            b.shell.destroy()
        }

        function test_ctrl_n_opens_a_new_message_in_the_pane() {
            const b = bootWide(100)
            verify(b.shell.newShortcut.enabled)
            b.shell.newShortcut.activated()
            compare(b.core.called("startCompose")[0].args, ["new", 0, "ann", ""])
            compare(b.shell.paneCompose, 1)
            // With compose open the key does nothing more.
            verify(!b.shell.newShortcut.enabled)
            b.shell.destroy()
        }

        function test_wide_compose_is_inset_and_capped() {
            const b = bootWide(200)
            b.shell.composeOpened(4)
            b.core.composeReady(4, { to: [], cc: [], bcc: [], subject: "", body: "", account: "ann", attachments: [] }, "", "")
            const page = find(b.shell, "composePage")
            const to = find(page, "to")
            compare(to.mapToItem(page, 0, 0).x, thm.gu(5))
            compare(to.width, thm.gu(90))
            b.shell.destroy()
        }

        function test_compose_opens_in_the_pane_when_wide() {
            const b = bootWide(100)
            b.shell.composeOpened(5)
            compare(b.shell.depth, 1)
            compare(b.shell.paneCompose, 5)
            b.shell.destroy()
        }
    }
}
