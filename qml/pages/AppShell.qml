import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQml 2.15
import "../platform"
import "../components"
import "../bindle/Platform" as Platform
import "accounts"
import "compose"
import "inbox"
import "reader"
import "search"
import "settings"

// The app below the window: which page shows (the wizard while there are no
// accounts, else the inbox), the pages the inbox leads to, compose (from the
// inbox, the reader, the Outbox or a mailto: link) and the full-page problem
// screen. Main creates EnvioCore and its models and hands them in, so tests
// can hand in a MockCore.
Item {
    id: root
    required property Theme theme
    required property var core
    required property var accounts
    required property var folders
    required property var messages
    required property var outbox
    required property var suggestions
    // The search model (message rows for the search page).
    required property var searchResults
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
            startPendingMailto()
            startPendingShare()
        }
    }

    // The account a new message starts from: the default one, else the
    // first.
    property string firstAccount: ""
    readonly property string composeAccount: core.defaultAccount !== "" ? core.defaultAccount : firstAccount

    Repeater {
        id: accountRows
        model: root.accounts
        delegate: Item {
            id: accountRow
            required property var model
            required property int index
            visible: false
            Binding {
                target: root
                property: "firstAccount"
                value: accountRow.model.key
                restoreMode: Binding.RestoreNone
                when: accountRow.index === 0
            }
        }
    }

    // For tests: the page on top and how many there are.
    readonly property Item currentPage: stack.currentItem
    readonly property int depth: stack.depth
    // The ring's inside, where pages, sheets, dialogs and the problem page lie.
    readonly property Item frameArea: frame.area

    // Compose pages: the core made the compose, the page follows it.
    function composeOpened(compose) { stack.push(composePage, { compose: compose }) }
    function newMessage() {
        const id = core.startCompose("new", 0, composeAccount, "")
        if (id > 0) composeOpened(id)
    }
    // A mailto: link waits until the core is up and there is an account to
    // write from; then the core fills the compose from it.
    property alias uriSource: links.sourceFile
    property string pendingMailto: ""
    function openMailto(link) {
        pendingMailto = link
        startPendingMailto()
    }
    function startPendingMailto() {
        // Not over the wizard: its Options step comes after the account exists.
        if (pendingMailto === "" || !core.ready || accountCount === 0 || rootKind !== "inbox") return
        const link = pendingMailto
        pendingMailto = ""
        const id = core.startCompose("mailto", 0, "", link)
        if (id > 0) composeOpened(id)
    }

    // Files shared in from another app wait the same way (no account: the
    // wizard shows meanwhile); then a new message opens with them attached.
    // The core copies each file out of the hub's inbox (attach).
    property var pendingShare: []
    // Main turns this on: only the real app listens to the Content Hub.
    property bool receiveShares: false
    function openShared(paths) {
        pendingShare = pendingShare.concat(paths)
        startPendingShare()
    }
    function startPendingShare() {
        if (pendingShare.length === 0 || !core.ready || accountCount === 0 || rootKind !== "inbox") return
        const paths = pendingShare
        pendingShare = []
        const id = core.startCompose("new", 0, composeAccount, "")
        if (id <= 0) return
        composeOpened(id)
        for (const p of paths) core.attach(id, p)
    }

    // For tests: where a transfer is handed in (`shares.accept(transfer)`).
    readonly property alias shares: shareIn
    ShareIn {
        id: shareIn
        listening: root.receiveShares
        onFilesShared: function(paths) { root.openShared(paths) }
    }

    Platform.Uri {
        id: links
        schemes: ["mailto"]
        onOpened: function(uri) { root.openMailto(uri) }
    }

    // The page that leaves is hidden at once; the new one slides in a little
    // and fades in. The style's own transitions keep both pages (whose roots
    // are transparent) on screen together for about half a second.
    AirmailFrame {
        id: frame
        theme: root.theme
        anchors.fill: parent
        StackView {
            id: stack
            anchors.fill: parent
            readonly property int enterMs: 150
            readonly property int enterShift: 24

            pushEnter: Transition {
                ParallelAnimation {
                    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: stack.enterMs; easing.type: Easing.OutCubic }
                    NumberAnimation { property: "x"; from: stack.enterShift; to: 0; duration: stack.enterMs; easing.type: Easing.OutCubic }
                }
            }
            pushExit: Transition { PropertyAction { property: "opacity"; value: 0 } }
            popEnter: Transition {
                ParallelAnimation {
                    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: stack.enterMs; easing.type: Easing.OutCubic }
                    NumberAnimation { property: "x"; from: -stack.enterShift; to: 0; duration: stack.enterMs; easing.type: Easing.OutCubic }
                }
            }
            popExit: Transition { PropertyAction { property: "opacity"; value: 0 } }
            replaceEnter: Transition {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: stack.enterMs; easing.type: Easing.OutCubic }
            }
            replaceExit: Transition { PropertyAction { property: "opacity"; value: 0 } }
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
            onMessageRequested: stack.push(readerPage, { messageId: message })
            onAccountsRequested: stack.push(accountsPage)
            onOutboxRequested: stack.push(outboxPage)
            onComposeRequested: root.newMessage()
            onSearchRequested: function(folder) { stack.push(searchPage, { folderId: folder }) }
            onSettingsRequested: stack.push(settingsPage)
        }
    }
    Component {
        id: searchPage
        SearchPage {
            theme: root.theme
            core: root.core
            accounts: root.accounts
            results: root.searchResults
            onBackRequested: stack.pop()
            onMessageRequested: function(message) { stack.push(readerPage, { messageId: message }) }
        }
    }
    Component {
        id: settingsPage
        SettingsPage {
            theme: root.theme
            core: root.core
            onBackRequested: stack.pop()
            onAppearanceRequested: stack.push(appearancePage)
            onSyncRequested: stack.push(syncPage)
            onNotificationsRequested: stack.push(notificationsPage)
            onImagesRequested: stack.push(imagesPage)
            onAccountsRequested: stack.push(accountsPage)
            onAboutRequested: stack.push(aboutPage)
        }
    }
    Component {
        id: appearancePage
        AppearancePage { theme: root.theme; core: root.core; onBackRequested: stack.pop() }
    }
    Component {
        id: syncPage
        SyncPage { theme: root.theme; accounts: root.accounts; core: root.core; onBackRequested: stack.pop() }
    }
    Component {
        id: notificationsPage
        NotificationsPage { theme: root.theme; accounts: root.accounts; core: root.core; onBackRequested: stack.pop() }
    }
    Component {
        id: imagesPage
        ImagesPage { theme: root.theme; core: root.core; onBackRequested: stack.pop() }
    }
    Component {
        id: aboutPage
        AboutPage { theme: root.theme; onBackRequested: stack.pop() }
    }
    Component {
        id: readerPage
        ReaderPage {
            theme: root.theme
            core: root.core
            accounts: root.accounts
            folders: root.folders
            onBackRequested: stack.pop()
            onComposeOpened: function(compose) { root.composeOpened(compose) }
        }
    }
    Component {
        id: wizardPage
        WizardPage {
            objectName: "wizardPage"
            canLeave: root.accountCount > 0
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
            onAccountsRequested: stack.push(accountsPage)
            onBackRequested: stack.pop()
            onComposeOpened: function(compose) { root.composeOpened(compose) }
        }
    }
    Component {
        id: composePage
        ComposePage {
            id: page
            // Set when the message was queued while another page was on top.
            property bool done: false
            StackView.onStatusChanged: {
                // Later: the pop that showed this page is still completing.
                if (done && StackView.status === StackView.Active)
                    Qt.callLater(function() { if (stack.currentItem === page) stack.pop() })
            }
            theme: root.theme
            core: root.core
            accounts: root.accounts
            suggestions: root.suggestions
            onClosed: stack.pop()
            // Queued arrives late: close this page, not whatever is on top
            // by now. A page below the top closes when it is shown again.
            onSent: function(text) {
                if (stack.currentItem === page) stack.pop()
                else page.done = true
                sentToast.show(text)
            }
        }
    }

    Toast {
        id: sentToast
        objectName: "sentToast"
        theme: root.theme
        z: 5
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: root.theme.frameInset + root.theme.gu(2) }
    }

    Connections {
        target: root.core
        function onReadyChanged() {
            if (!root.core.ready) return
            // The core came up (maybe by itself, after a store problem).
            root.problem = ""
            root.retrying = false
            root.route()
            root.startPendingMailto()
            root.startPendingShare()
        }
        function onStoreProblem(text) { root.retrying = false; root.problem = text }
        function onStartFailed(text) { root.retrying = false; root.problem = text }
        // A trust prompt is the setup wizard's; it answers its own. Nothing
        // outside the wizard accepts a certificate.
    }
    // The last account was removed: back to the wizard.
    onAccountCountChanged: {
        if (core.ready && accountCount === 0) route()
        startPendingMailto()
        startPendingShare()
    }

    Connections {
        target: Qt.application
        function onStateChanged() { root.core.setActive(Qt.application.state === Qt.ApplicationActive) }
    }
    Component.onCompleted: if (core.ready) route()
}
