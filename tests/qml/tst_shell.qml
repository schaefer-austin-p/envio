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
                     flagged: false, hasAttachments: false, selected: false, open: false, kbdFocus: false }
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
            b.shell.redirectUri.offer("enviooauth:/cb?code=abc")
            compare(b.core.called("oauthRedirect").length, 1)
            compare(b.core.called("oauthRedirect")[0].args[0], "enviooauth:/cb?code=abc")
            // Other schemes are not the redirect's.
            b.shell.redirectUri.offer("https://example.org/")
            compare(b.core.called("oauthRedirect").length, 1)
            b.shell.destroy()
        }
        function test_an_oauth_redirect_waits_until_the_core_is_ready() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.core.ready = false
            b.shell.redirectUri.offer("enviooauth:/cb?code=abc")
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

        function test_a_notification_click_opens_that_accounts_inbox() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 && b.shell.inboxItem !== null })
            b.shell.currentPage.settingsRequested()
            compare(b.shell.depth, 2)
            b.core.openInboxRequested("ann")
            compare(b.shell.depth, 1, "the page above the list closed")
            compare(b.core.called("listFolders").length, 1)
            compare(b.core.called("listFolders")[0].args, ["ann"])
            compare(b.shell.inboxItem.tab, "ann")
            // An account that is gone does nothing.
            b.core.openInboxRequested("gone")
            compare(b.core.called("listFolders").length, 1)
            b.shell.destroy()
        }

        function test_a_notification_click_over_a_stacked_compose_leaves_it_alone() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 && b.shell.inboxItem !== null })
            b.shell.openMessageFromRow(7)
            compare(b.shell.depth, 2)
            b.shell.composeOpened(1)
            compare(b.shell.depth, 3)
            b.core.openInboxRequested("ann")
            compare(b.shell.depth, 3, "the reader and its reply stay")
            compare(b.core.called("listFolders").length, 0)
            b.shell.destroy()
        }

        function test_a_notification_click_over_a_pane_compose_leaves_it_alone() {
            const b = bootWide(100)
            b.shell.composeOpened(1)
            compare(b.shell.paneCompose, 1)
            b.core.openInboxRequested("ann")
            compare(b.shell.paneCompose, 1)
            compare(b.core.called("listFolders").length, 0)
            b.shell.destroy()
        }

        function test_a_notification_click_over_the_add_account_wizard_is_ignored() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 && b.shell.inboxItem !== null })
            b.shell.currentPage.accountsRequested()
            compare(b.shell.depth, 2)
            b.shell.currentPage.addRequested()
            compare(b.shell.currentPage.objectName, "wizardPage")
            b.core.openInboxRequested("ann")
            compare(b.shell.depth, 3)
            compare(b.core.called("listFolders").length, 0)
            b.shell.destroy()
        }

        function test_a_click_before_the_start_fires_once_after_route() {
            const c = coreComponent.createObject(root)
            c.accounts.append(ann)
            const s = shellComponent.createObject(root, {
                theme: thm, core: c, accounts: c.accounts, folders: c.folders,
                messages: c.messages, outbox: c.outbox, suggestions: c.suggestions,
                searchResults: c.searchResults
            })
            c.openInboxRequested("ann")
            compare(c.called("listFolders").length, 0, "the core is not up yet")
            c.ready = true
            tryVerify(function() { return s.depth === 1 })
            compare(c.called("listFolders").length, 1)
            compare(c.called("listFolders")[0].args, ["ann"])
            // A later ready change does not replay it.
            c.ready = false
            c.ready = true
            wait(50)
            compare(c.called("listFolders").length, 1)
            s.destroy()
        }

        function test_a_click_that_cannot_apply_after_the_start_is_dropped() {
            const c = coreComponent.createObject(root)
            const s = shellComponent.createObject(root, {
                theme: thm, core: c, accounts: c.accounts, folders: c.folders,
                messages: c.messages, outbox: c.outbox, suggestions: c.suggestions,
                searchResults: c.searchResults
            })
            c.openInboxRequested("ann")
            c.ready = true
            tryVerify(function() { return s.depth === 1 && s.rootKind === "wizard" })
            c.accounts.append(ann)
            tryVerify(function() { return s.accountCount === 1 })
            s.wizardClosed()
            tryVerify(function() { return s.rootKind === "inbox" && s.inboxItem !== null })
            c.ready = false
            c.ready = true
            wait(50)
            compare(c.called("listFolders").length, 0, "a stale request never jumps the user later")
            s.destroy()
        }

        function test_a_notification_click_over_the_sign_in_again_page_leaves_it_alone() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 && b.shell.inboxItem !== null })
            b.shell.currentPage.accountsRequested()
            compare(b.shell.depth, 2)
            b.shell.currentPage.signInAgainRequested("ann")
            compare(b.shell.currentPage.objectName, "signInAgainPage")
            b.core.openInboxRequested("ann")
            compare(b.shell.depth, 3, "an OAuth sign-in in progress stays")
            compare(b.core.called("listFolders").length, 0)
            b.shell.destroy()
        }

        function test_a_notification_click_over_the_accounts_page_leaves_it_alone() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 && b.shell.inboxItem !== null })
            b.shell.currentPage.accountsRequested()
            compare(b.shell.depth, 2)
            b.core.openInboxRequested("ann")
            compare(b.shell.depth, 2, "a rename or delete dialog may be open")
            compare(b.core.called("listFolders").length, 0)
            b.shell.destroy()
        }

        function test_a_notification_click_over_the_wizard_is_ignored() {
            const b = boot(false)
            tryVerify(function() { return b.shell.depth === 1 })
            b.core.openInboxRequested("ann")
            compare(b.core.called("listFolders").length, 0)
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

        function test_a_refused_unfilled_compose_closes_and_toasts_the_reason() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.composeOpened(1)
            compare(b.shell.depth, 2)
            b.core.composeRefused(1, "That message is no longer available")
            tryVerify(function() { return b.shell.depth === 1 })
            compare(findByName(b.shell, "sentToast").text, "That message is no longer available")
            b.shell.destroy()
        }

        function test_a_refused_buried_compose_leaves_the_top_compose_alone_data() {
            return [{ tag: "first is composeItem", opens: [1, 2], refuse: 1, top: 2 },
                    { tag: "not composeItem", opens: [1, 2, 3], refuse: 2, top: 3 }]
        }
        function test_a_refused_buried_compose_leaves_the_top_compose_alone(data) {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            for (const c of data.opens) b.shell.composeOpened(c)
            const depth = b.shell.depth
            compare(depth, data.opens.length + 1)
            const top = b.shell.currentPage
            b.core.composeRefused(data.refuse, "That message is no longer available")
            wait(100)
            compare(b.shell.depth, depth, "the buried page waits under the top one")
            verify(b.shell.currentPage === top)
            compare(b.shell.currentPage.compose, data.top)
            compare(b.core.called("closeCompose").length, 0, "the other compose stays open in the core")
            compare(findByName(b.shell, "sentToast").text, "That message is no longer available")
            // The top compose ends; the refused page is shown again and goes.
            b.core.queued(data.top, "Sent")
            tryVerify(function() { return b.shell.depth === depth - 2 }, 3000)
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

        // Third column (T7): the folders drawer pinned open from 180 gu.
        function test_very_wide_shows_inline_drawer() {
            const b = bootWide(200)
            compare(b.shell.veryWide, true)
            const col = find(b.shell, "inlineDrawer")
            verify(col !== null && col.visible, "no inline drawer")
            compare(col.inline, true)
            compare(col.width, thm.gu(30))
            // The list and the pane sit to its right.
            const page = b.shell.currentPage
            verify(page.mapToItem(b.shell, 0, 0).x >= col.width, "list overlaps the drawer")
            const line = find(b.shell, "paneHairline")
            compare(line.x, thm.gu(80))
            b.shell.destroy()
        }

        function test_wide_but_not_very_wide_uses_overlay_drawer() {
            const b = bootWide(150)
            compare(b.shell.wide, true)
            compare(b.shell.veryWide, false)
            const col = find(b.shell, "inlineDrawer")
            verify(col === null || !col.visible, "inline drawer shown")
            compare(b.shell.currentPage.drawerInline, false)
            b.shell.destroy()
        }

        function test_menu_button_hidden_when_drawer_inline() {
            const b = bootWide(150)
            verify(find(b.shell, "menuButton").visible, "no menu button when overlay")
            b.shell.width = thm.gu(200)
            compare(b.shell.currentPage.drawerInline, true)
            verify(!find(b.shell, "menuButton").visible, "menu button with inline drawer")
            b.shell.destroy()
        }

        function test_inline_folder_pick_clears_open_message() {
            const b = bootWide(200)
            b.shell.currentPage.messageRequested(7)
            compare(b.core.openMessage, 7)
            find(b.shell, "inlineDrawer").folderRequested(3, "Archive")
            compare(b.core.openMessage, 0)
            compare(b.core.called("openList").length > 0, true)
            compare(b.shell.currentPage.title, "Archive")
            tryVerify(function() { return find(b.shell, "emptyReader").visible })
            // Never both: the overlay stays closed.
            verify(!b.shell.currentPage.overlayDrawerVisible, "overlay drawer shown")
            b.shell.destroy()
        }

        // The pinned column marks the shown list: Unified inbox, then the
        // picked folder instead.
        function test_inline_drawer_marks_the_shown_folder() {
            const b = bootWide(200)
            const col = find(b.shell, "inlineDrawer")
            compare(col.unifiedSelected, true)
            compare(col.selectedFolder, -1)
            col.folderRequested(3, "Archive")
            compare(col.unifiedSelected, false)
            compare(col.selectedFolder, 3)
            col.unifiedRequested()
            compare(col.unifiedSelected, true)
            compare(col.selectedFolder, -1)
            b.shell.destroy()
        }

        function test_veryWide_hysteresis() {
            const b = bootWide(170)
            compare(b.shell.veryWide, false)
            b.shell.width = thm.gu(180)
            compare(b.shell.veryWide, true)
            b.shell.width = thm.gu(178)
            compare(b.shell.veryWide, true)
            b.shell.width = thm.gu(175)
            compare(b.shell.veryWide, false)
            b.shell.width = thm.gu(178)
            compare(b.shell.veryWide, false)
            // Narrow means not very wide either.
            b.shell.width = thm.gu(200)
            compare(b.shell.veryWide, true)
            b.shell.width = thm.gu(70)
            compare(b.shell.veryWide, false)
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
            // The reader waits, hidden, under the compose.
            verify(!b.shell.readerItem.visible, "the reader shows under the compose")
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

        // #228 with #351's single compose item: wide, a draft tap opens it in
        // the pane, with the core's note; narrowing moves it to the stack.
        function test_wide_draft_tap_opens_the_pane_compose_with_its_note() {
            const b = bootWide(100)
            b.shell.currentPage.draftRequested("d1")
            compare(b.core.called("startCompose")[0].args, ["draft", 0, "", "d1"])
            compare(b.shell.paneCompose, 1)
            b.core.composeReady(1, composeDraft, "", "Editing a copy.")
            const page = find(b.shell, "composePage")
            compare(page.compose, 1)
            compare(find(page, "composeNote").text, "Editing a copy.")
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
            tryCompare(b.shell, "depth", 1)
            compare(b.shell.paneCompose, 4)
            const page = b.shell.composeItem
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

        function test_narrowing_carries_the_open_message_to_the_stack() {
            const b = bootWide(100)
            b.shell.currentPage.messageRequested(7)
            compare(b.core.openMessage, 7)
            const reader = b.shell.readerItem
            b.shell.width = thm.gu(80)
            compare(b.shell.wide, false)
            compare(b.shell.depth, 2)
            verify(b.shell.currentPage === reader, "the reader is not the top page")
            compare(reader.messageId, 7)
            compare(reader.wide, false)
            compare(b.core.openMessage, 7)
            b.shell.destroy()
        }

        function test_ctrl_n_opens_a_new_message_in_the_pane() {
            const b = bootWide(100)
            verify(b.shell.shortcuts.newKey.enabled)
            verify(!b.shell.newShortcut.enabled)
            b.shell.shortcuts.newKey.activated()
            compare(b.core.called("startCompose")[0].args, ["new", 0, "ann", ""])
            compare(b.shell.paneCompose, 1)
            // With compose open the key does nothing more.
            b.shell.shortcuts.newKey.activated()
            compare(b.core.called("startCompose").length, 1)
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

        // ---- T4 (#192): the reader and the compose are reparented, not recreated ----

        function pane(b) { return find(b.shell, "pane") }
        function longBody() {
            let lines = []
            for (let i = 0; i < 90; i++) lines.push("line " + i)
            return lines.join("\n")
        }

        function test_compose_survives_narrow_wide_narrow() {
            const b = wideCompose()
            const item = b.shell.composeItem
            verify(item !== null, "no compose item")
            find(item, "bodyField").text = "Typed body"
            b.core.reset()
            b.shell.width = thm.gu(70)
            b.shell.width = thm.gu(100)
            b.shell.width = thm.gu(70)
            verify(b.shell.composeItem === item, "the compose item was replaced")
            compare(find(item, "bodyField").text, "Typed body")
            compare(item.compose, 4)
            compare(b.core.called("closeCompose").length, 0)
            b.shell.destroy()
        }

        function test_narrowing_makes_compose_the_top_page() {
            const b = wideCompose()
            b.shell.width = thm.gu(70)
            verify(b.shell.currentPage === b.shell.composeItem, "the compose is not the top page")
            compare(b.shell.depth, 2)
            compare(b.shell.paneCompose, 0)
            b.shell.destroy()
        }

        function test_widening_with_compose_pops_to_list() {
            const b = wideCompose()
            b.shell.width = thm.gu(70)
            b.shell.width = thm.gu(100)
            tryCompare(b.shell, "depth", 1)
            compare(b.shell.paneCompose, 4)
            verify(b.shell.composeItem.parent === pane(b), "the compose is not in the pane")
            verify(b.shell.composeItem.visible)
            compare(b.shell.composeItem.opacity, 1)
            b.shell.destroy()
        }

        function test_narrow_compose_widens_into_the_pane() {
            // Opened narrow from the list, then the window grows.
            const b = bootWide(70)
            b.shell.composeOpened(6)
            b.core.composeReady(6, composeDraft, "", "")
            const item = b.shell.composeItem
            compare(b.shell.depth, 2)
            b.shell.width = thm.gu(100)
            tryCompare(b.shell, "depth", 1)
            compare(b.shell.paneCompose, 6)
            verify(b.shell.composeItem === item)
            compare(find(item, "subjectField").text, "Plans")
            b.shell.destroy()
        }

        function openedReader(b, id) {
            b.shell.currentPage.messageRequested(id)
            const reader = b.shell.readerItem
            verify(reader !== null, "no reader item")
            b.core.messageOpened({ id: id, html: "<p>" + new Array(500).join("Hello again. ") + "</p>", blockedImages: 0, from: "Lucia",
                fromAddress: "lucia@example.org", to: ["me@example.org"], cc: [], subject: "Plans " + id, date: 1700000000,
                account: "ann", senderKnown: false, senderVerified: true, attachments: [] })
            tryVerify(function() { return reader.ready })
            return reader
        }
        function fractionOf(reader) {
            const flick = find(reader, "readerFlick")
            return flick.contentY / Math.max(1, flick.contentHeight - flick.height)
        }

        function test_reader_scroll_fraction_survives() {
            const b = bootWide(100)
            const reader = openedReader(b, 7)
            const flick = find(reader, "readerFlick")
            waitForRendering(reader)
            flick.contentY = 0.5 * (flick.contentHeight - flick.height)
            tryVerify(function() { return Math.abs(reader.scrollFraction - 0.5) < 0.02 })
            b.shell.width = thm.gu(70)
            verify(b.shell.readerItem === reader, "the reader item was replaced")
            verify(b.shell.currentPage === reader)
            tryVerify(function() { return Math.abs(fractionOf(reader) - 0.5) < 0.02 }, 2000, "fraction " + fractionOf(reader) + " narrow")
            b.shell.width = thm.gu(100)
            tryCompare(b.shell, "depth", 1)
            verify(b.shell.readerItem === reader)
            tryVerify(function() { return Math.abs(fractionOf(reader) - 0.5) < 0.02 }, 2000, "fraction " + fractionOf(reader) + " wide")
            compare(reader.messageId, 7)
            b.shell.destroy()
        }

        function test_reader_back_on_the_narrowed_page_clears_the_open_row() {
            const b = bootWide(100)
            const reader = openedReader(b, 7)
            b.shell.width = thm.gu(70)
            reader.backRequested()
            tryCompare(b.shell, "depth", 1)
            compare(b.core.openMessage, 0)
            compare(b.core.messages.get(0).open, false)
            b.shell.destroy()
        }

        function test_narrow_reader_widens_into_the_pane() {
            const b = bootWide(70)
            b.shell.currentPage.messageRequested(7)
            const reader = b.shell.currentPage
            compare(b.shell.depth, 2)
            b.shell.width = thm.gu(100)
            tryCompare(b.shell, "depth", 1)
            verify(b.shell.readerItem === reader)
            verify(reader.parent === pane(b))
            compare(reader.wide, true)
            compare(b.core.openMessage, 7)
            b.shell.destroy()
        }

        function test_click_row_with_compose_closes_it_and_opens_message() {
            const b = wideCompose()
            // No row was open before the click.
            compare(b.core.messages.get(0).open, false)
            compare(b.core.messages.get(1).open, false)
            b.shell.openMessageFromRow(8)
            compare(b.core.calls.map(function(c) { return c.name }).filter(function(n) { return n === "closeCompose" || n === "setOpenMessage" }),
                    ["closeCompose", "setOpenMessage"])
            compare(b.core.called("closeCompose")[0].args, [4, false])
            compare(b.shell.paneCompose, 0)
            compare(b.shell.composeItem, null)
            compare(b.core.openMessage, 8)
            compare(b.core.messages.get(1).open, true)
            const toast = findByName(b.shell, "sentToast")
            verify(toast !== null)
            compare(toast.text, qsTr("Draft saved"))
            compare(toast.parent.mapFromItem(toast, toast.width / 2, 0).x, toast.parent.width / 2, "centred")
            verify(toast.width < toast.parent.width / 2, "a pill, not a bar")
            b.shell.destroy()
        }

        function collect(item, name, out) {
            if (item.objectName === name) out.push(item)
            const list = item.data !== undefined ? item.data : item.children
            for (let i = 0; list && i < list.length; i++) collect(list[i], name, out)
            if (item.contentItem && item.contentItem !== item) collect(item.contentItem, name, out)
            return out
        }

        function test_wide_inbox_toasts_centre_on_the_whole_window() {
            const b = bootWide(100)
            for (const name of ["undoToast", "noticeToast"]) {
                const toast = findByName(b.shell, name)
                toast.show("Archived", "Undo")
                wait(50)
                verify(toast.parent === b.shell, name + " sits on the window")
                fuzzyCompare(toast.x + toast.width / 2, b.shell.width / 2, 1, name + " is centred on the window")
                fuzzyCompare(toast.y + toast.height + thm.frameInset + thm.toastBottomMargin, b.shell.height, 1, name + " is 28 px up")
                toast.hide()
            }
            b.shell.destroy()
        }

        function test_wide_reader_toast_centres_on_the_whole_window() {
            const b = bootWide(100)
            b.shell.openMessageFromRow(8)
            verify(findByName(findByName(b.shell, "pane"), "readerFooter") !== undefined, "the pane holds a reader")
            // The inbox's and the reader's toast share a name: take the second.
            const all = collect(b.shell, "undoToast", [])
            compare(all.length, 2)
            const toast = all[1]
            toast.show("Archived", "Undo")
            wait(50)
            verify(toast.parent === b.shell)
            fuzzyCompare(toast.x + toast.width / 2, b.shell.width / 2, 1, "centred on the window")
            fuzzyCompare(toast.y + toast.height + thm.frameInset + thm.toastBottomMargin, b.shell.height, 1, "28 px up")
            toast.hide()
            b.shell.destroy()
        }

        function test_wide_compose_toast_centres_on_the_whole_window() {
            const b = wideCompose()
            const toast = findByName(b.shell, "composeToast")
            verify(toast !== undefined && toast !== null, "the compose has a toast")
            toast.show("Draft saved")
            wait(50)
            verify(toast.parent === b.shell, "sits on the window, not the pane")
            fuzzyCompare(toast.x + toast.width / 2, b.shell.width / 2, 1, "centred on the window")
            fuzzyCompare(toast.y + toast.height + thm.frameInset + thm.toastBottomMargin, b.shell.height, 1, "28 px up")
            toast.hide()
            b.shell.destroy()
        }

        function test_wide_sent_toast_does_not_cover_the_undo_toast() {
            const b = bootWide(100)
            const undo = findByName(b.shell, "undoToast")
            const sent = findByName(b.shell, "sentToast")
            let fired = 0
            undo.actionTriggered.connect(function() { fired++ })
            undo.show("Archived", "Undo")
            sent.show("Sent")
            wait(50)
            verify(sent.y + sent.height <= undo.y, "Sent sits above Undo, no overlap")
            const act = findByName(undo, "toastAction")
            const at = act.mapToItem(b.shell, act.width / 2, act.height / 2)
            mouseClick(b.shell, at.x, at.y)
            compare(fired, 1, "Undo is tappable")
            wait(50)
            fuzzyCompare(sent.y + sent.height + thm.frameInset + thm.toastBottomMargin, b.shell.height, 1, "Sent drops back when alone")
            sent.hide()
            b.shell.destroy()
        }

        function test_narrow_inbox_toast_stays_above_the_footer() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            const toast = findByName(b.shell, "undoToast")
            toast.show("Archived", "Undo")
            wait(50)
            verify(toast.parent !== b.shell, "on the page, not the window")
            fuzzyCompare(toast.y + toast.height + thm.toastBottomMargin, findByName(b.shell, "inboxFooter").y, 1, "above the footer")
            toast.hide()
            b.shell.destroy()
        }

        function test_no_row_open_while_compose_in_pane() {
            const b = bootWide(100)
            b.shell.currentPage.messageRequested(7)
            tryVerify(function() { return !!findByName(b.shell.currentPage, "openBar") })
            const bar = findByName(b.shell.currentPage, "openBar")
            verify(bar.visible, "the open row has no bar")
            b.shell.composeOpened(4)
            compare(b.shell.paneCompose, 4)
            verify(!bar.visible, "a row is marked open while compose is in the pane")
            // The reader is back, with its row, once the compose is gone.
            b.shell.paneCompose = 0
            verify(bar.visible, "the open row does not come back")
            b.shell.destroy()
        }

        function test_compose_close_shows_discard_sheet_in_pane() {
            const b = wideCompose()
            const page = b.shell.composeItem
            find(page, "backButton").clicked()
            const sheet = findByName(page, "discardSheet")
            verify(sheet !== null, "no sheet")
            tryVerify(function() { return sheet.opened })
            verify(findByName(page, "saveDraftRow") !== null, "no Save draft row")
            compare(b.core.called("closeCompose").length, 0)
            compare(b.shell.paneCompose, 4)
            b.shell.destroy()
        }

        function test_pane_close_send_discard_clear_pane_compose_data() {
            return [{ tag: "save draft" }, { tag: "discard" }, { tag: "send" }]
        }
        function test_pane_close_send_discard_clear_pane_compose(data) {
            const b = wideCompose()
            const page = b.shell.composeItem
            const depth = b.shell.depth
            if (data.tag === "save draft") findByName(page, "saveDraftRow").clicked()
            else if (data.tag === "discard") findByName(page, "discardActions").discarded()
            else b.core.queued(4, "Queued")
            tryCompare(b.shell, "paneCompose", 0)
            compare(b.shell.depth, depth)
            compare(page.done, false)
            tryVerify(function() { return find(b.shell, "emptyReader") !== null })
            if (data.tag === "save draft") compare(b.core.called("closeCompose")[0].args, [4, false])
            if (data.tag === "discard") compare(b.core.called("closeCompose")[0].args, [4, true])
            b.shell.destroy()
        }

        function test_stack_close_and_sent_unchanged() {
            const b = bootWide(70)
            b.shell.composeOpened(6)
            b.core.composeReady(6, composeDraft, "", "")
            compare(b.shell.depth, 2)
            b.shell.currentPage.leave()
            tryCompare(b.shell, "depth", 1)
            b.shell.composeOpened(7)
            b.core.composeReady(7, composeDraft, "", "")
            compare(b.shell.depth, 2)
            b.core.queued(7, "Queued")
            tryCompare(b.shell, "depth", 1)
            b.shell.composeOpened(8)
            b.core.composeReady(8, composeDraft, "", "")
            findByName(b.shell.currentPage, "discardActions").discarded()
            tryCompare(b.shell, "depth", 1)
            b.shell.destroy()
        }

        // #332: the cursor, the focused field and the scroll position go with the compose.
        function test_narrowing_keeps_body_cursor_focus_and_scroll() {
            const b = wideCompose()
            const page = b.shell.composeItem
            const body = find(page, "bodyField")
            body.text = longBody()
            const input = body.input
            input.forceActiveFocus()
            input.cursorPosition = 12
            const flick = find(page, "composeFlick")
            tryVerify(function() { return flick.contentHeight > flick.height + 300 })
            tryVerify(function() { return !page.resizing })
            flick.contentY = 250
            b.shell.width = thm.gu(70)
            verify(b.shell.composeItem === page)
            tryVerify(function() { return find(page, "bodyField").input.activeFocus }, 2000, "the body lost focus")
            compare(find(page, "bodyField").input.cursorPosition, 12)
            tryVerify(function() { return page.viewWanted === null && !page.resizing })
            fuzzyCompare(flick.contentY, 250, 2)
            b.shell.width = thm.gu(100)
            tryVerify(function() { return find(page, "bodyField").input.activeFocus }, 2000, "the body lost focus on widening")
            compare(find(page, "bodyField").input.cursorPosition, 12)
            tryVerify(function() { return page.viewWanted === null && !page.resizing })
            fuzzyCompare(flick.contentY, 250, 2)
            b.shell.destroy()
        }

        function test_narrowing_keeps_focus_in_the_subject() {
            const b = wideCompose()
            const page = b.shell.composeItem
            find(page, "subjectField").input.forceActiveFocus()
            find(page, "subjectField").input.cursorPosition = 3
            b.shell.width = thm.gu(70)
            tryVerify(function() { return find(page, "subjectField").input.activeFocus })
            compare(find(page, "subjectField").input.cursorPosition, 3)
            verify(!find(page, "bodyField").input.activeFocus)
            b.shell.destroy()
        }

        // Review 1281, 1: a message on its way out (Outbox) must not also be
        // saved as a draft by a row click.
        function test_row_click_while_sending_leaves_the_compose_alone() {
            const b = wideCompose()
            b.shell.composeItem.sending = true
            b.core.reset()
            b.shell.openMessageFromRow(8)
            compare(b.core.called("closeCompose").length, 0)
            compare(b.core.called("draftChanged").length, 0)
            compare(b.core.called("setOpenMessage").length, 0)
            compare(b.shell.paneCompose, 4)
            verify(b.shell.composeItem !== null)
            verify(findByName(b.shell, "sentToast").text !== qsTr("Draft saved"), "the toast says the draft is saved")
            b.shell.destroy()
        }

        // Review 1281, 2: with Settings on top the reader is not pushed over
        // it, and Back from Settings leaves the open message alone.
        function test_narrowing_with_a_page_on_top_does_not_push_the_reader() {
            const b = bootWide(100)
            b.shell.currentPage.messageRequested(7)
            const reader = b.shell.readerItem
            b.shell.currentPage.settingsRequested()
            tryVerify(function() { return b.shell.currentPage.objectName === "settingsPage" })
            const depth = b.shell.depth
            b.shell.width = thm.gu(70)
            compare(b.shell.wide, false)
            compare(b.shell.depth, depth)
            compare(b.shell.currentPage.objectName, "settingsPage")
            verify(reader.parent === pane(b), "the reader was moved onto the stack")
            b.shell.currentPage.backRequested()
            tryCompare(b.shell, "depth", 1)
            compare(b.core.openMessage, 7)
            compare(b.core.called("setOpenMessage").filter(function(c) { return c.args[0] === 0 }).length, 0)
            b.shell.destroy()
        }

        // Review 1281, 4: the stack is replaced (last account removed): the
        // reader and compose items go with it.
        function test_route_drops_the_reader_and_compose_items() {
            const b = bootWide(100)
            b.shell.currentPage.messageRequested(7)
            verify(b.shell.readerItem !== null)
            b.core.accounts.clear()
            tryVerify(function() { return b.shell.rootKind === "wizard" })
            compare(b.shell.readerItem, null)
            compare(b.shell.composeItem, null)
            b.shell.destroy()
        }
    }
}
