import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages"
import "../../qml/pages/reader"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: mock }
    property var openedUrls: []
    property var exported: []
    property var page: null

    Component {
        id: readerComponent
        ReaderPage {
            theme: thm
            core: mock
            accounts: mock.accounts
            folders: mock.folders
            // Qt.openUrlExternally cannot be observed; the page takes the opener.
            openUrl: function(url) { root.openedUrls.push(url) }
            // So is the Content Hub, which only exists on the phone.
            exportFile: function(path, mode) { root.exported.push({ path: path, mode: mode }) }
        }
    }
    SignalSpy { id: composing; target: root.page; signalName: "composeOpened" }
    SignalSpy { id: backed; target: root.page; signalName: "backRequested" }

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
        uriSource: ""
        anchors.fill: parent
        visible: false
    }

    TestCase {
        name: "Reader"
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
            const hit = everything(root.page, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function shown(name) {
            const hit = everything(root.page, []).filter(function(o) { return o.objectName === name })
            return hit.length > 0 && hit[0].visible
        }
        function view(attachments, blocked) {
            return { id: 5, html: "<p>Hello</p>", blockedImages: blocked, from: "Ana Ruiz",
                     fromAddress: "ana@example.org", to: ["me@example.org"], cc: [],
                     subject: "Invoice 0042", date: 1700000000, account: "ann",
                     senderKnown: false, attachments: attachments }
        }
        function pdf() { return [{ index: 0, name: "invoice.pdf", mime: "application/pdf", size: 86016 }] }
        function open(v) {
            mock.messageOpened(v)
            waitForRendering(root.page)
        }
        function init() {
            mock.reset()
            root.openedUrls = []
            root.exported = []
            root.page = readerComponent.createObject(root, { messageId: 5, width: 400, height: 700 })
            composing.target = root.page
            backed.target = root.page
            composing.clear()
            backed.clear()
        }
        function cleanup() {
            root.page.destroy()
            root.page = null
        }

        // Text from a sender is plain text: markup in it, an <img> above
        // all, must not be rendered, or its image is fetched.
        function test_hostile_header_and_attachment_names_fetch_nothing() {
            const evil = 'Hi <img src="http://example.invalid/x.png">'
            netRequests.reset()
            const v = view([{ index: 0, name: evil, mime: "application/pdf", size: 10 }], 0)
            v.subject = evil
            v.from = evil
            v.to = [evil]
            v.cc = [evil]
            open(v)
            mouseClick(find("attachmentRow-0"))
            tryVerify(function() { return find("attachmentSheet").opened })
            wait(500)
            compare(netRequests.count(), 0)
            compare(find("headerSubject").text, evil)
        }

        function test_opens_the_message_on_show() {
            const calls = mock.called("open")
            compare(calls.length, 1)
            compare(calls[0].args[0], 5)
            compare(calls[0].args[1], false)
            verify(calls[0].args[2] > 0)
        }

        function test_shows_the_header_and_body() {
            open(view([], 0))
            compare(find("bodyText").text, "<p>Hello</p>")
            compare(find("bodyText").textFormat, Text.RichText)
            verify(!shown("imagesBanner"))
            compare(find("headerSubject").text, "Invoice 0042")
        }

        function test_images_banner_loads_images() {
            open(view([], 2))
            verify(shown("imagesBanner"))
            mock.reset()
            mouseClick(find("loadImages"))
            const calls = mock.called("open")
            compare(calls.length, 1)
            compare(calls[0].args[0], 5)
            compare(calls[0].args[1], true)
            verify(calls[0].args[2] > 0)
        }

        function test_always_for_sender_calls_allow() {
            open(view([], 1))
            mock.reset()
            mouseClick(find("alwaysForSender"))
            compare(mock.called("allowImagesFrom").length, 1)
            compare(mock.called("allowImagesFrom")[0].args, ["ana@example.org"])
            // The sender is known now, so a plain open shows the images.
            const calls = mock.called("open")
            compare(calls.length, 1)
            compare(calls[0].args[1], false)
        }

        function test_link_opens_only_from_the_sheet() {
            open(view([], 0))
            const url = "https://evil.example/login?next=%2Fa"
            find("bodyText").linkActivated(url)
            compare(root.openedUrls.length, 0)
            const sheet = find("linkSheet")
            tryVerify(function() { return sheet.opened })
            // The address shows exactly as written.
            compare(find("linkAddress").text, url)
            mouseClick(find("linkOpen"))
            compare(root.openedUrls, [url])
        }

        function test_direction_overrides_in_a_link_are_shown_not_applied() {
            open(view([], 0))
            const url = "https://good.example/\u202Eformat.exe"
            find("bodyText").linkActivated(url)
            tryVerify(function() { return find("linkSheet").opened })
            const shown = find("linkAddress").text
            compare(shown, "https://good.example/[U+202E]format.exe")
            verify(shown.indexOf("\u202E") < 0)
            mouseClick(find("linkOpen"))
            // Display only: what opens is the address as it was.
            compare(root.openedUrls, [url])
        }

        function test_a_failed_save_is_not_exported_by_the_next_one() {
            open(view(pdf(), 0))
            mouseClick(find("attachmentRow-0"))
            tryVerify(function() { return find("attachmentSheet").opened })
            wait(100)
            mouseClick(find("attachment-share"))
            compare(mock.called("saveAttachment").length, 1)
            mock.attachmentFailed()
            mock.attachmentSaved("/tmp/other.pdf")
            compare(root.exported.length, 0)
        }

        function test_an_unrelated_notice_does_not_drop_the_export() {
            open(view(pdf(), 0))
            mouseClick(find("attachmentRow-0"))
            tryVerify(function() { return find("attachmentSheet").opened })
            wait(100)
            mouseClick(find("attachment-share"))
            mock.notice("Sync finished")
            mock.attachmentSaved("/tmp/invoice.pdf")
            compare(root.exported, [{ path: "/tmp/invoice.pdf", mode: "share" }])
        }

        function test_link_sheet_cancel_opens_nothing() {
            open(view([], 0))
            find("bodyText").linkActivated("https://example.org/")
            tryVerify(function() { return find("linkSheet").opened })
            mouseClick(find("linkCancel"))
            compare(root.openedUrls.length, 0)
        }

        function test_forward_goes_straight_to_compose_attachments_and_all() {
            open(view(pdf(), 0))
            mock.reset()
            mouseClick(find("forward"))
            const calls = mock.called("startCompose")
            compare(calls.length, 1)
            compare(calls[0].args.slice(0, 2), ["forward", 5])
            compare(composing.count, 1)
            compare(composing.signalArguments[0].length, 1)
        }

        function test_reply_starts_a_reply() {
            open(view([], 0))
            mock.reset()
            mouseClick(find("reply"))
            compare(mock.called("startCompose")[0].args.slice(0, 2), ["reply", 5])
            compare(composing.count, 1)
        }

        function test_archive_acts_on_this_message() {
            open(view([], 0))
            mock.reset()
            mouseClick(find("act-archive"))
            compare(mock.calls.map(function(c) { return c.name }), ["select", "act"])
            compare(mock.called("select")[0].args, [5, true])
            compare(mock.called("act")[0].args, ["archive", 0])
            compare(backed.count, 1)
        }

        function test_move_cancel_leaves_no_selection() {
            open(view([], 0))
            mock.reset()
            mouseClick(find("act-more"))
            tryVerify(function() { return find("readerMore").opened })
            wait(100) // the sheet lays its rows out on the next frame
            mouseClick(find("moreMove"))
            tryVerify(function() { return find("moveSheet").opened })
            compare(mock.called("select")[0].args, [5, true])
            find("moveSheet").close()
            tryVerify(function() { return mock.called("clearSelection").length === 1 })
        }

        function test_add_sender_to_contacts() {
            open(view([], 0))
            mock.reset()
            mouseClick(find("act-more"))
            tryVerify(function() { return find("readerMore").opened })
            wait(100) // the sheet lays its rows out on the next frame
            mouseClick(find("moreContact"))
            compare(mock.called("addContact")[0].args, ["Ana Ruiz", "ana@example.org"])
        }

        function test_attachment_save_goes_to_the_cache_then_the_hub() {
            open(view(pdf(), 0))
            mouseClick(find("attachmentRow-0"))
            tryVerify(function() { return find("attachmentSheet").opened })
            wait(100) // as above
            mock.reset()
            mouseClick(find("attachment-share"))
            const saves = mock.called("saveAttachment")
            compare(saves.length, 1)
            compare(saves[0].args[0], 5)
            compare(saves[0].args[1], 0)
            verify(saves[0].args[2].length > 0)
            mock.attachmentSaved(saves[0].args[2] + "/invoice.pdf")
            compare(root.exported, [{ path: saves[0].args[2] + "/invoice.pdf", mode: "share" }])
        }

        function test_unavailable_offers_retry() {
            mock.messageUnavailable(5, "That message is gone.")
            waitForRendering(root.page)
            compare(find("unavailableText").text, "That message is gone.")
            mock.reset()
            mouseClick(find("retry"))
            compare(mock.called("open").length, 1)
            compare(mock.called("open")[0].args[0], 5)
        }

        function test_another_messages_view_is_ignored() {
            const v = view([], 0)
            v.id = 9
            open(v)
            verify(!shown("bodyText"))
        }

        function test_a_tap_in_the_shell_pushes_the_reader() {
            shell.visible = true
            shellCore.accounts.append({ key: "ann", name: "Ann", email: "ann@example.org",
                                        status: "Up to date", state: "ok", unread: 0 })
            shellCore.ready = true
            const inbox = {}
            tryVerify(function() {
                const hit = everything(shell, []).filter(function(o) { return o.objectName === "inboxPage" })
                if (hit.length === 0 || !hit[0].visible) return false
                inbox.page = hit[0]
                return true
            })
            shellCore.reset()
            inbox.page.messageRequested(7)
            tryVerify(function() {
                return everything(shell, []).filter(function(o) { return o.objectName === "readerPage" && o.visible }).length === 1
            })
            compare(shellCore.called("open").length, 1)
            compare(shellCore.called("open")[0].args[0], 7)
            shell.visible = false
        }
    }
}
