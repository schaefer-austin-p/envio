import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages/accounts"

Item {
    id: root
    width: 400
    height: 700

    Theme { id: thm }
    MockCore { id: core }
    WizardPage {
        id: wizard
        theme: thm
        core: core
        anchors.fill: parent
    }
    SignalSpy { id: wizardClosed; target: wizard; signalName: "closed" }

    readonly property string srvJson: JSON.stringify({
        name: "Ann", email: "ann@example.org", username: "ann",
        imapHost: "imap.hosting.example", imapPort: 993, imapSecurity: "tls",
        smtpHost: "smtp.hosting.example", smtpPort: 587, smtpSecurity: "starttls",
        source: "srv", needsConfirm: true })
    readonly property string plainJson: JSON.stringify({
        name: "Ann", email: "ann@example.org", username: "ann",
        imapHost: "imap.example.org", imapPort: 993, imapSecurity: "tls",
        smtpHost: "smtp.example.org", smtpPort: 587, smtpSecurity: "starttls",
        source: "ispdb", needsConfirm: false })

    TestCase {
        name: "Wizard"
        when: windowShown

        function everything(item, out) {
            out.push(item)
            for (const prop of ["data", "children"]) {
                if (item[prop] === undefined) continue
                for (let i = 0; i < item[prop].length; i++) everything(item[prop][i], out)
                break
            }
            return out
        }
        function find(name, from) {
            const all = everything(from || wizard, [])
            const hit = all.filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function init() {
            core.reset()
            wizard.clearPasswords()
            wizard.step = "email"
            wizard.error = ""
            wizard.busy = false
            find("emailField").text = ""
            wizardClosed.clear()
        }
        function typeEmail() { find("emailField").text = "ann@example.org" }
        function toSignIn(json, needsConfirm) {
            typeEmail()
            mouseClick(find("continueButton"))
            core.detected("ann@example.org", "Settings found", needsConfirm, json)
            compare(wizard.step, "signin")
        }
        function dialogButton(dialogName, action) {
            const d = find(dialogName)
            tryVerify(function() { return d.opened })
            // The buttons start stacked: wait for the column to lay out.
            const p = find("primaryAction", d.contentItem)
            const q = find("secondaryAction", d.contentItem)
            tryVerify(function() { return q.mapToItem(null, 0, 0).y > p.mapToItem(null, 0, 0).y }, 2000)
            return find(action, d.contentItem)
        }
        function connectCalls() { return core.called("connect") }

        function test_ispdb_note_is_shown_before_detect() {
            const note = find("ispdbNote")
            verify(note.visible)
            compare(note.text, core.note)
            compare(core.called("detect").length, 0)
            typeEmail()
            const cont = find("continueButton")
            verify(cont.enabled)
            verify(cont.mapToItem(null, 0, 0).y > 0)
            mouseClick(cont)
            compare(core.called("detect").length, 1)
            compare(core.called("detect")[0].args[0], "ann@example.org")
            // The note was on the same screen as the button that sent it.
            compare(wizard.step, "email")
            verify(note.visible)
        }

        function test_srv_hosts_need_confirmation() {
            toSignIn(srvJson, true)
            const dlg = find("confirmHosts")
            tryVerify(function() { return dlg.opened })
            compare(find("imapLine", dlg.contentItem).text.indexOf("imap.hosting.example") >= 0, true)
            compare(find("smtpLine", dlg.contentItem).text.indexOf("smtp.hosting.example") >= 0, true)
            compare(wizard.serversConfirmed, false)

            // Cancel: back to step 1, nothing connected.
            mouseClick(dialogButton("confirmHosts", "primaryAction"))
            tryVerify(function() { return !dlg.visible })
            tryCompare(wizard, "step", "email")
            compare(wizard.serversConfirmed, false)
            compare(connectCalls().length, 0)

            // Detected again; this time confirm.
            mouseClick(find("continueButton"))
            core.detected("ann@example.org", "Settings found", true, srvJson)
            mouseClick(dialogButton("confirmHosts", "secondaryAction"))
            tryVerify(function() { return !dlg.visible })
            compare(wizard.serversConfirmed, true)
            compare(wizard.step, "signin")
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            compare(connectCalls().length, 1)
            compare(connectCalls()[0].args[0], srvJson)
            compare(connectCalls()[0].args[1], "hunter2")
            compare(connectCalls()[0].args[2], true)
        }

        function test_trust_dialog_primary_is_cancel() {
            toSignIn(plainJson, false)
            core.trustPrompt("ann@example.org", "imap", "mail.example.net",
                             "4F:2A:91", "4f2a91", "No authority signed it.")
            const dlg = find("trustDialog")
            tryVerify(function() { return dlg.opened })
            compare(dlg.title, "Can't verify mail.example.net")
            compare(dlg.primaryText, "Cancel")
            compare(dlg.secondaryText, "Trust this certificate")
            const primary = dialogButton("trustDialog", "primaryAction")
            const secondary = find("secondaryAction", dlg.contentItem)
            verify(primary.primary && !secondary.primary)
            const fp = find("fingerprint", dlg.contentItem)
            compare(fp.text, "4F:2A:91")
            compare(fp.font.family, thm.fontMono)
            mouseClick(primary)
            tryVerify(function() { return !dlg.visible })
            compare(core.called("acceptSetupCertificate").length, 0)
            compare(connectCalls().length, 0)
        }

        function test_trust_accept_reconnects_with_hex() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            compare(connectCalls().length, 1)
            core.trustPrompt("ann@example.org", "smtp", "mail.example.net",
                             "4F:2A:91", "4f2a91", "No authority signed it.")
            mouseClick(dialogButton("trustDialog", "secondaryAction"))
            tryVerify(function() { return !find("trustDialog").visible })
            compare(core.called("acceptSetupCertificate").length, 1)
            compare(core.called("acceptSetupCertificate")[0].args[0], "smtp")
            compare(core.called("acceptSetupCertificate")[0].args[1], "4f2a91")
            compare(connectCalls().length, 2)
            compare(connectCalls()[1].args[0], plainJson)
            compare(connectCalls()[1].args[1], "hunter2")
            // The order matters: accept first, then connect.
            const names = core.calls.map(function(c) { return c.name })
            verify(names.indexOf("acceptSetupCertificate") < names.lastIndexOf("connect"))
        }

        function test_setup_problem_keeps_the_password() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            core.setupProblem("ann@example.org", "The server refused the password.")
            compare(wizard.step, "signin")
            compare(find("passwordField").text, "hunter2")
            const err = find("stepError", find("passwordField").parent.parent)
            verify(err.visible)
            compare(err.text, "The server refused the password.")
            compare(wizard.setupJson, plainJson)
            verify(find("signInContinue").enabled)
        }

        function test_password_cleared_after_added() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            core.accountAdded("ann")
            compare(find("passwordField").text, "")
            compare(wizard.step, "options")
            compare(find("syncText").text, "While Envio is open")
            compare(find("keepText").text, "Last 30 days")
            // Default sender, then Done: saved, and the wizard closes.
            mouseClick(find("defaultToggle"))
            mouseClick(find("doneButton"))
            compare(core.called("saveSettings").length, 1)
            compare(core.called("saveSettings")[0].args[3], "ann")
            compare(wizardClosed.count, 1)
        }

        function test_password_cleared_when_closed() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            wizard.back()
            wizard.step = "signin"
            verify(find("passwordField").text !== "")
            wizard.close()
            compare(find("passwordField").text, "")
            compare(wizardClosed.count, 1)
        }

        function test_manual_setup_connects_with_typed_servers() {
            typeEmail()
            mouseClick(find("manualButton"))
            compare(wizard.step, "servers")
            compare(core.called("detect").length, 0)
            find("imapHost").text = "imap.mine.example"
            find("smtpHost").text = "smtp.mine.example"
            find("smtpPort").text = "465"
            find("smtpStarttls").checked = false
            find("serversPassword").text = "hunter2"
            verify(find("connectButton").enabled)
            // Below the fold of this window: activate it directly.
            find("connectButton").clicked()
            compare(connectCalls().length, 1)
            const setup = JSON.parse(connectCalls()[0].args[0])
            compare(setup.email, "ann@example.org")
            compare(setup.username, "ann@example.org")
            compare(setup.imapHost, "imap.mine.example")
            compare(setup.imapPort, 993)
            compare(setup.imapSecurity, "tls")
            compare(setup.smtpHost, "smtp.mine.example")
            compare(setup.smtpPort, 465)
            compare(setup.smtpSecurity, "tls")
            compare(setup.needsConfirm, false)
            compare(connectCalls()[0].args[1], "hunter2")
            compare(connectCalls()[0].args[2], false)
        }

        function test_manual_setup_needs_valid_ports() {
            typeEmail()
            mouseClick(find("manualButton"))
            find("imapHost").text = "imap.mine.example"
            find("smtpHost").text = "smtp.mine.example"
            find("serversPassword").text = "hunter2"
            verify(find("connectButton").enabled)
            find("imapPort").text = "99999"
            verify(!find("connectButton").enabled)
        }

        function test_app_password_label_for_provider() {
            const json = JSON.stringify({ name: "Ann", email: "ann@gmail.com", username: "ann@gmail.com",
                imapHost: "imap.gmail.com", imapPort: 993, imapSecurity: "tls",
                smtpHost: "smtp.gmail.com", smtpPort: 465, smtpSecurity: "tls",
                source: "provider", needsConfirm: false })
            toSignIn(json, false)
            verify(find("appPasswordHint").visible)
            compare(find("passwordField").label, "App password")
        }
    }
}
