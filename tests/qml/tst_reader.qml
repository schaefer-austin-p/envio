import QtQuick 2.15
import QtTest 1.2
import QtQuick.Controls 2.15
import "../../qml/platform"
import "../../qml/platform/stampTint.js" as StampTint
import "../../qml/pages"
import "../../qml/pages/reader"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    Component { id: metricsComponent; TextMetrics {} }
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
    Component { id: coverComponent; Item {} }
    Component { id: spyComponent; SignalSpy { signalName: "contentHeightChanged" } }
    StackView { id: coverStack; width: 400; height: 700; visible: false }
    SignalSpy { id: composing; target: root.page; signalName: "composeOpened" }
    SignalSpy { id: heightSpy; signalName: "contentHeightChanged" }
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
        searchResults: shellCore.searchResults
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
            compare(root.page.bodyHtml, "<style>a{color:" + String(thm.link) + "}</style><p>Hello</p>")
            compare(find("bodyText").textFormat, TextEdit.RichText)
            verify(!shown("imagesBanner"))
            compare(find("headerSubject").text, "Invoice 0042")
        }

        function test_body_links_use_the_theme_link_colour_data() {
            return [
                { tag: "carbon light", airmail: false, dark: false, want: "#B8342C" },
                { tag: "carbon dark", airmail: false, dark: true, want: "#E36A60" },
                { tag: "airmail light", airmail: true, dark: false, want: "#1F3A5F" },
                { tag: "airmail dark", airmail: true, dark: true, want: "#A9C1E3" }
            ]
        }

        // #74: no board shows a body link, so Carbon uses accent and Airmail
        // its heading blue; Qt's default #0000FF must never show.
        function test_body_links_use_the_theme_link_colour(data) {
            open(view([], 0))
            thm.name = data.airmail ? "airmail" : "carbon"
            thm.darkMode = data.dark ? "dark" : "light"
            compare(String(thm.link).toUpperCase(), data.want)
            verify(root.page.bodyHtml.indexOf("<style>a{color:" + String(thm.link) + "}") === 0)
            thm.name = "carbon"
            thm.darkMode = "system"
        }

        function test_header_stamp_is_airmail_only() {
            open(view([], 0))
            compare(find("headerStamp").item, null)
            thm.name = "airmail"
            verify(find("headerStamp").item !== null)
            compare(find("headerSender").text, "Ana Ruiz")
            compare(find("headerSender").font.family, thm.fontLabel)
            thm.name = "carbon"
            compare(find("headerStamp").item, null)
        }

        // A sender with no display name is a bare address: mono, so rn/m stay distinct.
        function test_airmail_bare_address_sender_uses_the_address_font() {
            const v = view([], 0)
            v.from = "ceo@acrne-corp.com"
            open(v)
            thm.name = "airmail"
            compare(find("headerSender").font.family, thm.fontAddress)
            verify(thm.fontAddress !== thm.fontLabel)
            thm.name = "carbon"
        }

        function has(name) {
            return everything(root.page, []).some(function(o) { return o.objectName === name })
        }

        function test_airmail_header_stamp_row_sits_below_the_title() {
            open(view([], 0))
            thm.name = "airmail"
            waitForRendering(root.page)
            const title = find("headerSubject")
            const stampRow = find("headerStamp")
            const y = stampRow.mapToItem(title, 0, 0).y
            verify(y >= title.height, "the stamp row at " + y + " overlaps the title of height " + title.height)
            // The table starts below the stamp row, and its From row is gone.
            verify(!has("header-From"))
            verify(has("header-To"))
            compare(find("headerAddress").text, "ana@example.org")
            compare(find("headerAddress").font.family, thm.fontAddress)
            const table = find("header-To").parent.parent.parent
            verify(table.mapToItem(title, 0, 0).y >= y + thm.stampHeight)
            thm.name = "carbon"
        }

        // M1: a long (spoofed) address keeps its domain end visible.
        function test_long_airmail_address_keeps_its_domain() {
            const v = view([], 0)
            v.fromAddress = "paypal.com-account-security-verification-team@evil.ru"
            open(v)
            thm.name = "airmail"
            waitForRendering(root.page)
            const a = find("headerAddress")
            compare(a.text, v.fromAddress)
            const m = createTemporaryObject(metricsComponent, root,
                { font: a.font, elide: a.elide, elideWidth: 200, text: a.text })
            verify(m.elidedText !== a.text, "elided: " + m.elidedText)
            verify(m.elidedText.endsWith("@evil.ru"), "shows " + m.elidedText)
            thm.name = "carbon"
        }

        // M3: a sender whose address is empty (bidi-spoofed) gets the row's colour.
        function test_header_stamp_colour_matches_the_row_for_an_empty_address() {
            const names = ['"PayPal" <service@lapyap.evil>', "Ana Ruiz <ana@example.org>",
                           "Bob <bob@example.org>", "Cy <cy@x.org>", "Di <di@y.net>", "Ed <ed@z.io>"]
            for (let i = 0; i < names.length; i++) {
                const v = view([], 0)
                v.from = names[i]
                v.fromAddress = ""
                open(v)
                thm.name = "airmail"
                waitForRendering(root.page)
                const avatar = find("headerStamp").item.children[0]
                compare(avatar.blue, StampTint.isBlue(StampTint.addressOf(v.from)), names[i])
                thm.name = "carbon"
            }
        }

        function test_header_addresses_use_the_address_font_data() {
            return [{ tag: "carbon", airmail: false }, { tag: "airmail", airmail: true }]
        }
        function test_header_addresses_use_the_address_font(data) {
            thm.name = data.airmail ? "airmail" : "carbon"
            const v = view([], 0)
            v.to = ["me@example.org", "Bob <bob@example.org>"]
            v.cc = ["cc@example.org"]
            open(v)
            compare(find("header-To").font.family, thm.fontAddress)
            compare(find("header-Cc").font.family, thm.fontAddress)
            // A row without an address stays in the label font.
            compare(find("header-Account").font.family, thm.fontLabel)
        }

        function test_airmail_header_drops_only_the_from_row() {
            thm.name = "airmail"
            open(view([], 0))
            verify(!shown("header-From"))
            verify(shown("header-To") && shown("header-Date") && shown("header-Account"))
        }

        function test_carbon_header_is_unchanged() {
            open(view([], 0))
            thm.name = "carbon"
            waitForRendering(root.page)
            const title = find("headerSubject")
            compare(title.y, 0)
            const from = find("header-From")
            verify(from.visible)
            // The title, then gu(1.5), then the table: hairline, gu(1), rows.
            const table = from.parent.parent.parent
            compare(table.y, title.height + thm.gu(1.5))
            compare(from.parent.y, thm.gu(1))
            verify(!has("headerAddress"))
        }

        function test_body_takes_the_body_roles_data() {
            return [{ tag: "carbon", airmail: false }, { tag: "airmail", airmail: true }]
        }
        function test_body_takes_the_body_roles(data) {
            thm.name = data.airmail ? "airmail" : "carbon"
            open(view([], 0))
            const body = find("bodyText")
            compare(body.font.family, thm.fontBody)
            compare(body.font.pixelSize, thm.bodySize)
            thm.name = "carbon"
        }

        function test_link_parts_keep_mono_under_airmail() {
            thm.name = "airmail"
            open(view([], 0))
            const url = "https://bank.example@evil.example/login"
            mock.linkViews[url] = { "beforeHost": "https://", "userinfo": "bank.example", "host": "evil.example",
                                    "unicodeHost": "", "unicodeNote": "", "afterHost": "/login", "openable": true, "warning": "" }
            find("bodyText").linkActivated(url)
            tryVerify(function() { return find("linkSheet").opened })
            for (const name of ["linkBefore", "linkUserinfo", "linkHost", "linkAfter"])
                compare(find(name).font.family, thm.fontMono, name)
            thm.name = "carbon"
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
            // The address shows as the core split it (here whole).
            compare(find("linkAfter").text, url)
            mouseClick(find("linkOpen"))
            compare(root.openedUrls, [url])
        }

        function test_decoration_before_the_at_is_flagged_and_the_host_shown() {
            open(view([], 0))
            const url = "https://bank.example@evil.example/login"
            mock.linkViews[url] = { "beforeHost": "https://", "userinfo": "bank.example", "host": "evil.example",
                                    "unicodeHost": "", "unicodeNote": "", "afterHost": "/login", "openable": true,
                                    "warning": "Before the @ is decoration. This link opens evil.example." }
            find("bodyText").linkActivated(url)
            tryVerify(function() { return find("linkSheet").opened })
            compare(find("linkHost").text, "evil.example")
            compare(find("linkHost").font.bold, true)
            compare(find("linkUserinfo").text, "bank.example@")
            compare(find("linkUserinfo").visible, true)
            compare(find("linkWarning").visible, true)
            verify(find("linkWarning").text.indexOf("evil.example") >= 0)
            mouseClick(find("linkOpen"))
            // Display only: what opens is the address as it was.
            compare(root.openedUrls, [url])
        }

        function test_link_sheet_bolds_the_ascii_host() {
            open(view([], 0))
            const url = "https://p\u0430ypal.com/login"
            mock.linkViews[url] = { "beforeHost": "https://", "userinfo": "", "host": "xn--pypal-4ve.com",
                                    "unicodeHost": "p\u0430ypal.com",
                                    "unicodeNote": "Displays as p\u0430ypal.com, which may imitate another site.",
                                    "afterHost": "/login", "openable": true, "warning": "" }
            find("bodyText").linkActivated(url)
            tryVerify(function() { return find("linkSheet").opened })
            compare(find("linkHost").text, "xn--pypal-4ve.com")
            compare(find("linkHost").font.bold, true)
            compare(find("linkUnicode").visible, true)
            compare(find("linkUnicode").font.bold, false)
            compare(find("linkUnicode").text, "Displays as p\u0430ypal.com, which may imitate another site.")
        }

        function test_a_long_host_wraps_and_is_shown_in_full() {
            open(view([], 0))
            const host = "www.bank.example.com.secure-account-verify-now.evil.example"
            const user = "bank.example.com.secure-account-verify-now-please-sign-in"
            const url = "https://" + user + "@" + host + "/login"
            mock.linkViews[url] = { "beforeHost": "https://", "userinfo": user, "host": host,
                                    "unicodeHost": "", "unicodeNote": "", "afterHost": "/login", "openable": true,
                                    "warning": "Before the @ is decoration. This link opens " + host + "." }
            find("bodyText").linkActivated(url)
            tryVerify(function() { return find("linkSheet").opened })
            const row = find("linkAddress")
            for (const name of ["linkBefore", "linkUserinfo", "linkHost", "linkAfter"])
                verify(find(name).width <= row.width, name + " is wider than the sheet")
            verify(find("linkHost").lineCount > 1)
            verify(find("linkUserinfo").lineCount > 1)
        }

        function test_a_plain_link_shows_no_decoration_and_no_warning() {
            open(view([], 0))
            const url = "https://example.org/"
            mock.linkViews[url] = { "beforeHost": "https://", "userinfo": "", "host": "example.org",
                                    "unicodeHost": "", "unicodeNote": "", "afterHost": "/", "openable": true, "warning": "" }
            find("bodyText").linkActivated(url)
            tryVerify(function() { return find("linkSheet").opened })
            compare(find("linkUserinfo").visible, false)
            compare(find("linkWarning").visible, false)
        }

        function test_a_malformed_link_cannot_be_opened() {
            open(view([], 0))
            const url = "https://exa mple.org/"
            mock.linkViews[url] = { "beforeHost": "", "userinfo": "", "host": "", "unicodeHost": "", "unicodeNote": "",
                                    "afterHost": url, "openable": false,
                                    "warning": "This link looks malformed and can't be opened." }
            find("bodyText").linkActivated(url)
            tryVerify(function() { return find("linkSheet").opened })
            compare(find("linkOpen").enabled, false)
            compare(find("linkWarning").visible, true)
            mouseClick(find("linkOpen"))
            compare(root.openedUrls.length, 0)
        }

        function test_a_failed_save_is_not_exported_by_the_next_one() {
            open(view(pdf(), 0))
            mouseClick(find("attachmentRow-0"))
            tryVerify(function() { return find("attachmentSheet").opened })
            wait(100)
            mouseClick(find("attachment-share"))
            compare(mock.called("saveAttachment").length, 1)
            mock.attachmentFailed(5)
            mock.attachmentSaved("/tmp/other.pdf")
            compare(root.exported.length, 0)
        }

        function test_another_messages_failed_save_keeps_the_export() {
            open(view(pdf(), 0))
            mouseClick(find("attachmentRow-0"))
            tryVerify(function() { return find("attachmentSheet").opened })
            wait(100)
            mouseClick(find("attachment-share"))
            mock.attachmentFailed(9)
            mock.attachmentSaved("/tmp/mine.pdf")
            compare(root.exported.length, 1)
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

        function test_an_action_on_a_pruned_row_does_not_leave() {
            open(view([], 0))
            mock.reset()
            mock.actRuns = false
            mouseClick(find("act-archive"))
            mock.actRuns = true
            compare(backed.count, 0)
            // The page asks again; the core answers if the message is gone.
            compare(mock.called("open").length, 1)
            mock.messageUnavailable(5, "That message is gone.")
            waitForRendering(root.page)
            compare(find("unavailableText").text, "That message is gone.")
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

        function test_loading_shows_until_the_view_arrives() {
            waitForRendering(root.page)
            verify(shown("loadingText"))
            verify(!shown("bodyText"))
            open(view([], 0))
            verify(!shown("loadingText"))
            verify(shown("bodyText"))
        }

        // A rich-text relayout costs hundreds of ms on a big message, so the
        // body gets its height once (empty, then the text) while the view
        // arrives and the page settles.
        function test_body_lays_out_once() {
            const body = find("bodyText")
            heightSpy.target = body
            heightSpy.clear()
            open(view(pdf(), 0))
            wait(100)
            verify(heightSpy.count <= 2, "contentHeight changed " + heightSpy.count + " times")
            compare(body.text.length > 0, true)
            heightSpy.target = null
        }

        // The body waits for the push transition: while the status is not
        // Active, the page shows Loading and no body, even with the view in.
        function test_body_waits_for_the_push_transition() {
            coverStack.visible = true
            // The first page of a stack is not animated; the reader is the second.
            coverStack.push(coverComponent)
            coverStack.push(readerComponent, { messageId: 5, width: 400, height: 700 })
            const reader = coverStack.currentItem
            mock.messageOpened(view([], 0))
            verify(reader.StackView.status !== StackView.Active,
                   "no window to check: status is " + reader.StackView.status)
            compare(reader.ready, false)
            compare(reader.bodyHtml, "")
            tryCompare(reader.StackView, "status", StackView.Active)
            tryCompare(reader, "ready", true)
            verify(reader.bodyHtml.length > 0)
            coverStack.clear()
            coverStack.visible = false
        }

        // Reply and forward push the compose page over the reader: the body
        // stays as it was, and Back does not lay it out again.
        function test_a_page_pushed_over_the_reader_keeps_the_body() {
            coverStack.visible = true
            coverStack.push(readerComponent, { messageId: 5, width: 400, height: 700 })
            const reader = coverStack.currentItem
            mock.messageOpened(view([], 0))
            tryCompare(reader.StackView, "status", StackView.Active)
            const body = everything(reader, []).filter(function(o) { return o.objectName === "bodyText" })[0]
            tryVerify(function() { return body.visible && body.contentHeight > 0 })
            const html = reader.bodyHtml
            const height = body.contentHeight
            const spy = createTemporaryObject(spyComponent, root, { target: body })
            coverStack.push(coverComponent)
            tryCompare(reader.StackView, "status", StackView.Inactive)
            wait(50)
            compare(reader.bodyHtml, html)
            verify(reader.ready)
            compare(body.contentHeight, height)
            coverStack.pop()
            tryCompare(reader.StackView, "status", StackView.Active)
            wait(50)
            compare(reader.bodyHtml, html)
            compare(body.contentHeight, height)
            compare(spy.count, 0)
            coverStack.clear()
            coverStack.visible = false
        }

        // The reader keeps the board's body line height now that TextEdit
        // has no lineHeight of its own.
        function test_body_line_height_follows_the_theme() {
            open(view([], 0))
            thm.name = "airmail"
            compare(thm.bodyLineHeight, 1.6)
            verify(root.page.bodyHtml.indexOf("p,li,div{line-height:160%}") > 0, root.page.bodyHtml)
            thm.name = "carbon"
            compare(thm.bodyLineHeight, 1.0)
            compare(root.page.bodyHtml.indexOf("line-height"), -1)
        }

        function test_body_is_read_only_and_links_open_the_sheet() {
            open(view([], 0))
            const body = find("bodyText")
            compare(body.readOnly, true)
            compare(body.activeFocusOnPress, false)
            compare(body.selectByMouse, false)
            compare(body.cursorVisible, false)
            body.linkActivated("https://example.org/")
            tryVerify(function() { return find("linkSheet").opened })
            compare(root.openedUrls.length, 0)
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
                                        status: "Up to date", state: "ok", unread: 0, unreadText: "" })
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
