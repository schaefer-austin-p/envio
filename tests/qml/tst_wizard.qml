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
            find("confirmHosts").close()
            find("trustDialog").close()
            tryVerify(function() { return !find("confirmHosts").visible && !find("trustDialog").visible })
            core.reset()
            wizard.clearPasswords()
            wizard.oauthSession.reset()
            wizard.oauthProvider = ""
            wizard.accountName = ""
            opened = []
            wizard.oauthSession.uri.opener = function(u) { opened.push(String(u)); return true }
            wizard.step = "email"
            wizard.error = ""
            wizard.busy = false
            find("emailField").text = ""
            wizardClosed.clear()
        }
        function typeEmail() { find("emailField").text = "ann@example.org" }
        readonly property string serversLines:
            "Incoming (IMAP): imap.example.org:993, TLS\nOutgoing (SMTP): smtp.example.org:587, STARTTLS"
        function toSignIn(json, needsConfirm) {
            typeEmail()
            mouseClick(find("continueButton"))
            core.detected("ann@example.org", "Settings found", needsConfirm, json, serversLines, "")
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

        function eachTheme(body) {
            for (const name of ["carbon", "airmail"]) {
                thm.name = name
                body(name)
            }
            thm.name = "carbon"
        }
        function guessedJson() {
            return JSON.stringify({
                name: "Ann", email: "ann@example.org", username: "ann@login",
                imapHost: "imap.guess.example", imapPort: 143, imapSecurity: "starttls",
                smtpHost: "smtp.guess.example", smtpPort: 465, smtpSecurity: "tls",
                source: "guess", needsConfirm: false })
        }

        function test_sign_in_shows_the_servers_text() {
            eachTheme(function() {
                init()
                toSignIn(plainJson, false)
                const t = find("serversText")
                verify(t.visible)
                compare(t.text, serversLines)
                compare(t.textFormat, Text.PlainText)
                verify(find("editServers").visible)
            })
        }

        function test_edit_servers_opens_the_servers_step_prefilled() {
            eachTheme(function() {
                init()
                toSignIn(guessedJson(), false)
                mouseClick(find("editServers"))
                compare(wizard.step, "servers")
                compare(find("imapHost").text, "imap.guess.example")
                compare(find("imapPort").text, "143")
                compare(find("imapStarttls").checked, true)
                compare(find("smtpHost").text, "smtp.guess.example")
                compare(find("smtpPort").text, "465")
                compare(find("smtpStarttls").checked, false)
                compare(find("username").text, "ann@login")
            })
        }

        function test_manual_setup_starts_empty_after_edit_servers() {
            eachTheme(function() {
                init()
                toSignIn(guessedJson(), false)
                mouseClick(find("editServers"))
                back()
                compare(wizard.step, "email")
                typeEmail()
                mouseClick(find("manualButton"))
                compare(wizard.step, "servers")
                compare(find("imapHost").text, "")
                compare(find("smtpHost").text, "")
                compare(find("imapPort").text, "993")
                compare(find("smtpPort").text, "587")
                compare(find("imapStarttls").checked, false)
                compare(find("smtpStarttls").checked, true)
            })
        }

        function test_edit_servers_is_disabled_while_busy() {
            eachTheme(function() {
                init()
                toSignIn(plainJson, false)
                wizard.busy = true
                verify(!find("editServers").enabled)
                wizard.busy = false
                verify(find("editServers").enabled)
            })
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
            core.detected("ann@example.org", "Settings found", true, srvJson, "", "")
            mouseClick(dialogButton("confirmHosts", "secondaryAction"))
            tryVerify(function() { return !dlg.visible })
            tryCompare(wizard, "serversConfirmed", true)
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
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            core.trustPrompt("ann@example.org", "imap", "mail.example.net", "Can't verify the imap server",
                             "4F:2A:91", "4f2a91", "No authority signed it.")
            const dlg = find("trustDialog")
            tryVerify(function() { return dlg.opened })
            compare(dlg.title, "Can't verify the imap server")
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
            wait(50)  // the deferred signal would have arrived by now
            compare(core.called("acceptSetupCertificate").length, 0)
            compare(connectCalls().length, 1)
        }

        function test_trust_dialog_shows_the_core_title() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            core.trustPrompt("ann@example.org", "smtp", "mail.example.net",
                             "Can't verify the outgoing mail server (SMTP)",
                             "4F:2A:91", "4f2a91", "No authority signed it.")
            const dlg = find("trustDialog")
            tryVerify(function() { return dlg.opened })
            compare(dlg.title, "Can't verify the outgoing mail server (SMTP)")
        }

        function test_trust_accept_reconnects_with_hex() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            compare(connectCalls().length, 1)
            core.trustPrompt("ann@example.org", "smtp", "mail.example.net", "Can't verify the smtp server",
                             "4F:2A:91", "4f2a91", "No authority signed it.")
            mouseClick(dialogButton("trustDialog", "secondaryAction"))
            tryVerify(function() { return !find("trustDialog").visible })
            tryVerify(function() { return core.called("acceptSetupCertificate").length === 1 })
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

        // #381: a Microsoft address is refused after Continue, as a card.
        function refuse() {
            find("emailField").text = "ann@outlook.com"
            mouseClick(find("continueButton"))
            core.setupRefused("ann@outlook.com", "Microsoft accounts aren\u2019t supported yet",
                              "Envio can\u2019t connect to Outlook, Hotmail or Microsoft 365 mail for now. Try another address.")
        }
        function test_refusal_shows_a_notice_card() {
            refuse()
            compare(wizard.step, "email")
            const card = find("refusalCard")
            verify(card.visible)
            compare(card.kind, "info")
            compare(card.title, "Microsoft accounts aren\u2019t supported yet")
            compare(card.text, "Envio can\u2019t connect to Outlook, Hotmail or Microsoft 365 mail for now. Try another address.")
            verify(!wizard.busy)
        }
        function test_no_card_before_a_refusal() {
            typeEmail()
            verify(!find("refusalCard").visible)
            compare(find("continueButton").text, "Continue")
        }
        function test_refusal_changes_the_button_and_refocuses_the_field() {
            refuse()
            const button = find("continueButton")
            compare(button.text, "Change address")
            verify(button.enabled)
            find("emailField").input.focus = false
            core.reset()
            mouseClick(button)
            verify(find("emailField").input.activeFocus)
            compare(core.called("detect").length, 0)
        }
        function test_refusal_hides_manual_setup_and_the_ispdb_note() {
            typeEmail()
            verify(find("manualButton").visible)
            verify(find("ispdbNote").visible)
            refuse()
            verify(!find("manualButton").visible)
            verify(!find("ispdbNote").visible)
        }
        function test_editing_the_address_clears_the_refusal() {
            refuse()
            find("emailField").text = "ann@example.org"
            verify(!find("refusalCard").visible)
            compare(find("continueButton").text, "Continue")
            verify(find("manualButton").visible)
            verify(find("ispdbNote").visible)
        }
        function test_a_refusal_for_another_address_is_ignored() {
            typeEmail()
            mouseClick(find("continueButton"))
            core.setupRefused("someone@outlook.com", "T", "X")
            verify(!find("refusalCard").visible)
        }

        function test_password_cleared_after_added() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            core.accountAdded("ann", "ann@example.org")
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

        function back() {
            const all = everything(wizard, []).filter(function(o) { return o.text === "Back" && o.clicked !== undefined })
            verify(all.length > 0)
            all[0].clicked()
        }

        function test_password_cleared_when_closed() {
            // Back from the last step closes the wizard; both password
            // fields are empty.
            find("serversPassword").text = "left-over"
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            core.accountAdded("ann", "ann@example.org")
            compare(wizard.step, "options")
            find("passwordField").text = "typed-after"
            find("serversPassword").text = "typed-after"
            back()
            compare(find("passwordField").text, "")
            compare(find("serversPassword").text, "")
            compare(wizardClosed.count, 1)
        }

        function test_back_clears_the_password_and_the_confirmation() {
            toSignIn(srvJson, true)
            mouseClick(dialogButton("confirmHosts", "secondaryAction"))
            tryVerify(function() { return !find("confirmHosts").visible })
            find("passwordField").text = "hunter2"
            tryCompare(wizard, "serversConfirmed", true)
            back()
            compare(wizard.step, "email")
            compare(find("passwordField").text, "")
            compare(wizard.serversConfirmed, false)
            // Another address: step 2 comes up empty and unconfirmed.
            find("emailField").text = "bob@example.net"
            mouseClick(find("continueButton"))
            core.detected("bob@example.net", "Settings found", false, plainJson, "", "")
            compare(wizard.step, "signin")
            compare(find("passwordField").text, "")
            compare(wizard.serversConfirmed, false)
        }

        function test_first_run_has_no_back_on_the_email_step() {
            wizard.canLeave = false
            verify(!find("backButton").visible)
            // Between steps Back still works.
            toSignIn(plainJson, false)
            verify(find("backButton").visible)
            back()
            compare(wizard.step, "email")
            verify(!find("backButton").visible)
            wizard.canLeave = true
            verify(find("backButton").visible)
        }

        function test_editing_the_address_clears_the_password() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            wizard.step = "email"
            find("emailField").text = "ann@example.org2"
            compare(find("passwordField").text, "")
            compare(wizard.serversConfirmed, false)
        }

        function test_manual_setup_needs_an_address() {
            // #131: no address, no account.
            const manual = find("manualButton")
            compare(manual.enabled, false)
            find("emailField").text = "ann"
            compare(manual.enabled, false)
            find("emailField").text = "ada@"
            compare(manual.enabled, false)
            compare(find("continueButton").enabled, false)
            find("emailField").text = "ada@@"
            compare(manual.enabled, false)
            compare(find("continueButton").enabled, false)
            typeEmail()
            compare(manual.enabled, true)
            find("emailField").text = ""
            compare(manual.enabled, false)
            mouseClick(manual)
            compare(wizard.step, "email")
        }

        function test_servers_step_back_clears_its_password() {
            typeEmail()
            mouseClick(find("manualButton"))
            find("serversPassword").text = "hunter2"
            back()
            compare(wizard.step, "email")
            compare(find("serversPassword").text, "")
        }

        function test_late_events_after_back_are_ignored() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            compare(wizard.busy, true)
            back()
            compare(wizard.busy, false)
            core.trustPrompt("ann@example.org", "imap", "mail.example.net", "Can't verify the imap server", "4F", "4f", "Late.")
            core.setupProblem("ann@example.org", "Late problem.")
            core.accountAdded("ann", "ann@example.org")
            core.detected("ann@example.org", "Late", false, plainJson, "", "")
            wait(50)
            verify(!find("trustDialog").visible)
            compare(wizard.step, "email")
            compare(wizard.error, "")
            compare(core.called("acceptSetupCertificate").length, 0)
            compare(connectCalls().length, 1)
        }

        function test_a_late_account_added_for_another_address_does_not_move_the_wizard() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            back()
            find("emailField").text = "bob@example.org"
            mouseClick(find("continueButton"))
            core.detected("bob@example.org", "Settings found", false,
                plainJson.replace(/ann@example.org/g, "bob@example.org"), "", "")
            compare(wizard.step, "signin")
            find("passwordField").text = "swordfish"
            mouseClick(find("signInContinue"))
            compare(wizard.busy, true)
            core.accountAdded("ann", "ann@example.org")
            wait(50)
            compare(wizard.step, "signin")
            compare(wizard.busy, true)
            compare(find("passwordField").text, "swordfish")
            core.accountAdded("bob", "bob@example.org")
            compare(wizard.step, "options")
        }

        function test_a_trust_prompt_on_the_address_step_is_ignored() {
            typeEmail()
            mouseClick(find("continueButton"))
            compare(wizard.busy, true)
            core.trustPrompt("ann@example.org", "imap", "mail.example.net", "Can't verify the imap server", "4F", "4f", "Early.")
            wait(50)
            verify(!find("trustDialog").visible)
            compare(wizard.step, "email")
            compare(wizard.busy, true)
        }

        function test_answers_for_another_address_are_ignored() {
            toSignIn(plainJson, false)
            find("passwordField").text = "hunter2"
            mouseClick(find("signInContinue"))
            core.setupProblem("someone@else.example", "Not ours.")
            core.trustPrompt("someone@else.example", "imap", "x", "Can't verify the imap server", "4F", "4f", "Not ours.")
            wait(50)
            compare(wizard.error, "")
            verify(!find("trustDialog").visible)
            compare(wizard.busy, true)
        }

        function test_a_late_detected_does_not_leave_manual_setup() {
            typeEmail()
            mouseClick(find("continueButton"))
            mouseClick(find("manualButton"))
            compare(wizard.step, "servers")
            core.detected("ann@example.org", "Late", false, plainJson, "", "")
            compare(wizard.step, "servers")
        }

        function test_a_new_detected_resets_the_confirmation() {
            toSignIn(srvJson, true)
            mouseClick(dialogButton("confirmHosts", "secondaryAction"))
            tryVerify(function() { return !find("confirmHosts").visible })
            tryCompare(wizard, "serversConfirmed", true)
            // The same attempt is answered again, with hosts to confirm.
            core.detected("ann@example.org", "Settings found", true, srvJson, "", "")
            compare(wizard.serversConfirmed, false)
            find("passwordField").text = "hunter2"
            wizard.submit()
            compare(connectCalls().length, 1)
            compare(connectCalls()[0].args[2], false)
        }

        // #381: Connect refuses a Microsoft address typed into manual setup;
        // the wizard goes back to the email step, where the card is.
        function test_manual_setup_refusal_returns_to_the_email_step_with_the_card() {
            find("emailField").text = "ann@outlook.com"
            mouseClick(find("manualButton"))
            compare(wizard.step, "servers")
            find("imapHost").text = "outlook.office365.com"
            find("smtpHost").text = "smtp.office365.com"
            find("serversPassword").text = "hunter2"
            find("connectButton").clicked()
            verify(wizard.busy)
            core.setupRefused("ann@outlook.com", "Microsoft accounts aren\u2019t supported yet", "Try another address.")
            compare(wizard.step, "email")
            verify(!wizard.busy)
            verify(find("refusalCard").visible)
            compare(find("continueButton").text, "Change address")
            compare(find("serversPassword").text, "")
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
            // The user typed these hosts: their own choice, so confirmed.
            compare(connectCalls()[0].args[2], true)
            // A stale detected name does not leak into step 3.
            core.accountAdded("ann", "ann@example.org")
            compare(find("accountName").text, "ann@example.org")
        }

        function test_smtp_port_465_turns_starttls_off() {
            typeEmail()
            mouseClick(find("manualButton"))
            find("smtpPort").text = "587"
            verify(find("smtpStarttls").checked)
            find("smtpPort").text = "465"
            verify(!find("smtpStarttls").checked)
            find("smtpStarttls").checked = true // still the user's to change
            verify(find("smtpStarttls").checked)
            find("smtpPort").text = "587"
            verify(find("smtpStarttls").checked)
        }

        function test_manual_name_ignores_an_earlier_detect() {
            toSignIn(plainJson, false)
            back()
            mouseClick(find("manualButton"))
            find("imapHost").text = "imap.mine.example"
            find("smtpHost").text = "smtp.mine.example"
            find("serversPassword").text = "hunter2"
            find("connectButton").clicked()
            core.accountAdded("ann", "ann@example.org")
            compare(find("accountName").text, "ann@example.org")
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

        // #87: while a step is busy its footer shows a spinner and a status
        // line in place of the button, then the button comes back.
        function busyShows(buttonName, text) {
            const button = find(buttonName)
            const line = find("footerBusy", button.parent.parent)
            for (const look of ["carbon", "airmail"]) {
                for (const mode of ["light", "dark"]) {
                    const label = look + " " + mode
                    thm.name = look
                    thm.darkMode = mode
                    wizard.busy = false
                    verify(!line.visible, label)
                    verify(button.visible, label)
                    wizard.busy = true
                    verify(line.visible, label)
                    compare(line.text, text, label)
                    verify(!button.visible, label)
                    compare(line.height, button.parent.height, label)
                    wizard.busy = false
                    verify(!line.visible, label)
                    verify(button.visible, label)
                }
            }
            thm.name = "carbon"
            thm.darkMode = "system"
        }
        function test_email_step_shows_busy_exactly_while_busy() {
            busyShows("continueButton", "Looking up settings…")
        }
        function test_servers_step_shows_busy_exactly_while_busy() {
            typeEmail()
            mouseClick(find("manualButton"))
            compare(wizard.step, "servers")
            busyShows("connectButton", "Connecting…")
        }
        function test_signin_step_shows_busy_exactly_while_busy() {
            toSignIn(plainJson, false)
            busyShows("signInContinue", "Signing in…")
        }

        // Microsoft sign-in (#219 T8)
        property var opened: []
        readonly property string link: "https://login.example.org/authorize?x=1"
        function toMicrosoft() {
            typeEmail()
            mouseClick(find("continueButton"))
            core.detected("ann@example.org", "", false, plainJson, "", "Microsoft")
            compare(wizard.step, "signin")
        }
        function startMicrosoft() {
            toMicrosoft()
            mouseClick(find("oauthButton"))
            compare(wizard.step, "browser")
            core.oauthBrowser("ann@example.org", link)
        }

        function test_without_oauth_there_is_a_password_and_no_microsoft_button() {
            toSignIn(plainJson, false)
            verify(!find("oauthButton").visible)
            verify(find("signInContinue").visible)
            verify(find("passwordField").visible)
            verify(!find("browserStep").visible)
        }
        function test_with_oauth_there_is_a_button_and_no_password_field() {
            toMicrosoft()
            verify(find("oauthButton").visible)
            compare(find("oauthButton").text, "Sign in with Microsoft")
            verify(!find("passwordField").visible)
            verify(!find("signInContinue").visible)
            verify(!find("editServers").visible)
        }
        function test_the_button_starts_the_attempt_and_shows_the_browser_step() {
            toMicrosoft()
            mouseClick(find("oauthButton"))
            compare(core.called("startOAuth").length, 1)
            compare(core.called("startOAuth")[0].args[0], "ann@example.org")
            compare(core.called("startOAuth")[0].args[1], "")
            compare(wizard.step, "browser")
            verify(find("browserStep").visible)
            verify(find("cancelSignIn").visible)
        }
        function test_the_browser_is_opened_once_with_the_events_url() {
            startMicrosoft()
            compare(opened.length, 1)
            compare(opened[0], link)
            // The same event for another address is not ours.
            core.oauthBrowser("other@example.org", "https://login.example.org/other")
            compare(opened.length, 1)
        }
        function test_cancel_calls_cancelOAuth_and_the_card_waits_for_the_core() {
            startMicrosoft()
            mouseClick(find("cancelSignIn"))
            compare(core.called("cancelOAuth").length, 1)
            // Nothing is claimed until the core says the attempt ended.
            compare(wizard.oauthSession.noticeKind, "")
            compare(wizard.step, "browser")
            core.oauthDone("ann@example.org", "cancelled", "")
            compare(wizard.step, "signin")
            compare(find("oauthNotice").title, "Sign-in cancelled")
        }
        function test_an_account_added_before_the_cancel_arrived_is_not_called_cancelled() {
            startMicrosoft()
            mouseClick(find("cancelSignIn"))
            core.accountAdded("ann", "ann@example.org")
            core.oauthDone("ann@example.org", "added", "")
            compare(wizard.step, "options")
            compare(find("accountName").text, "Ann")
            compare(wizard.oauthSession.noticeKind, "")
        }
        function test_back_while_waiting_is_cancel() {
            startMicrosoft()
            wizard.back()
            compare(core.called("cancelOAuth").length, 1)
            core.oauthDone("ann@example.org", "cancelled", "")
            compare(wizard.step, "signin")
        }
        function test_a_refused_start_shows_the_cores_reason() {
            toMicrosoft()
            mouseClick(find("oauthButton"))
            compare(wizard.step, "browser")
            core.oauthDone("ann@example.org", "failed", "This account is already added")
            compare(wizard.step, "signin")
            const card = find("oauthNotice")
            verify(card.visible)
            compare(card.kind, "warning")
            compare(card.text, "This account is already added")
            verify(find("oauthButton").visible)
            verify(!wizard.busy)
        }
        function test_back_while_the_core_saves_the_account_keeps_waiting_for_it() {
            startMicrosoft()
            core.oauthConfirm("ann@example.org", "bob@example.org", "x")
            mouseClick(find("useAccount"))
            compare(wizard.oauthSession.state, "saving")
            wizard.back()
            compare(wizard.oauthSession.state, "saving")
            compare(core.called("cancelOAuth").length, 0)
            core.accountAdded("ann", "ann@example.org")
            compare(wizard.step, "options")
            // The identity-confirm path ends on the same name.
            compare(find("accountName").text, "Ann")
        }
        function test_each_failure_shows_its_card() {
            const texts = ["Microsoft took too long.", "No connection.", "Microsoft refused.", "Something went wrong."]
            for (let i = 0; i < texts.length; i++) {
                init()
                startMicrosoft()
                core.oauthDone("ann@example.org", "failed", texts[i])
                compare(wizard.step, "signin")
                const card = find("oauthNotice")
                verify(card.visible)
                compare(card.kind, "warning")
                compare(card.text, texts[i])
                verify(find("oauthButton").visible)
            }
        }
        function test_the_identity_question_has_use_and_back_buttons() {
            startMicrosoft()
            core.oauthConfirm("ann@example.org", "bob@example.org", "x")
            const card = find("oauthNotice")
            verify(card.visible)
            compare(card.title, "You signed in as bob@example.org")
            compare(card.text, "Use this account for ann@example.org?")
            verify(find("useAccount").visible)
            // As drawn, the button carries the external-link icon.
            compare(find("useAccount").iconName, "external")
            verify(!find("oauthButton").visible)
            mouseClick(find("useAccount"))
            compare(core.called("confirmOAuth").length, 1)
            compare(core.called("confirmOAuth")[0].args[0], true)
        }
        function test_back_on_the_identity_question_declines() {
            startMicrosoft()
            core.oauthConfirm("ann@example.org", "bob@example.org", "x")
            wizard.back()
            compare(core.called("confirmOAuth").length, 1)
            compare(core.called("confirmOAuth")[0].args[0], false)
            compare(core.called("cancelOAuth").length, 0)
            core.oauthDone("ann@example.org", "cancelled", "")
            compare(wizard.step, "signin")
        }
        function test_account_added_moves_on_and_done_added_only_clears_the_browser_step() {
            startMicrosoft()
            core.accountAdded("ann", "ann@example.org")
            compare(wizard.step, "options")
            core.oauthDone("ann@example.org", "added", "")
            compare(wizard.step, "options")
            compare(find("accountName").text, "Ann")
            compare(wizard.oauthSession.state, "idle")
            compare(wizard.oauthSession.noticeKind, "")
        }
    }
}
