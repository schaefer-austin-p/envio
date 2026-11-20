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
    // A 1x1 PNG: the preload test needs an image that loads offline.
    readonly property string pixel: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
    // A 3x2 PNG, for a second image of another size.
    readonly property string wide: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAIAAAASFvFNAAAAEElEQVR4nGP4z8AAQQxwFgBB0gX7h/C5SAAAAABJRU5ErkJggg=="

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
    SignalSpy { id: bodySet; signalName: "textChanged" }
    TextEdit { id: pasteTarget; visible: false; textFormat: TextEdit.PlainText }

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
                     senderKnown: false, senderVerified: true, senderCheckable: false,
                     attachments: attachments }
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

        function findText(text, from) {
            const hit = everything(from, []).filter(function(o) { return o.text === text && o.visible })
            verify(hit.length > 0, "no item with text " + text)
            return hit[0]
        }
        function test_reader_offer_shows_undo_toast() {
            open(view([], 0))
            const toast = find("undoToast")
            verify(!toast.visible)
            mock.undoOffer(31, "Reply to Grace queued", "reader")
            verify(toast.visible)
            compare(toast.text, "Reply to Grace queued")
            // Another offer's close leaves it; its own close hides it.
            mock.undoClosed(30)
            verify(toast.visible)
            mock.undoClosed(31)
            verify(!toast.visible)
        }

        function test_undo_calls_core_undo() {
            open(view([], 0))
            mock.undoOffer(32, "Reply to Grace queued", "reader")
            mouseClick(findText("Undo", find("undoToast")))
            compare(mock.called("undo").pop().args, [32])
        }

        function test_list_offer_does_not_show_in_reader() {
            open(view([], 0))
            mock.undoOffer(33, "Archived “Plans”", "list")
            verify(!find("undoToast").visible)
        }

        function test_add_to_calendar_exports_the_saved_ics() {
            root.page.hubAvailable = true
            const v = view([], 0)
            v.invite = { method: "request", state: "open", stateText: "", answer: "", sending: false,
                         sendFailed: false, summary: "Quarterly planning", location: "", allDay: false,
                         floating: false, start: 1795168800, end: 1795172400, startDate: "", endDate: "",
                         zoneUnknown: false, repeats: false, organizer: "Grace Example", canAnswer: true, canAddToCalendar: true,
                         icsIndex: 1 }
            open(v)
            mouseClick(find("inviteCalendar"))
            const saved = mock.called("exportInvite").pop().args
            compare(saved[0], 5)
            verify(saved[1].length > 0)
            // Another message's export is not this tap's.
            mock.inviteExported(6, "/tmp/envio-test/attachments/other.ics")
            compare(root.exported.length, 0)
            mock.inviteExported(5, "/tmp/envio-test/attachments/invite.ics")
            compare(root.exported, [{ path: "/tmp/envio-test/attachments/invite.ics", mode: "calendar" }])
            // A later save is not a calendar one.
            mock.attachmentSaved("/tmp/envio-test/attachments/other.pdf")
            mock.inviteExported(5, "/tmp/envio-test/attachments/again.ics")
            compare(root.exported.length, 1)
        }

        // An attachment save still in flight keeps its file, and the
        // calendar keeps its own (review 733).
        function test_a_pending_attachment_save_and_the_calendar_do_not_swap_files() {
            root.page.hubAvailable = true
            const v = view(pdf(), 0)
            v.invite = { method: "request", state: "open", stateText: "", answer: "", sending: false,
                         sendFailed: false, summary: "Q", location: "", allDay: false, floating: false,
                         start: 1795168800, end: 0, startDate: "", endDate: "", zoneUnknown: false,
                         repeats: false, organizer: "Grace Example", canAnswer: true, canAddToCalendar: true, icsIndex: -1 }
            open(v)
            // The card pushes the attachment below the fold: scroll to it.
            const flick = everything(root.page, []).filter(function(o) {
                return o.contentY !== undefined && o.boundsBehavior !== undefined && o.objectName !== "sheetFlick" })[0]
            flick.contentY = Math.max(0, flick.contentHeight - flick.height)
            waitForRendering(root.page)
            mouseClick(find("attachmentRow-0"))
            tryVerify(function() { return find("attachmentSheet").opened })
            wait(100)
            mouseClick(find("attachment-share"))
            tryVerify(function() { return !find("attachmentSheet").opened })
            mouseClick(find("inviteCalendar"))
            mock.attachmentSaved("/tmp/envio-test/attachments/invoice.pdf")
            mock.inviteExported(5, "/tmp/envio-test/attachments/Q.ics")
            compare(root.exported, [{ path: "/tmp/envio-test/attachments/invoice.pdf", mode: "share" },
                                    { path: "/tmp/envio-test/attachments/Q.ics", mode: "calendar" }])
        }

        function test_a_failed_calendar_save_forgets_the_export() {
            root.page.hubAvailable = true
            const v = view([], 0)
            v.invite = { method: "request", state: "open", stateText: "", answer: "", sending: false,
                         sendFailed: false, summary: "Q", location: "", allDay: false, floating: false,
                         start: 1795168800, end: 0, startDate: "", endDate: "", zoneUnknown: false,
                         repeats: false, organizer: "Grace Example", canAnswer: true, canAddToCalendar: true, icsIndex: 0 }
            open(v)
            mouseClick(find("inviteCalendar"))
            mock.inviteExportFailed(5)
            mock.inviteExported(5, "/tmp/envio-test/attachments/x.ics")
            compare(root.exported.length, 0)
        }

        // No offer yet: no id (0 is a real one) can close a toast never shown.
        function test_no_undo_target_before_an_offer() {
            compare(root.page.undoId, -1)
        }

        function test_invite_card_shows_only_with_an_invite() {
            open(view([], 0))
            verify(!shown("inviteCard"))
            const v = view([], 0)
            v.invite = { method: "request", state: "open", stateText: "", answer: "", sending: false,
                         sendFailed: false, summary: "Quarterly planning", location: "", allDay: false,
                         floating: false, start: 1795168800, end: 1795172400, startDate: "", endDate: "",
                         zoneUnknown: false, repeats: false, organizer: "Grace Example", canAnswer: true, canAddToCalendar: true,
                         icsIndex: 0 }
            open(v)
            verify(shown("inviteCard"))
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

        function test_mail_with_own_colours_uses_the_paper_card_data() {
            return [
                { tag: "carbon dark", airmail: false, paper: "#FFFFFF", text: "#1E2A38", link: "#B8342C" },
                { tag: "airmail dark", airmail: true, paper: "#FFFFFF", text: "#22303F", link: "#1F3A5F" }
            ]
        }

        // #108: the mail's own dark text assumes a white page.
        function test_mail_with_own_colours_uses_the_paper_card(data) {
            thm.name = data.airmail ? "airmail" : "carbon"
            thm.darkMode = "dark"
            const v = view([], 0)
            v.ownColours = true
            open(v)
            compare(String(find("bodyCard").color).toUpperCase(), data.paper)
            compare(find("bodyCard").color, thm.paper)
            compare(String(find("bodyText").color).toUpperCase(), data.text)
            compare(find("bodyText").color, thm.paperText)
            verify(root.page.bodyHtml.indexOf("<style>a{color:" + String(thm.paperLink) + "}") === 0)
            compare(String(thm.paperLink).toUpperCase(), data.link)
            thm.name = "carbon"
            thm.darkMode = "system"
        }

        // #163: a rich TextEdit never draws a remote image that arrives
        // after it laid out; once the preload has it, the body is set again.
        function test_a_loaded_image_sets_the_body_again() {
            const v = view([], 0)
            v.html = '<p>Hi</p><img src="' + root.pixel + '" width="20" height="20">'
            v.remoteImages = [root.pixel]
            open(v)
            // Qt's re-serialised text drops the redraw's empty comment, so
            // count the sets instead of comparing the text.
            bodySet.target = find("bodyText")
            bodySet.clear()
            tryCompare(find("preloadedImage"), "status", Image.Ready)
            tryVerify(function() { return root.page.imageRedraws > 0 }, 2000)
            verify(bodySet.count > 0)
        }

        // #170: the core caps a too-wide image with no width once it knows
        // the natural size, so the page reports what it loaded, batched.
        function test_loaded_image_sizes_go_to_the_core_once() {
            const v = view([], 0)
            v.html = '<p>Hi</p><img src="' + root.pixel + '"><img src="' + root.wide + '">'
            v.remoteImages = [root.pixel, root.wide]
            open(v)
            tryVerify(function() { return mock.called("reportImageSizes").length > 0 }, 2000)
            const calls = mock.called("reportImageSizes")
            compare(calls.length, 1)
            compare(calls[0].args[0], 5)
            const sizes = JSON.parse(calls[0].args[1])
            compare(sizes[root.pixel], [1, 1])
            compare(sizes[root.wide], [3, 2])
            // The core shows the message again: the same images are not
            // reported a second time.
            open(v)
            wait(700)
            compare(mock.called("reportImageSizes").length, 1)
        }

        // The core takes at most 256 sizes from one report: more go in
        // several.
        function test_many_image_sizes_go_in_batches() {
            for (let i = 0; i < 300; i++) root.page.noteImageSize("https://e.test/" + i, 10, 10)
            root.page.reportSizes()
            const calls = mock.called("reportImageSizes")
            compare(calls.length, 2)
            compare(Object.keys(JSON.parse(calls[0].args[1])).length, 256)
            compare(Object.keys(JSON.parse(calls[1].args[1])).length, 44)
        }

        function test_a_view_without_images_preloads_nothing() {
            open(view([], 0))
            const hit = everything(root.page, []).filter(function(o) { return o.objectName === "preloadedImage" })
            compare(hit.length, 0)
            wait(400)
            compare(root.page.imageRedraws, 0)
            compare(mock.called("reportImageSizes").length, 0)
        }

        function test_plain_mail_keeps_the_theme_card_in_dark() {
            thm.darkMode = "dark"
            const v = view([], 0)
            v.ownColours = false
            open(v)
            compare(find("bodyCard").color, thm.surface)
            compare(find("bodyText").color, thm.text)
            verify(root.page.bodyHtml.indexOf("<style>a{color:" + String(thm.link) + "}") === 0)
            thm.darkMode = "system"
        }

        function test_light_mode_is_unchanged() {
            thm.darkMode = "light"
            const v = view([], 0)
            v.ownColours = true
            open(v)
            compare(find("bodyCard").color, thm.surface)
            compare(find("bodyText").color, thm.text)
            compare(thm.paperLink, thm.link)
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

        function test_header_account_row_shows_the_label_not_the_sender_name() {
            mock.accounts.clear()
            mock.accounts.append({ key: "ann", name: "Ann", display: "Work", label: "Work", email: "ann@example.org",
                                   status: "", state: "ok", unread: 0, unreadText: "" })
            open(view([], 0))
            tryCompare(find("header-Account"), "text", "Work")
            mock.accounts.clear()
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
            root.page.linkTapped(url)
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
            // This message loads its images now (this message only); later
            // mail from the sender goes through the core's check.
            const calls = mock.called("open")
            compare(calls.length, 1)
            compare(calls[0].args[0], 5)
            compare(calls[0].args[1], true)
        }

        function test_always_for_sender_shows_for_checkable_unverified_mail() {
            const v = view([], 1)
            v.senderVerified = false
            v.senderCheckable = true
            open(v)
            verify(shown("alwaysForSender"))
            mock.reset()
            mouseClick(find("alwaysForSender"))
            compare(mock.called("allowImagesFrom").length, 1)
            compare(mock.called("allowImagesFrom")[0].args, ["ana@example.org"])
            const calls = mock.called("open")
            compare(calls.length, 1)
            compare(calls[0].args[1], true)
        }

        function test_always_for_sender_is_hidden_for_unverified_mail() {
            const v = view([], 1)
            v.senderVerified = false
            v.senderCheckable = false
            open(v)
            verify(shown("imagesBanner"))
            verify(shown("loadImages"))
            verify(!shown("alwaysForSender"))
        }

        function test_link_opens_only_from_the_sheet() {
            open(view([], 0))
            const url = "https://evil.example/login?next=%2Fa"
            root.page.linkTapped(url)
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
            root.page.linkTapped(url)
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
            root.page.linkTapped(url)
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
            root.page.linkTapped(url)
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
            root.page.linkTapped(url)
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
            root.page.linkTapped(url)
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
            root.page.linkTapped("https://example.org/")
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

        // The wide layout: the pane is wider than the text measure.
        function wideReader() {
            root.page.destroy()
            root.page = readerComponent.createObject(root, { messageId: 5, wide: true, width: 1000, height: 700 })
            open(view([], 0))
        }
        function test_reader_measure_caps_at_80_gu() {
            wideReader()
            compare(find("bodyCard").width, thm.gu(80))
            compare(root.page.bodyWidth, thm.gu(80) - thm.gu(3))
            root.page.width = thm.gu(60)
            compare(find("bodyCard").width, thm.gu(60) - thm.gu(10))
        }
        function test_wide_reader_column_is_5_gu_from_the_pane_edge() {
            wideReader()
            compare(find("bodyCard").mapToItem(root.page, 0, 0).x, thm.gu(5))
            compare(find("reply").mapToItem(root.page, 0, 0).x, thm.gu(5))
        }
        function test_pane_reader_resets_for_the_next_message() {
            wideReader()
            const longHtml = "<p>" + new Array(400).join("Hello again. ") + "</p>"
            mock.messageOpened({ id: 5, html: longHtml, blockedImages: 0, from: "Ana Ruiz", fromAddress: "ana@example.org", to: ["me@example.org"],
                cc: [], subject: "Invoice", date: 1700000000, account: "ann", senderKnown: false, senderVerified: true, attachments: pdf() })
            waitForRendering(root.page)
            const flick = find("readerFlick")
            flick.contentY = 300
            root.page.pendingMode = "forward"
            root.page.calendarPending = true
            root.page.undoId = 9
            find("act-more").clicked()
            root.page.messageId = 6
            compare(flick.contentY, 0)
            compare(root.page.pendingMode, "")
            compare(root.page.calendarPending, false)
            compare(root.page.undoId, -1)
            verify(!find("readerMore").opened, "the more sheet stays open")
        }
        function test_scroll_fraction_follows_the_scroll_and_restores() {
            const longHtml = "<p>" + new Array(400).join("Hello again. ") + "</p>"
            mock.messageOpened({ id: 5, html: longHtml, blockedImages: 0, from: "Ana Ruiz", fromAddress: "ana@example.org", to: ["me@example.org"],
                cc: [], subject: "Invoice", date: 1700000000, account: "ann", senderKnown: false, senderVerified: true, attachments: [] })
            waitForRendering(root.page)
            const flick = find("readerFlick")
            compare(root.page.scrollFraction, 0)
            flick.contentY = 0.5 * (flick.contentHeight - flick.height)
            fuzzyCompare(root.page.scrollFraction, 0.5, 0.02)
            // The text re-lays out at another width; the fraction is what counts.
            root.page.holdScroll()
            root.page.width = 250
            waitForRendering(root.page)
            root.page.restoreScroll()
            tryVerify(function() { return Math.abs(flick.contentY / (flick.contentHeight - flick.height) - 0.5) < 0.02 })
            fuzzyCompare(root.page.scrollFraction, 0.5, 0.02)
        }
        function test_narrow_reader_keeps_its_full_width() {
            open(view([], 0))
            compare(find("bodyCard").width, 400 - thm.gu(4))
        }
        function test_reply_forward_are_20_gu_wide() {
            wideReader()
            compare(find("reply").width, thm.gu(20))
            compare(find("forward").width, thm.gu(20))
        }
        function test_wide_reader_has_close_in_place_of_back() {
            wideReader()
            compare(find("backButton").text, qsTr("Close"))
            compare(find("backButton").iconName, "close")
        }

        function checkRows() {
            mock.messages.clear()
            mock.messages.append({ id: 2, account: "ann", selected: true })
            mock.messages.append({ id: 3, account: "ann", selected: true })
        }
        function checked() {
            const out = []
            for (let i = 0; i < mock.messages.count; i++)
                if (mock.messages.get(i).selected) out.push(mock.messages.get(i).id)
            return out
        }
        function untouched() {
            return mock.called("select").length === 0 && mock.called("clearSelection").length === 0
                && mock.called("act").length === 0
        }

        function test_reader_archive_calls_act_on_with_its_message_id() {
            open(view([], 0))
            mock.reset()
            mouseClick(find("act-archive"))
            compare(mock.calls.map(function(c) { return c.name }), ["actOn"])
            compare(mock.called("actOn")[0].args, [[5], "archive", 0])
            compare(backed.count, 1)
        }

        function test_reader_action_keeps_checked_rows() {
            open(view([], 0))
            checkRows()
            mock.reset()
            mouseClick(find("act-archive"))
            compare(mock.called("actOn")[0].args, [[5], "archive", 0])
            verify(untouched())
            compare(checked(), [2, 3])
            mock.messages.clear()
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

        function test_reader_move_passes_folder_and_keeps_checked_rows() {
            open(view([], 0))
            checkRows()
            mock.folders.clear()
            mock.folders.append({ id: 8, account: "ann", name: "Trips 2026", role: "", unread: 0, depth: 0 })
            mock.reset()
            mouseClick(find("act-more"))
            tryVerify(function() { return find("readerMore").opened })
            wait(100) // the sheet lays its rows out on the next frame
            mouseClick(find("moreMove"))
            tryVerify(function() { return find("moveSheet").opened })
            wait(100)
            mouseClick(find("folder-8"))
            compare(mock.called("actOn")[0].args, [[5], "move", 8])
            verify(untouched())
            compare(checked(), [2, 3])
            mock.messages.clear()
            mock.folders.clear()
        }

        function test_reader_move_cancel_touches_nothing() {
            open(view([], 0))
            checkRows()
            mock.reset()
            mouseClick(find("act-more"))
            tryVerify(function() { return find("readerMore").opened })
            wait(100)
            mouseClick(find("moreMove"))
            tryVerify(function() { return find("moveSheet").opened })
            find("moveSheet").close()
            wait(100)
            verify(untouched())
            compare(mock.called("actOn").length, 0)
            compare(checked(), [2, 3])
            mock.messages.clear()
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

        // #173: sender-designed (paper) mail keeps its own spacing; only
        // plain-text mail gets the board's body line height.
        function test_paper_mail_gets_no_injected_line_height() {
            thm.name = "airmail"
            const v = view([], 0)
            v.ownColours = true
            open(v)
            compare(root.page.bodyHtml.indexOf("line-height"), -1)
            verify(root.page.bodyHtml.indexOf("<style>a{color:") === 0)
            thm.name = "carbon"
        }

        function test_plain_mail_keeps_the_airmail_line_height() {
            thm.name = "airmail"
            const v = view([], 0)
            v.ownColours = false
            open(v)
            verify(root.page.bodyHtml.indexOf("p,li,div{line-height:160%}") > 0, root.page.bodyHtml)
            thm.name = "carbon"
        }

        // #165: a long press selects a word and offers Copy; a drag or tap
        // never selects (selectByMouse stays off so a flick scrolls).
        function entry(menu, name) {
            for (let i = 0; i < menu.count; i++)
                if (menu.itemAt(i).objectName === name) return menu.itemAt(i)
            verify(false, "no menu entry " + name)
        }
        function test_long_press_selects_a_word_and_shows_handles() {
            const v = view([], 0)
            v.html = "<p>Order 48213 shipped</p>"
            open(v)
            const body = find("bodyText")
            const handles = find("bodyHandles")
            compare(handles.visible, false)
            const r = body.positionToRectangle(body.text.length > 0 ? 8 : 0)
            root.page.holdBody(r.x + 1, r.y + r.height / 2)
            compare(body.selectedText, "48213")
            compare(handles.visible, true)
            verify(find("bodyMenu").visible || find("bodyMenu").opened)
            compare(find("bodyMenu").copyOnly, true)
        }

        // #291: the menu must not cover either handle's touch target. Covers
        // the menu's geometry as opened by holdBody (real Menu and handles,
        // offscreen); not the Lomiri window and a finger on the device.
        function overlaps(a, b) {
            return a.x < b.x + b.width && b.x < a.x + a.width
                && a.y < b.y + b.height && b.y < a.y + a.height
        }
        function flickOf(item) {
            let f = item
            while (f && f.contentY === undefined) f = f.parent
            verify(f, "no Flickable above the body")
            return f
        }
        // Holds `word`, first scrolling so its line sits `atY` below the top
        // of the view (undefined: no scrolling). Returns the menu's and the
        // selection's rectangles, in the body's coordinates.
        function holdAndCheck(word, atY) {
            const body = find("bodyText")
            const r = body.positionToRectangle(body.getText(0, body.length).indexOf(word))
            if (atY !== undefined) {
                const flick = flickOf(body)
                const y = body.mapToItem(flick.contentItem, 0, r.y).y
                flick.contentY = Math.max(0, y - atY)
                verify(flick.contentY > 0, "not scrolled")
            }
            root.page.holdBody(r.x + 1, r.y + r.height / 2)
            compare(body.selectedText, word)
            const menu = find("bodyMenu")
            verify(menu.visible || menu.opened)
            // The menu's real open height, not the closed 12 px.
            tryVerify(function() { return menu.height >= thm.touch }, 2000, "menu height " + menu.height)
            tryCompare(menu, "height", menu.openHeight)
            const m = Qt.rect(menu.x, menu.y, menu.width, menu.height)
            const names = ["startHandle", "endHandle"]
            for (let i = 0; i < names.length; i++) {
                const h = find(names[i])
                const t = thm.touch
                const c = body.mapFromItem(h, h.width / 2, h.height / 2)
                const box = Qt.rect(c.x - t / 2, c.y - t / 2, t, t)
                verify(!overlaps(m, box), names[i] + " is covered: menu " + JSON.stringify(m)
                       + " handle " + JSON.stringify(box))
            }
            return { menu: m, line: body.positionToRectangle(body.selectionStart), body: body }
        }
        function paragraphs(n, marked) {
            let html = ""
            for (let i = 0; i < n; i++) html += "<p>Line " + i + (i === 12 ? " " + marked : "") + "</p>"
            return html
        }
        function test_the_menu_clears_the_handles_below_the_first_line() {
            const v = view([], 0)
            v.html = "<p>Intro line</p><p>Order 48213 shipped</p><p>More text here</p>"
            open(v)
            holdAndCheck("48213")
        }
        function test_the_menu_clears_the_handles_on_the_first_line() {
            const v = view([], 0)
            v.html = "<p>48213 shipped on time</p><p>More text here</p>"
            open(v)
            holdAndCheck("48213")
        }
        // With room above, the menu sits wholly above the first selected line.
        function test_the_menu_goes_above_the_selection_when_there_is_room() {
            const v = view([], 0)
            v.html = paragraphs(60, "48213")
            open(v)
            const got = holdAndCheck("48213", 300)
            verify(got.menu.y + got.menu.height <= got.line.y,
                   "menu " + JSON.stringify(got.menu) + " line " + JSON.stringify(got.line))
        }
        // A scrolled page, with the word near the top of the view: no room
        // above, so the menu is below the end handle; handles still clear.
        function test_the_menu_clears_the_handles_in_a_scrolled_page() {
            const v = view([], 0)
            v.html = paragraphs(60, "48213")
            open(v)
            const got = holdAndCheck("48213", 20)
            verify(flickOf(got.body).contentY > 0)
            verify(got.menu.y >= got.line.y + got.line.height, "menu not below: " + JSON.stringify(got.menu))
        }
        function test_the_menu_stays_inside_the_body_at_the_right_edge() {
            const v = view([], 0)
            v.html = "<p>Intro</p><p>Intro</p><p>Intro</p><p align=\"right\">end 48213</p>"
            open(v)
            const got = holdAndCheck("48213")
            verify(got.menu.x >= 0 && got.menu.x + got.menu.width <= got.body.width,
                   "menu " + JSON.stringify(got.menu) + " body width " + got.body.width)
        }

        function test_copy_puts_the_selection_on_the_clipboard_as_plain_text() {
            const v = view([], 0)
            v.html = "<p>Code <b>7391</b> inside</p>"
            open(v)
            const body = find("bodyText")
            body.select(0, body.length)
            const menu = find("bodyMenu")
            menu.open()
            entry(menu, "editCopy").triggered()
            pasteTarget.text = ""
            pasteTarget.paste()
            compare(pasteTarget.text, "Code 7391 inside")
        }

        function test_the_body_menu_offers_no_cut_or_paste() {
            open(view([], 0))
            const menu = find("bodyMenu")
            menu.open()
            compare(entry(menu, "editCut").visible, false)
            compare(entry(menu, "editPaste").visible, false)
            compare(entry(menu, "editCopy").visible, true)
            compare(entry(menu, "editSelectAll").visible, true)
        }

        function test_body_is_read_only_and_links_open_the_sheet() {
            open(view([], 0))
            const body = find("bodyText")
            compare(body.readOnly, true)
            compare(body.activeFocusOnPress, false)
            compare(body.selectByMouse, false)
            compare(body.cursorVisible, false)
            root.page.linkTapped("https://example.org/")
            tryVerify(function() { return find("linkSheet").opened })
            compare(root.openedUrls.length, 0)
        }

        // A point inside the body's first link, in the body's coordinates.
        function linkSpot(body) {
            for (let y = 1; y < body.height; y += 3)
                for (let x = 1; x < body.width; x += 4)
                    if (body.linkAt(x, y) !== "") return Qt.point(x, y)
            verify(false, "no link in the body")
        }
        function linkBody(href) {
            const v = view([], 0)
            v.html = "<p>Please <a href=\"" + href + "\">verify your email</a> now</p>"
            open(v)
            return find("bodyText")
        }

        // On the phone a link is tapped with a finger: the body's TapHandler
        // takes a touch release, so TextEdit.linkActivated never fires there.
        function test_a_touch_tap_on_a_link_opens_the_sheet() {
            const url = "https://accounts.example/verify?t=1"
            const body = linkBody(url)
            const p = linkSpot(body)
            touchEvent(body).press(0, body, p.x, p.y).commit()
            touchEvent(body).release(0, body, p.x, p.y).commit()
            tryVerify(function() { return find("linkSheet").opened })
            compare(find("linkSheet").address, url)
            compare(root.openedUrls.length, 0)
        }

        function test_a_mouse_click_on_a_link_opens_the_sheet_once() {
            const url = "https://accounts.example/verify?t=2"
            const body = linkBody(url)
            mock.reset()
            const p = linkSpot(body)
            mouseClick(body, p.x, p.y)
            tryVerify(function() { return find("linkSheet").opened })
            compare(mock.called("linkView").length, 1)
        }

        function test_a_tap_beside_a_link_opens_no_sheet() {
            const body = linkBody("https://accounts.example/")
            mouseClick(body, body.width - 2, body.height - 2)
            wait(50)
            compare(find("linkSheet").opened, false)
        }

        // Review 620: a long press on a link word must not also open the link.
        function test_a_long_press_on_a_link_does_not_open_the_sheet() {
            const body = linkBody("https://accounts.example/")
            const p = linkSpot(body)
            touchEvent(body).press(0, body, p.x, p.y).commit()
            wait(800)
            touchEvent(body).release(0, body, p.x, p.y).commit()
            wait(50)
            verify(body.selectedText !== "")
            compare(find("linkSheet").opened, false)
        }

        // Review 880: with a mouse, TapHandler reports `tapped` on the release
        // of a long press; that release must not open the link either.
        function test_a_mouse_long_press_on_a_link_does_not_open_the_sheet() {
            const body = linkBody("https://accounts.example/")
            const p = linkSpot(body)
            mousePress(body, p.x, p.y)
            wait(800)
            mouseRelease(body, p.x, p.y)
            wait(50)
            verify(body.selectedText !== "")
            compare(find("linkSheet").opened, false)
            // Once the Copy menu is closed, the next click is a normal tap again.
            root.page.dismissSelection()
            tryVerify(function() { return !find("bodyMenu").visible })
            mouseClick(body, p.x, p.y)
            tryVerify(function() { return find("linkSheet").opened })
        }

        // Review 296: dragging a handle hides the menu; when the drag ends it
        // comes back beside the longer selection, clear of both handles.
        function test_the_menu_reopens_clear_of_the_handles_after_a_handle_drag() {
            const v = view([], 0)
            v.html = "<p>Intro line</p><p>Order 48213 shipped on time today</p><p>More text here</p>"
            open(v)
            const body = find("bodyText")
            const menu = find("bodyMenu")
            holdAndCheck("48213")
            const before = body.selectedText
            const h = find("endHandle")
            mousePress(h, h.width / 2, h.height / 2)
            tryVerify(function() { return !menu.visible }, 2000, "menu still open during the drag")
            const to = h.mapToItem(body, h.width / 2, h.height / 2)
            const far = body.mapToItem(h, to.x + thm.gu(12), to.y)
            mouseMove(h, far.x, far.y)
            verify(body.selectedText.length > before.length, "selection did not grow")
            mouseRelease(h, far.x, far.y)
            tryVerify(function() { return menu.visible }, 2000, "menu did not reopen")
            tryCompare(menu, "height", menu.openHeight)
            const m = Qt.rect(menu.x, menu.y, menu.width, menu.height)
            const names = ["startHandle", "endHandle"]
            for (let i = 0; i < names.length; i++) {
                const hh = find(names[i])
                const c = body.mapFromItem(hh, hh.width / 2, hh.height / 2)
                const box = Qt.rect(c.x - thm.touch / 2, c.y - thm.touch / 2, thm.touch, thm.touch)
                verify(!overlaps(m, box), names[i] + " is covered: menu " + JSON.stringify(m)
                       + " handle " + JSON.stringify(box))
            }
        }

        // An edge gesture or overlay can steal the touch: the drag then ends
        // by cancel, and the menu must still come back. (The test cannot make
        // Qt cancel a real grab, so it emits the MouseArea's `canceled`.)
        function test_the_menu_reopens_when_a_handle_drag_is_canceled() {
            const v = view([], 0)
            v.html = "<p>Intro line</p><p>Order 48213 shipped on time today</p><p>More text here</p>"
            open(v)
            const menu = find("bodyMenu")
            holdAndCheck("48213")
            const h = find("endHandle")
            mousePress(h, h.width / 2, h.height / 2)
            tryVerify(function() { return !menu.visible }, 2000, "menu still open during the drag")
            h.children[0].canceled()
            tryVerify(function() { return menu.visible }, 2000, "menu did not reopen after cancel")
        }

        function test_a_tap_clears_the_selection_handles_and_menu() {
            const v = view([], 0)
            v.html = "<p>Order 48213 shipped</p>"
            open(v)
            const body = find("bodyText")
            root.page.holdBody(1, 1)
            verify(body.selectedText !== "")
            root.page.dismissSelection()
            compare(body.selectedText, "")
            compare(find("bodyHandles").visible, false)
            tryVerify(function() { return !find("bodyMenu").visible })
        }

        function test_a_text_reset_closes_the_menu() {
            const v = view([], 0)
            v.html = "<p>Order 48213 shipped</p>"
            open(v)
            root.page.holdBody(1, 1)
            verify(find("bodyMenu").visible)
            root.page.imageRedraws += 1
            tryVerify(function() { return !find("bodyMenu").visible })
        }

        function test_paragraphs_and_breaks_copy_as_lines() {
            const v = view([], 0)
            v.html = "<p>First line<br>second line</p><p>Next paragraph</p>"
            open(v)
            const body = find("bodyText")
            body.select(0, body.length)
            const menu = find("bodyMenu")
            menu.open()
            entry(menu, "editCopy").triggered()
            pasteTarget.text = ""
            pasteTarget.paste()
            const lines = pasteTarget.text.split(/\r?\n/).filter(l => l !== "")
            compare(lines, ["First line", "second line", "Next paragraph"])
        }

        function test_another_messages_view_is_ignored() {
            const v = view([], 0)
            v.id = 9
            open(v)
            verify(!shown("bodyText"))
        }

        function test_a_tap_in_the_shell_pushes_the_reader() {
            shell.visible = true
            shellCore.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org",
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
