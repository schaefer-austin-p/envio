import QtQuick 2.15
import "../../platform"
import "../../components"
import "wizardFlow.js" as Flow

// The account wizard (boards 13 and 14): provider, then address, sign in (or
// typed servers or an app password), options. It holds only UI state; what to do comes from the
// core's signals. Passwords stay in the step fields and are cleared after
// `accountAdded` and when the wizard closes.
Item {
    id: root
    required property Theme theme
    required property var core
    // "provider", "email" (Other), "signin", "browser" (waiting for the
    // provider's page), "apppass" (iCloud, Gmail, Yahoo), "servers" or "options".
    property string step: "provider"
    // The pick: "outlook", "m365", "icloud", "gmail", "yahoo" or "other".
    property string provider: ""
    // What the pick asks of the sign-in: "graph" (Outlook.com), "imap"
    // (Microsoft 365), or "" (the address's own provider).
    property string oauthKind: ""
    readonly property bool picked: Flow.isMicrosoft(provider)
    // A refused sign-in is told on the provider step.
    property string providerNoticeKind: ""
    property string providerNoticeTitle: ""
    property string providerNoticeText: ""
    // The provider this address signs in with ("Microsoft"), else "": the
    // sign-in step then has a button and no password.
    property string oauthProvider: ""
    readonly property alias oauthSession: session
    property string error: ""
    // The address cannot be used (Microsoft, #381): shown as a card on the
    // email step until the address is edited.
    property string refusedTitle: ""
    property string refusedText: ""
    property bool manualOk: false
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
        appPass.password = ""
    }
    function pick(id) {
        forget()
        providerNoticeKind = ""
        providerNoticeTitle = ""
        providerNoticeText = ""
        provider = id
        if (Flow.isMicrosoft(id)) {
            oauthProvider = "Microsoft"
            oauthKind = id === "outlook" ? "graph" : "imap"
            signIn.oauthEmail = ""
            step = "signin"
        } else if (id === "other") {
            step = "email"
        } else {
            appPass.email = ""
            step = "apppass"
        }
    }
    // The sign-in ended with nothing saved: tell it where the pick was made,
    // else on the sign-in step.
    function refuse(outcome, text) {
        const card = Flow.refusal(outcome, text)
        if (picked) {
            forget()
            provider = ""
            providerNoticeKind = card.kind
            providerNoticeTitle = card.title
            providerNoticeText = card.text
            step = "provider"
        } else {
            session.noticeKind = card.kind
            session.noticeTitle = card.title
            session.noticeText = card.text
        }
    }
    // "Sign in with Microsoft": after a pick the address is typed on the page.
    function signInWithMicrosoft() {
        error = ""
        if (picked) {
            asked = signIn.oauthEmail.trim()
            attempting = true
            accountName = asked.split("@")[0]
        } else {
            accountName = setup.name
        }
        session.start()
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
        oauthKind = ""
        serversConfirmed = false
        busy = false
        asked = ""
        attempting = false
        error = ""
        refusedTitle = ""
        refusedText = ""
        manualOk = false
    }
    function back() {
        // The core is storing the account: Back would drop its AccountAdded.
        if (session.state === "saving") return
        // The provider's page is open or the user is asked: Back is Cancel
        // and the page stays on the sign-in step.
        if (step === "browser" || session.state === "confirm" || session.state === "mismatch") {
            session.cancel()
            return
        }
        if (step === "provider" || step === "options") {
            error = ""
            close()
        } else {
            // From the second step on, Back is the pick again; on the Other
            // path the later steps go back through the address.
            const toProvider = step === "email" || step === "apppass" || (provider !== "" && provider !== "other")
            forget()
            if (toProvider) provider = ""
            step = toProvider ? "provider" : "email"
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
    readonly property bool connecting: (step === "signin" && oauthProvider === "") || step === "servers" || step === "apppass"
    function submit() {
        if (!connecting) return
        error = ""
        busy = true
        attempting = true
        if (step === "apppass") {
            asked = appPass.email.trim()
            const json = core.appPasswordSetup(provider, asked)
            accountName = json === "" ? asked : JSON.parse(json).name
            core.connect(json, appPass.password, false)
        } else if (step === "servers") {
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
        showBack: root.canLeave || (root.step !== "provider" && root.step !== "options")
        title: root.step === "email" || root.step === "provider" ? qsTr("Add account")
             : (root.step === "signin" || root.step === "browser") && root.oauthProvider !== ""
                 ? qsTr("Sign in · %1").arg(root.picked ? Flow.providerName(root.provider) : root.oauthProvider)
             : root.step === "apppass" ? qsTr("Sign in · %1").arg(root.provider === "icloud" ? "iCloud" : root.provider === "gmail" ? "Gmail" : "Yahoo")
             : root.step === "signin" ? qsTr("Sign in")
             : root.step === "servers" ? qsTr("Server settings")
             : qsTr("Account options")
        onBackClicked: root.back()
    }

    StepCounter {
        id: counter
        objectName: "stepCounter"
        theme: root.theme
        anchors { top: bar.bottom; left: parent.left; right: parent.right }
        step: Flow.stepNumber(root.step, root.provider)
        total: Flow.stepTotal(root.step, root.provider)
    }
    ProviderStep {
        objectName: "providerStep"
        theme: root.theme
        anchors { top: counter.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "provider"
        noticeKind: root.providerNoticeKind
        noticeTitle: root.providerNoticeTitle
        noticeText: root.providerNoticeText
        onPicked: function(id) { root.pick(id) }
        onCancelRequested: root.close()
    }
    AppPasswordStep {
        id: appPass
        objectName: "appPasswordStep"
        theme: root.theme
        anchors { top: counter.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "apppass"
        provider: root.provider
        error: root.error
        busy: root.busy
        onContinued: root.submit()
    }
    EmailStep {
        id: emailStep
        theme: root.theme
        core: root.core
        anchors { top: counter.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "email"
        error: root.error
        refusedTitle: root.refusedTitle
        refusedText: root.refusedText
        manualOk: root.manualOk
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
        anchors { top: counter.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "signin"
        foundText: root.foundText
        serversText: root.serversText
        appPassword: root.setup.source === "provider"
        error: root.error
        busy: root.busy
        oauthProvider: root.oauthProvider
        oauthEditable: root.picked
        mismatch: session.state === "mismatch"
        noticeKind: session.noticeKind
        noticeTitle: session.noticeTitle
        noticeText: session.noticeText
        confirming: session.state === "confirm"
        onContinued: root.submit()
        onEditServersRequested: { root.forget(); servers.prefill(root.setup); root.step = "servers" }
        onOauthRequested: root.signInWithMicrosoft()
        onOauthAnswered: function(accept) { session.answer(accept) }
    }
    BrowserStep {
        objectName: "browserStep"
        theme: root.theme
        session: session
        anchors { top: counter.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "browser"
        onCancelRequested: session.cancel()
    }
    OAuthSession {
        id: session
        objectName: "oauthSession"
        core: root.core
        email: root.asked
        kind: root.oauthKind
        onDeclined: function(outcome, text) { root.refuse(outcome, text) }
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
        anchors { top: counter.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        visible: root.step === "servers"
        email: emailStep.email.trim()
        error: root.error
        busy: root.busy
        onContinued: root.submit()
    }
    OptionsStep {
        id: options
        theme: root.theme
        anchors { top: counter.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
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
        onClosed: if (!confirmHosts.chosen && !root.serversConfirmed && root.step === "signin") { root.forget(); root.step = "email" }
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
            signIn.oauthEmail = root.asked
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
        // The mail is on Microsoft 365 (#384): for now the card says so, with
        // the way out of "Set up manually"; the sign-in button comes with the
        // wizard banner.
        function onMailOnMicrosoft(email, title, text) {
            if (!root.current(email)) return
            root.busy = false
            root.refusedTitle = title
            root.refusedText = text
            root.manualOk = true
        }
        function onSetupRefused(email, title, text, manualOk) {
            if (!root.current(email)) return
            root.busy = false
            root.refusedTitle = title
            root.refusedText = text
            root.manualOk = manualOk
            // Manual setup skips Detect: the refusal comes back from Connect,
            // so the card is shown where the address is.
            if (root.step === "servers") {
                root.clearPasswords()
                root.step = "email"
            }
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
