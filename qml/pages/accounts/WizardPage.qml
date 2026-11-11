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
    signal closed()

    function clearPasswords() {
        signIn.password = ""
        servers.password = ""
    }
    function close() {
        clearPasswords()
        root.closed()
    }
    function back() {
        error = ""
        if (step === "email" || step === "options") close()
        else step = "email"
    }
    function detect() {
        error = ""
        busy = true
        asked = emailStep.email.trim()
        core.detect(asked)
    }
    function submit() {
        error = ""
        busy = true
        if (step === "servers") core.connect(servers.setupJson(), servers.password, false)
        else core.connect(setupJson, signIn.password, serversConfirmed)
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
        onManualRequested: { root.error = ""; root.step = "servers" }
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
        accountName: root.setup.name !== undefined ? root.setup.name : emailStep.email.trim()
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
        onCancelled: root.step = "email"
        // Cancel, or dismissing the dialog any other way: back to the address.
        onClosed: if (!root.serversConfirmed && root.step === "signin") root.step = "email"
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
            if (email !== root.asked) return
            root.busy = false
            root.foundText = text
            root.setupJson = json
            root.serversConfirmed = false
            root.step = "signin"
            if (needsConfirm) confirmHosts.open()
        }
        function onSetupProblem(email, text) {
            root.busy = false
            root.error = text
        }
        function onTrustPrompt(email, service, host, fingerprint, hex, text) {
            root.busy = false
            trust.service = service
            trust.host = host
            trust.fingerprint = fingerprint
            trust.hex = hex
            trust.body = text
            trust.open()
        }
        function onAccountAdded(account) {
            root.busy = false
            root.clearPasswords()
            root.addedKey = account
            root.step = "options"
        }
    }
}
