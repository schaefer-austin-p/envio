import QtQuick 2.15
import "../../bindle/Platform" as Platform

// One "Sign in with Microsoft" attempt as the pages see it, for the wizard
// and for "Sign in again". It holds no logic of the flow (the core does):
// it starts the attempt, opens the browser when the core says so, and turns
// the core's answers into a state and a notice for the page to draw.
//
// state: "idle" (nothing running; a notice may say how the last one ended),
//   "waiting" (the browser has the user), "confirm" (signed in as another
//   address: the page asks), "saving" (the user said yes, the core stores it).
// Only signals for `email` count; the core runs one attempt at a time.
Item {
    id: root
    visible: false
    required property var core
    // The address the attempt is for, and for "Sign in again" the account key.
    property string email: ""
    property string account: ""
    property string state: "idle"
    // Cancel was sent and the core has not answered yet: the state stays as it
    // was, and the core's answer decides (it may have finished first).
    property bool ending: false
    // The page's card: kind "" (none), "info" or "warning".
    property string noticeKind: ""
    property string noticeTitle: ""
    property string noticeText: ""
    // The link the core wants opened; "Open the browser again" opens it again.
    property string url: ""
    // For tests: where the browser is opened (`uri.opener` is replaced there).
    readonly property alias uri: uri
    // The core saved a new account (its AccountAdded came first), or the
    // account signed in again.
    signal added()
    signal resumed()

    Platform.Uri {
        id: uri
        objectName: "oauthUri"
        sourceFile: ""
    }

    function clearNotice() {
        noticeKind = ""
        noticeTitle = ""
        noticeText = ""
    }
    function cancelledNotice() {
        noticeKind = "info"
        noticeTitle = qsTr("Sign-in cancelled")
        noticeText = qsTr("Nothing was changed. No account was added.")
    }
    // Forget a finished or abandoned attempt (the page was left); tells no one.
    function reset() {
        ending = false
        state = "idle"
        url = ""
        clearNotice()
    }
    function start() {
        clearNotice()
        url = ""
        ending = false
        state = "waiting"
        core.startOAuth(email, account)
    }
    // Cancel, or Back while waiting or being asked.
    // The "cancelled" card waits for the core's OAuthDone, so it never says
    // "no account was added" about an account that was.
    function cancel() {
        if (state === "idle" || state === "saving" || ending) return
        ending = true
        if (state === "confirm") core.confirmOAuth(false)
        else core.cancelOAuth()
    }
    function answer(accept) {
        if (state !== "confirm" || ending) return
        core.confirmOAuth(accept)
        clearNotice()
        if (accept) {
            state = "saving"
        } else {
            state = "idle"
            cancelledNotice()
        }
    }
    function reopen() {
        if (url !== "") uri.open(url)
    }

    function mine(address) {
        return state !== "idle" && String(address).toLowerCase() === email.toLowerCase()
    }

    Connections {
        target: root.core
        function onOauthBrowser(address, link) {
            if (root.state !== "waiting" || !root.mine(address)) return
            root.url = link
            uri.open(link)
        }
        function onOauthConfirm(address, signedInAs, text) {
            if (!root.mine(address)) return
            root.state = "confirm"
            root.noticeKind = "warning"
            if (signedInAs !== "") {
                root.noticeTitle = qsTr("You signed in as %1").arg(signedInAs)
                root.noticeText = qsTr("Use this account for %1?").arg(root.email)
            } else {
                // No claim to name: the core's whole sentence.
                root.noticeTitle = ""
                root.noticeText = text
            }
        }
        function onOauthDone(address, outcome, text) {
            if (!root.mine(address)) return
            root.ending = false
            root.state = "idle"
            root.url = ""
            if (outcome === "added") {
                root.clearNotice()
                root.added()
            } else if (outcome === "resumed") {
                root.clearNotice()
                root.resumed()
            } else if (outcome === "cancelled") {
                root.cancelledNotice()
            } else {
                root.noticeKind = "warning"
                root.noticeTitle = ""
                root.noticeText = text
            }
        }
    }
}
