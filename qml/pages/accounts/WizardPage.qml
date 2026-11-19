import QtQuick 2.15
import "../../platform"
import "../../components"

// The account wizard (boards 13 and 14): address, sign in (or typed
// servers), options. It holds only UI state; what to do comes from the
// core's signals. Passwords stay in the step fields and are cleared after
// `accountAdded` and when the wizard closes.
Item {
    id: root
    required property Theme theme
    required property var core
    // "email", "signin", "browser" (waiting for the provider's page),
    // "servers" or "options".
    property string step: "email"
    // The provider this address signs in with ("Microsoft"), else "": the
    // sign-in step then has a button and no password.
    property string oauthProvider: ""
    readonly property alias oauthSession: session
    property string error: ""
    property bool busy: false
    // The address `detect` was asked about; an answer for another is stale.
    property string asked: ""
    // True while an attempt is in flight. `asked` alone cannot say so: manual
    // setup may go without an address, and then `asked` is "" too.
    property bool attempting: false
    property string foundText: ""
    property string serversText: ""
    property string setupJson: ""
    readonly property var setup: setupJson === "" ? ({}) : JSON.parse(setupJson)
    // True only after "Use these servers" for the hosts now in `setup`.
    property bool serversConfirmed: false
    property string addedKey: ""
    // What the account was created with, for step 3.
    property string accountName: ""
    // False on first run: with no account the wizard is the root page, so
    // its first step has nowhere to go back to.
    property bool canLeave: true
    signal closed()

    function clearPasswords() {
        signIn.password = ""
        servers.password = ""
    }
    function close() {
        clearPasswords()
        root.closed()
    }
    // Drop everything tied to the address being set up: the passwords,
    // the confirmation of its hosts, and interest in any answer still on
    // its way (`attempting` is off, so late events are ignored).
    function forget() {
        clearPasswords()
        session.reset()
        oauthProvider = ""
        serversConfirmed = false
        busy = false
        asked = ""
        attempting = false
        error = ""
    }
    function back() {
        // The provider's page is open or the user is asked: Back is Cancel
        // and the page stays on the sign-in step.
        if (step === "browser" || session.state === "confirm") {
            session.cancel()
            return
        }
        if (step === "email" || step === "options") {
            error = ""
            close()
        } else {
            forget()
            step = "email"
        }
    }
    // Is `email` the address of the attempt in flight?
    function current(email) {
        return attempting && String(email).toLowerCase() === asked.toLowerCase()
    }
    function detect() {
        forget()
        busy = true
        asked = emailStep.email.trim()
        attempting = true
        core.detect(asked)
    }
    // Only the sign-in and servers steps connect (a provider sign-in goes
    // through `session`, not `connect`).
    readonly property bool connecting: (step === "signin" && oauthProvider === "") || step === "servers"
    function submit() {
        if (!connecting) return
        error = ""
        busy = true
        attempting = true
        if (step === "servers") {
            // The user typed these hosts: their own choice, so confirmed.
            asked = emailStep.email.trim()
            accountName = asked
            core.connect(servers.setupJson(), servers.password, true)
        } else {
            asked = setup.email
            accountName = setup.name
            core.connect(setupJson, signIn.password, serversConfirmed)
        }
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: root.canLeave || (root.step !== "email" && root.step !== "options")
        title: root.step === "email" ? qsTr("Add account")
             : (root.step === "signin" || root.step === "browser") && root.oauthProvider !== ""
                 ? qsTr("Sign in · %1").arg(root.oauthProvider)
             : root.step === "signin" ? qsTr("Sign in")
             : root.step === "servers" ? qsTr("Server settings")
             : qsTr("Account options")
        onBackClicked: root.back()
    }

    EmailStep {
        id: emailStep
        theme: root.theme
        core: root.core
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "email"
        error: root.error
        busy: root.busy
        onContinued: root.detect()
        // Editing the address ends the attempt for the old one.
        onEmailChanged: root.forget()
        // A detect still running is dropped: its answer must not move us.
        onManualRequested: { root.forget(); servers.prefill(null); root.step = "servers" }
    }
    SignInStep {
        id: signIn
        theme: root.theme
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "signin"
        foundText: root.foundText
        serversText: root.serversText
        appPassword: root.setup.source === "provider"
        error: root.error
        busy: root.busy
        oauthProvider: root.oauthProvider
        oauthEmail: root.asked
        noticeKind: session.noticeKind
        noticeTitle: session.noticeTitle
        noticeText: session.noticeText
        confirming: session.state === "confirm"
        onContinued: root.submit()
        onEditServersRequested: { root.forget(); servers.prefill(root.setup); root.step = "servers" }
        onOauthRequested: { root.error = ""; session.start() }
        onOauthAnswered: function(accept) { session.answer(accept) }
    }
    BrowserStep {
        objectName: "browserStep"
        theme: root.theme
        session: session
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "browser"
        onCancelRequested: session.cancel()
    }
    OAuthSession {
        id: session
        objectName: "oauthSession"
        core: root.core
        email: root.asked
        // The wizard waits for the account while the browser or the core has
        // the attempt (`onAccountAdded` only takes an attempt that is busy).
        onStateChanged: {
            root.busy = state === "waiting" || state === "saving"
            if (state === "waiting") root.step = "browser"
            else if (root.step === "browser") root.step = "signin"
        }
    }
    ServersStep {
        objectName: "serversStep"
        id: servers
        theme: root.theme
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "servers"
        email: emailStep.email.trim()
        error: root.error
        busy: root.busy
        onContinued: root.submit()
    }
    OptionsStep {
        id: options
        theme: root.theme
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "options"
        accountName: root.accountName
        onDone: {
            if (options.isDefault) {
                root.core.saveSettings(root.core.theme, root.core.darkMode, root.core.imagesAlways, root.addedKey)
            }
            root.close()
        }
    }

    ConfirmHostsDialog {
        id: confirmHosts
        objectName: "confirmHosts"
        theme: root.theme
        imapHost: root.setup.imapHost !== undefined ? root.setup.imapHost : ""
        imapPort: root.setup.imapPort !== undefined ? root.setup.imapPort : 0
        smtpHost: root.setup.smtpHost !== undefined ? root.setup.smtpHost : ""
        smtpPort: root.setup.smtpPort !== undefined ? root.setup.smtpPort : 0
        onConfirmed: root.serversConfirmed = true
        onCancelled: { root.forget(); root.step = "email" }
        // Cancel, or dismissing the dialog any other way: back to the address.
        onClosed: if (!root.serversConfirmed && root.step === "signin") { root.forget(); root.step = "email" }
    }
    TrustDialog {
        id: trust
        objectName: "trustDialog"
        theme: root.theme
        // Setup has no account yet: the accepted fingerprint is kept for
        // the next `connect`, which is sent again.
        onTrusted: {
            root.core.acceptSetupCertificate(trust.service, trust.hex)
            root.submit()
        }
    }

    Connections {
        target: root.core
        function onDetected(email, text, needsConfirm, json, serversText, oauthProvider) {
            if (!root.current(email)) return
            root.busy = false
            root.oauthProvider = oauthProvider
            root.foundText = text
            root.serversText = serversText
            root.setupJson = json
            root.serversConfirmed = false
            root.step = "signin"
            // A provider sign-in has no servers to confirm: they are the provider's.
            if (needsConfirm && oauthProvider === "") confirmHosts.open()
        }
        function onSetupProblem(email, text) {
            if (!root.current(email)) return
            root.busy = false
            root.error = text
        }
        function onTrustPrompt(email, service, host, title, fingerprint, hex, text) {
            if (!root.current(email) || !root.busy || !root.connecting) return
            root.busy = false
            trust.service = service
            trust.host = host
            trust.heading = title
            trust.fingerprint = fingerprint
            trust.hex = hex
            trust.body = text
            trust.open()
        }
        function onAccountAdded(account, email) {
            // Only the attempt in flight, for its own address, counts.
            if (!root.busy || !root.current(email)) return
            root.busy = false
            root.clearPasswords()
            root.addedKey = account
            root.step = "options"
        }
    }
}
