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

        readonly property var ann: ({ key: "ann", name: "Ann", email: "ann@example.org", status: "", state: "ok", unread: 0, unreadText: "" })

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

        function test_new_message_pushes_compose() {
            const b = boot()
            tryVerify(function() { return b.shell.depth === 1 })
            b.shell.newMessage()
            compare(b.core.called("startCompose")[0].args, ["new", 0, "ann", ""])
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
    }
}
