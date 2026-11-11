import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"
import "accounts"
import "inbox"

// The app below the window: which page shows (the wizard while there are no
// accounts, else the inbox), the pages the inbox leads to, the trust prompt
// for an account already set up, and the full-page problem screen. Main
// creates EnvioCore and its models and hands them in, so tests can hand in a
// MockCore.
Item {
    id: root
    required property Theme theme
    required property var core
    required property var accounts
    required property var folders
    required property var messages
    required property var outbox
    // A start or store problem in words, shown full-page until "Try again".
    property string problem: ""

    readonly property int accountCount: accountRows.count
    readonly property bool wizardOpen: stack.currentItem !== null && stack.currentItem.objectName === "wizardPage"

    // The first page once the core is ready: the wizard without accounts.
    function route() {
        stack.replace(null, accountCount === 0 ? wizardPage : inboxPage)
    }
    function keyOf(email) {
        for (let i = 0; i < accountRows.count; i++) {
            const row = accountRows.itemAt(i)
            if (row !== null && row.email.toLowerCase() === String(email).toLowerCase()) return row.key
        }
        return ""
    }
    // Back from the wizard: where it came from, or the inbox once the first
    // account exists. With none, there is nowhere else to go.
    function wizardClosed() {
        if (accountCount === 0) return
        if (stack.depth > 1) stack.pop()
        else stack.replace(null, inboxPage)
    }

    Repeater {
        id: accountRows
        model: root.accounts
        delegate: Item {
            visible: false
            required property var model
            readonly property string key: model.key
            readonly property string email: model.email
        }
    }

    StackView {
        id: stack
        anchors.fill: parent
    }

    Component {
        id: inboxPage
        InboxPage {
            theme: root.theme
            core: root.core
            accounts: root.accounts
            folders: root.folders
            messages: root.messages
            outbox: root.outbox
            onMessageRequested: root.core.open(message, false, Math.round(width))
            onAccountsRequested: stack.push(accountsPage)
            onOutboxRequested: stack.push(outboxPage)
            // The reader, compose, search and settings arrive with their pages.
        }
    }
    Component {
        id: wizardPage
        WizardPage {
            objectName: "wizardPage"
            theme: root.theme
            core: root.core
            onClosed: root.wizardClosed()
        }
    }
    Component {
        id: accountsPage
        AccountsPage {
            theme: root.theme
            core: root.core
            accounts: root.accounts
            onBackRequested: stack.pop()
            onAddRequested: stack.push(wizardPage)
            onUpdatePasswordRequested: stack.push(updatePasswordPage, { account: account, accountName: name })
        }
    }
    Component {
        id: updatePasswordPage
        UpdatePasswordPage {
            theme: root.theme
            core: root.core
            onClosed: stack.pop()
        }
    }
    Component {
        id: outboxPage
        OutboxPage {
            theme: root.theme
            core: root.core
            outbox: root.outbox
            onBackRequested: stack.pop()
        }
    }

    // An existing account's certificate pin changed or is unknown. (Setup
    // asks through the wizard, which answers its own prompts.)
    TrustDialog {
        id: trust
        objectName: "trustDialog"
        theme: root.theme
        property string account: ""
        onTrusted: root.core.acceptCertificate(trust.account, trust.service, trust.hex)
    }

    ProblemPage {
        anchors.fill: parent
        z: 10
        visible: root.problem !== ""
        theme: root.theme
        text: root.problem
        onRetry: {
            root.problem = ""
            root.core.start()
            root.core.refresh("")
        }
    }

    Connections {
        target: root.core
        function onReadyChanged() { if (root.core.ready) root.route() }
        function onStoreProblem(text) { root.problem = text }
        function onStartFailed(text) { root.problem = text }
        function onTrustPrompt(email, service, host, fingerprint, hex, text) {
            if (root.wizardOpen) return
            const key = root.keyOf(email)
            if (key === "") return
            trust.account = key
            trust.service = service
            trust.host = host
            trust.fingerprint = fingerprint
            trust.hex = hex
            trust.body = text
            trust.open()
        }
    }
    // The last account was removed: back to the wizard.
    onAccountCountChanged: if (core.ready && accountCount === 0 && !wizardOpen) route()

    Connections {
        target: Qt.application
        function onStateChanged() { root.core.setActive(Qt.application.state === Qt.ApplicationActive) }
    }
    Component.onCompleted: if (core.ready) route()
}
