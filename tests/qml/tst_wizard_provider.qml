import QtQuick 2.15
import QtTest 1.2
import "../../qml/platform"
import "../../qml/pages/accounts"

// The wizard's provider step, its routes, the step counter, the mismatch
// question and the app-password page (T17, #221).
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

    readonly property string link: "https://login.example/authorize?x=1"

    TestCase {
        name: "WizardProvider"
        when: windowShown

        property var opened: []

        function everything(item, out) {
            out.push(item)
            for (const prop of ["data", "children"]) {
                if (item[prop] === undefined) continue
                for (let i = 0; i < item[prop].length; i++) everything(item[prop][i], out)
                break
            }
            return out
        }
        function find(name) {
            const hit = everything(wizard, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function init() {
            find("confirmHosts").close()
            find("trustDialog").close()
            core.reset()
            wizard.clearPasswords()
            wizard.oauthSession.reset()
            wizard.oauthProvider = ""
            wizard.oauthKind = ""
            wizard.provider = ""
            wizard.providerNoticeKind = ""
            wizard.accountName = ""
            opened = []
            wizard.oauthSession.uri.opener = function(u) { opened.push(String(u)); return true }
            find("appPasswordUri").opener = function(u) { opened.push(String(u)); return true }
            wizard.step = "provider"
            wizard.error = ""
            wizard.busy = false
            wizard.canLeave = true
            wizardClosed.clear()
        }
        function back() {
            find("backButton").clicked()
        }
        function pick(id) { mouseClick(find("provider_" + id)) }
        function counter() { return find("stepCounterText").text }
        function title() { return find("title").text }

        // The Microsoft path to the waiting page.
        function signInAs(id, address) {
            pick(id)
            find("oauthEmail").text = address
            mouseClick(find("oauthButton"))
            compare(wizard.step, "browser")
            core.oauthBrowser(address, link)
        }

        function test_the_provider_step_lists_six_choices_in_the_boards_order() {
            const ids = ["outlook", "m365", "icloud", "gmail", "yahoo", "other"]
            const names = ["Outlook.com", "Microsoft 365", "iCloud", "Gmail", "Yahoo", "Other"]
            let last = -1
            for (let i = 0; i < ids.length; i++) {
                const row = find("provider_" + ids[i])
                verify(row.visible)
                verify(row.mapToItem(null, 0, 0).y > last, ids[i] + " is out of order")
                last = row.mapToItem(null, 0, 0).y
                verify(row.chevron)
                const texts = everything(row, []).filter(function(o) { return o.text === names[i] })
                verify(texts.length > 0, names[i])
            }
            compare(wizard.step, "provider")
            compare(title(), "Add account")
            verify(find("providerCancel").visible)
        }

        function test_outlook_routes_to_the_microsoft_sign_in_with_the_graph_kind() {
            pick("outlook")
            compare(wizard.step, "signin")
            compare(title(), "Sign in · Outlook.com")
            verify(find("oauthButton").visible)
            verify(!find("oauthEmail").readOnly)
            find("oauthEmail").text = "ann@example.org"
            mouseClick(find("oauthButton"))
            const call = core.called("startOAuth")
            compare(call.length, 1)
            compare(call[0].args[0], "ann@example.org")
            compare(call[0].args[1], "")
            compare(call[0].args[2], "graph")
        }

        function test_microsoft_365_routes_to_the_sign_in_with_the_imap_kind() {
            pick("m365")
            compare(wizard.step, "signin")
            compare(title(), "Sign in · Microsoft 365")
            find("oauthEmail").text = "ann@example.org"
            mouseClick(find("oauthButton"))
            compare(core.called("startOAuth")[0].args[2], "imap")
        }

        function test_the_app_password_providers_route_to_the_app_password_page() {
            const titles = { "icloud": "Sign in · iCloud", "gmail": "Sign in · Gmail", "yahoo": "Sign in · Yahoo" }
            for (const id in titles) {
                init()
                pick(id)
                compare(wizard.step, "apppass", id)
                compare(title(), titles[id])
                verify(find("appPasswordStep").visible)
            }
        }

        function test_other_routes_to_the_address_step() {
            pick("other")
            compare(wizard.step, "email")
            verify(find("emailField").visible)
            verify(!find("appPasswordStep").visible)
        }

        function test_the_counter_has_no_total_on_the_provider_step() {
            compare(counter(), "Step 1")
        }

        function test_the_counter_is_of_three_on_a_fixed_path() {
            for (const id of ["outlook", "m365", "icloud", "gmail", "yahoo"]) {
                init()
                pick(id)
                compare(counter(), "Step 2 of 3", id)
            }
            wizard.step = "options"
            compare(counter(), "Step 3 of 3")
        }

        function test_the_counter_is_of_four_on_the_other_path() {
            pick("other")
            compare(counter(), "Step 2 of 4")
            find("emailField").text = "ann@example.org"
            mouseClick(find("continueButton"))
            core.detected("ann@example.org", "Settings found", false, "{}", "", "")
            compare(wizard.step, "signin")
            compare(counter(), "Step 3 of 4")
            wizard.step = "options"
            compare(counter(), "Step 4 of 4")
        }

        function test_back_from_the_second_step_returns_to_the_provider_step() {
            for (const id of ["outlook", "m365", "icloud", "gmail", "yahoo", "other"]) {
                init()
                pick(id)
                back()
                compare(wizard.step, "provider", id)
                compare(counter(), "Step 1")
                compare(wizardClosed.count, 0)
            }
        }

        function test_back_on_the_other_path_goes_through_the_address_step() {
            pick("other")
            find("emailField").text = "ann@example.org"
            mouseClick(find("continueButton"))
            core.detected("ann@example.org", "Settings found", false, "{}", "", "")
            back()
            compare(wizard.step, "email")
            back()
            compare(wizard.step, "provider")
        }

        function test_back_and_cancel_on_the_provider_step_close_the_wizard() {
            back()
            compare(wizardClosed.count, 1)
            mouseClick(find("providerCancel"))
            compare(wizardClosed.count, 2)
        }

        function test_first_run_has_no_back_on_the_provider_step() {
            wizard.canLeave = false
            verify(!find("backButton").visible)
            pick("icloud")
            verify(find("backButton").visible)
        }

        function test_picking_again_starts_clean() {
            pick("icloud")
            find("appPasswordEmail").text = "ann@example.org"
            find("appPasswordField").text = "abcd efgh"
            back()
            pick("gmail")
            compare(find("appPasswordEmail").text, "")
            compare(find("appPasswordField").text, "")
        }

        // The mismatch question.
        function test_a_mismatch_asks_for_one_more_sign_in() {
            signInAs("outlook", "ann@example.org")
            core.oauthMismatch("ann@example.org", "This is a work or school account",
                "ann@example.org signed in as a work or school account, not Outlook.com. One more sign-in sets it up the right way.")
            compare(wizard.oauthSession.state, "mismatch")
            compare(wizard.step, "signin")
            const card = find("oauthNotice")
            verify(card.visible)
            compare(card.kind, "warning")
            compare(card.title, "This is a work or school account")
            verify(card.text.indexOf("ann@example.org signed in as a work or school account") === 0)
            compare(find("oauthExplain").text, "Microsoft opens in the browser again. Sign in with the same address.")
            verify(find("mismatchContinue").visible)
            compare(find("mismatchContinue").text, "Sign in with Microsoft")
            verify(!find("oauthButton").visible)
            verify(!find("oauthEmail").visible)
            compare(counter(), "Step 2 of 3")
        }

        function test_the_second_sign_in_confirms_and_waits_for_the_browser_again() {
            signInAs("outlook", "ann@example.org")
            core.oauthMismatch("ann@example.org", "This is a work or school account", "text")
            mouseClick(find("mismatchContinue"))
            compare(core.called("confirmOAuth").length, 1)
            compare(core.called("confirmOAuth")[0].args[0], true)
            compare(wizard.step, "browser")
            compare(opened.length, 1)
            core.oauthBrowser("ann@example.org", link + "&second=1")
            compare(opened.length, 2)
            core.accountAdded("ann", "ann@example.org")
            compare(wizard.step, "options")
        }

        function test_back_on_the_mismatch_question_declines_and_stays_on_the_sign_in_step() {
            signInAs("outlook", "ann@example.org")
            core.oauthMismatch("ann@example.org", "This is a work or school account", "text")
            back()
            compare(core.called("confirmOAuth").length, 1)
            compare(core.called("confirmOAuth")[0].args[0], false)
            core.oauthDone("ann@example.org", "cancelled", "")
            compare(wizard.step, "signin")
            compare(wizard.oauthSession.state, "idle")
        }

        // Refusals end on the provider step.
        function test_a_decline_on_the_providers_page_returns_to_the_provider_step() {
            signInAs("outlook", "ann@example.org")
            core.oauthDone("ann@example.org", "cancelled", "")
            compare(wizard.step, "provider")
            const card = find("providerNotice")
            verify(card.visible)
            compare(card.kind, "info")
            compare(card.title, "Sign-in cancelled")
            compare(card.text, "Nothing was saved. Choose a provider to try again.")
            compare(counter(), "Step 1")
        }

        function test_an_admin_block_returns_to_the_provider_step_with_a_warning() {
            signInAs("m365", "ann@example.org")
            core.oauthDone("ann@example.org", "admin", "text")
            compare(wizard.step, "provider")
            const card = find("providerNotice")
            compare(card.kind, "warning")
            compare(card.title, "Your organization has not allowed Envio")
            compare(card.text, "Ask your administrator to allow it. Nothing was saved.")
        }

        function test_a_wrong_second_sign_in_returns_to_the_provider_step_with_the_cores_text() {
            signInAs("outlook", "ann@example.org")
            core.oauthDone("ann@example.org", "mismatch", "The second sign-in was for another account.")
            compare(wizard.step, "provider")
            compare(find("providerNotice").kind, "warning")
            compare(find("providerNotice").text, "The second sign-in was for another account.")
        }

        function test_cancel_in_envio_stays_on_the_sign_in_step() {
            signInAs("outlook", "ann@example.org")
            mouseClick(find("cancelSignIn"))
            compare(core.called("cancelOAuth").length, 1)
            core.oauthDone("ann@example.org", "cancelled", "")
            compare(wizard.step, "signin")
            verify(!find("providerNotice").visible)
        }

        function test_picking_clears_an_earlier_refusal() {
            signInAs("outlook", "ann@example.org")
            core.oauthDone("ann@example.org", "cancelled", "")
            pick("icloud")
            back()
            verify(!find("providerNotice").visible)
        }

        // The app-password page.
        function test_the_icloud_page_has_the_boards_text_and_steps() {
            pick("icloud")
            compare(find("appPasswordIntro").text,
                "iCloud needs an app password for Envio, not your Apple Account password. Turn on two-factor authentication first.")
            compare(find("appPasswordSteps").text,
                "1. Open your Apple Account settings.\n2. Under Sign-In and Security, choose App-Specific Passwords.\n3. Create one named \"Envio\" and copy it.")
            compare(find("appPasswordLink").text, "Open Apple Account settings")
            compare(find("appPasswordField").placeholder, "16 letters, dashes and spaces don't matter")
        }

        function test_the_gmail_and_yahoo_pages_differ_in_text_and_link() {
            pick("gmail")
            compare(find("appPasswordLink").text, "Open Google account settings")
            verify(find("appPasswordIntro").text.indexOf("2-Step Verification") > 0)
            back()
            pick("yahoo")
            compare(find("appPasswordLink").text, "Open Yahoo account security")
        }

        function test_the_link_button_opens_the_providers_page() {
            pick("icloud")
            mouseClick(find("appPasswordLink"))
            compare(opened.length, 1)
            verify(opened[0].indexOf("https://") === 0)
        }

        function test_continue_waits_for_an_address_and_a_password() {
            pick("icloud")
            verify(!find("appPasswordContinue").enabled)
            find("appPasswordEmail").text = "ann@example.org"
            verify(!find("appPasswordContinue").enabled)
            find("appPasswordField").text = "abcd efgh ijkl mnop"
            verify(find("appPasswordContinue").enabled)
        }

        function test_continue_connects_with_the_known_setup_and_no_host_confirmation() {
            pick("icloud")
            find("appPasswordEmail").text = " ann@example.org "
            find("appPasswordField").text = "abcd efgh ijkl mnop"
            mouseClick(find("appPasswordContinue"))
            compare(core.called("appPasswordSetup")[0].args[0], "icloud")
            compare(core.called("appPasswordSetup")[0].args[1], "ann@example.org")
            const call = core.called("connect")
            compare(call.length, 1)
            compare(JSON.parse(call[0].args[0]).email, "ann@example.org")
            compare(call[0].args[1], "abcd efgh ijkl mnop")
            compare(call[0].args[2], false)
            verify(wizard.busy)
            core.accountAdded("ann", "ann@example.org")
            compare(wizard.step, "options")
            compare(wizard.accountName, "ann")
            compare(find("appPasswordField").text, "")
        }

        function test_a_setup_problem_shows_on_the_page_and_keeps_the_password() {
            pick("gmail")
            find("appPasswordEmail").text = "ann@example.org"
            find("appPasswordField").text = "abcd efgh ijkl mnop"
            mouseClick(find("appPasswordContinue"))
            core.setupProblem("ann@example.org", "The server refused the password")
            compare(wizard.step, "apppass")
            verify(!wizard.busy)
            compare(find("appPasswordStep").error, "The server refused the password")
            compare(find("appPasswordField").text, "abcd efgh ijkl mnop")
        }

        function test_back_from_the_app_password_page_clears_the_password() {
            pick("yahoo")
            find("appPasswordField").text = "abcd"
            back()
            compare(find("appPasswordField").text, "")
        }
    }
}
