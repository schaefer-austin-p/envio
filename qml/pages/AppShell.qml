import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"
import "accounts"
import "inbox"

// The app below the window: which page shows (the wizard while there are no
// accounts, else the inbox), the pages the inbox leads to and the full-page
// problem screen. Main
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
    property bool retrying: false

    readonly property int accountCount: accountRows.count

    // What the bottom of the stack is: "wizard", "inbox" or "" (nothing yet).
    property string rootKind: ""
    // The first page once the core is ready: the wizard without accounts.
    // The stack is replaced only when its root has to change, so a page the
    // user opened (the Outbox) survives a worker restart.
    function route() {
        const want = accountCount === 0 ? "wizard" : "inbox"
        if (rootKind === want && stack.depth > 0) return
        rootKind = want
        stack.replace(null, want === "wizard" ? wizardPage : inboxPage)
    }
    // Back from the wizard: where it came from, or the inbox once the first
    // account exists. With none, there is nowhere else to go.
    function wizardClosed() {
        if (accountCount === 0) return
        if (stack.depth > 1) stack.pop()
        else {
            rootKind = "inbox"
            stack.replace(null, inboxPage)
        }
    }

    Repeater {
        id: accountRows
        model: root.accounts
        delegate: Item {
            visible: false
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
            objectName: "outboxPage"
            theme: root.theme
            core: root.core
            outbox: root.outbox
            onBackRequested: stack.pop()
        }
    }

    ProblemPage {
        anchors.fill: parent
        z: 10
        visible: root.problem !== ""
        theme: root.theme
        text: root.problem
        busy: root.retrying
        // The page stays up, busy, until the core is ready or reports again.
        onRetry: {
            root.retrying = true
            root.core.start()
            root.core.refresh("")
        }
    }

    Connections {
        target: root.core
        function onReadyChanged() {
            if (!root.core.ready) return
            // The core came up (maybe by itself, after a store problem).
            root.problem = ""
            root.retrying = false
            root.route()
        }
        function onStoreProblem(text) { root.retrying = false; root.problem = text }
        function onStartFailed(text) { root.retrying = false; root.problem = text }
        // A trust prompt is the setup wizard's; it answers its own. Nothing
        // outside the wizard accepts a certificate.
    }
    // The last account was removed: back to the wizard.
    onAccountCountChanged: if (core.ready && accountCount === 0) route()

    Connections {
        target: Qt.application
        function onStateChanged() { root.core.setActive(Qt.application.state === Qt.ApplicationActive) }
    }
    Component.onCompleted: if (core.ready) route()
}
