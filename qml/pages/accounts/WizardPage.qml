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
    // "email", "signin", "servers" or "options".
    property string step: "email"
    property string error: ""
    property bool busy: false
    // The address `detect` was asked about; an answer for another is stale.
    property string asked: ""
    property string foundText: ""
    property string setupJson: ""
    readonly property var setup: setupJson === "" ? ({}) : JSON.parse(setupJson)
    // True only after "Use these servers" for the hosts now in `setup`.
    property bool serversConfirmed: false
    property string addedKey: ""
    // What the account was created with, for step 3.
    property string accountName: ""
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
    // its way (`asked` is empty, so late events are ignored).
    function forget() {
        clearPasswords()
        serversConfirmed = false
        busy = false
        asked = ""
        error = ""
    }
    function back() {
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
        return asked !== "" && String(email).toLowerCase() === asked.toLowerCase()
    }
    function detect() {
        forget()
        busy = true
        asked = emailStep.email.trim()
        core.detect(asked)
    }
    function submit() {
        error = ""
        busy = true
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
        showBack: true
        title: root.step === "email" ? qsTr("Add account")
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
        onManualRequested: { root.forget(); root.step = "servers" }
    }
    SignInStep {
        id: signIn
        theme: root.theme
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "signin"
        foundText: root.foundText
        appPassword: root.setup.source === "provider"
        error: root.error
        busy: root.busy
        onContinued: root.submit()
    }
    ServersStep {
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
        function onDetected(email, text, needsConfirm, json) {
            if (!root.current(email)) return
            root.busy = false
            root.foundText = text
            root.setupJson = json
            root.serversConfirmed = false
            root.step = "signin"
            if (needsConfirm) confirmHosts.open()
        }
        function onSetupProblem(email, text) {
            if (!root.current(email)) return
            root.busy = false
            root.error = text
        }
        function onTrustPrompt(email, service, host, fingerprint, hex, text) {
            if (!root.current(email)) return
            root.busy = false
            trust.service = service
            trust.host = host
            trust.fingerprint = fingerprint
            trust.hex = hex
            trust.body = text
            trust.open()
        }
        function onAccountAdded(account) {
            // It names no address: only an attempt still in flight counts.
            if (!root.busy || root.asked === "") return
            root.busy = false
            root.clearPasswords()
            root.addedKey = account
            root.step = "options"
        }
    }
}
