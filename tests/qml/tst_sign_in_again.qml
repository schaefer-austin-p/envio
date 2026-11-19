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
    SignInAgainPage {
        id: page
        theme: thm
        core: core
        accounts: core.accounts
        account: "ann"
        anchors.fill: parent
    }
    SignalSpy { id: closed; target: page; signalName: "closed" }

    TestCase {
        name: "SignInAgain"
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
            const hit = everything(page, []).filter(function(o) { return o.objectName === name })
            verify(hit.length > 0, "no item named " + name)
            return hit[0]
        }
        function init() {
            core.reset()
            core.accounts.clear()
            core.accounts.append({ key: "ann", name: "Ann", display: "Ann", label: "", email: "ann@example.org",
                                   status: "Sign in again", state: "auth", oauth: true })
            page.session.reset()
            opened = []
            page.session.uri.opener = function(u) { opened.push(String(u)); return true }
            closed.clear()
            tryCompare(page, "accountEmail", "ann@example.org")
        }

        function test_shows_the_account_and_why() {
            compare(find("accountName").text, "Ann")
            compare(find("signInAgainNotice").title, "Microsoft needs you to sign in again")
            verify(find("signInAgainButton").visible)
            verify(!find("browserStep").visible)
        }
        function test_the_button_starts_with_the_account_key() {
            mouseClick(find("signInAgainButton"))
            compare(core.called("startOAuth").length, 1)
            compare(core.called("startOAuth")[0].args[0], "ann@example.org")
            compare(core.called("startOAuth")[0].args[1], "ann")
            verify(find("browserStep").visible)
            core.oauthBrowser("ann@example.org", "https://login.example.org/authorize?x=1")
            compare(opened.length, 1)
        }
        function test_resumed_closes_the_page() {
            mouseClick(find("signInAgainButton"))
            core.oauthDone("ann@example.org", "resumed", "")
            compare(closed.count, 1)
        }
        function test_cancel_stays_and_calls_cancelOAuth() {
            mouseClick(find("signInAgainButton"))
            mouseClick(find("cancelSignIn"))
            compare(core.called("cancelOAuth").length, 1)
            compare(closed.count, 0)
            compare(find("signInAgainNotice").title, "Sign-in cancelled")
        }
        function test_a_failure_replaces_the_expired_card() {
            mouseClick(find("signInAgainButton"))
            core.oauthDone("ann@example.org", "failed", "Microsoft refused.")
            compare(find("signInAgainNotice").text, "Microsoft refused.")
            verify(find("signInAgainButton").visible)
        }
    }
}
