import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/components"
import "../../qml/pages/accounts"
import "../../qml/pages/inbox"
import "../../qml/pages/reader"

// #67: under Airmail no label is set in the monospace font; the
// fingerprint alone keeps it.
Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm; name: "airmail" }

    Column {
        width: 360
        MessageRow {
            id: row
            theme: thm
            width: 360
            showAccount: true
            model: ({ "id": 1, "selected": false, "unread": true, "from": "Ana", "fromAddress": "ana@example.org", "flagged": false,
                      "hasAttachments": true, "dateBucket": "daymonth", "date": 1700000000,
                      "subject": "Hi", "preview": "Hello", "accountName": "Work" })
            index: 0
        }
        HeaderBlock {
            id: header
            theme: thm
            width: 360
            view: ({ "subject": "Invoice", "from": "Ana <a@x.org>", "to": ["me"], "cc": [], "date": 1700000000,
                     "account": "Work" })
        }
        AttachmentList {
            id: attachments
            theme: thm
            width: 360
            files: [{ "index": 0, "name": "a.pdf", "size": 84000 }]
        }
        Toast { id: toast; theme: thm; width: 360 }
        DateText { id: dateText; theme: thm; bucket: "daymonth"; date: 1700000000 }
    }
    TrustDialog { id: trust; theme: thm; host: "mail.example.net"; fingerprint: "AB:CD"; hex: "abcd" }
    ConfirmHostsDialog { id: hosts; theme: thm; imapHost: "imap.example.net"; imapPort: 993; smtpHost: "smtp.example.net"; smtpPort: 465 }

    TestCase {
        name: "Fonts"
        when: windowShown

        function everything(item, out) {
            out.push(item)
            for (const prop of ["data", "children", "contentItem"]) {
                const kids = item[prop]
                if (kids === undefined || kids === null) continue
                if (kids.length === undefined) { everything(kids, out); continue }
                for (let i = 0; i < kids.length; i++) everything(kids[i], out)
            }
        }

        function monoTexts(top) {
            const all = []
            everything(top, all)
            return all.filter(function(t) {
                return t.text !== undefined && t.font !== undefined && t.visible
                    && t.font.family === thm.fontMono
            })
        }

        function labelTexts(top) {
            const all = []
            everything(top, all)
            return all.filter(function(t) {
                return t.text !== undefined && t.font !== undefined && t.visible
                    && t.font.family === thm.fontLabel
            })
        }

        function test_no_label_uses_mono_under_airmail() {
            toast.show("Archived", "Undo")
            for (const top of [row, header, attachments, toast, dateText]) {
                const bad = monoTexts(top)
                compare(bad.length, 0, "mono Text in " + top + ": " + (bad.length ? bad[0].text : ""))
                compare(labelTexts(top).length > 0, true, "no fontLabel Text found in " + top)
            }
        }

        function test_confirm_hosts_keep_mono_under_airmail() {
            hosts.open()
            tryVerify(function() { return hosts.opened })
            const all = []
            everything(hosts.contentItem, all)
            for (const name of ["imapLine", "smtpLine"]) {
                const t = all.filter(function(o) { return o.objectName === name })
                verify(t.length >= 1)
                compare(t[0].font.family, thm.fontMono, name)
            }
            hosts.close()
        }

        function test_trust_fingerprint_keeps_mono() {
            trust.open()
            tryVerify(function() { return trust.opened })
            const all = []
            everything(trust.contentItem, all)
            let found = null
            for (const t of all) if (t.objectName === "fingerprint") found = t
            verify(found !== null)
            compare(found.font.family, thm.fontMono)
            trust.close()
        }
    }
}
