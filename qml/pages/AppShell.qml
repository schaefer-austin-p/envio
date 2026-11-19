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

    // Wide layout: the list and a pane side by side from 90 gu, back to the
    // page stack only below 86 gu, so a window dragged around the edge does
    // not flap.
    readonly property bool wide: _wide
    property bool _wide: false
    function checkWide() {
        if (!theme) return
        const on = width >= theme.gu(90), off = width < theme.gu(86)
        if (!_wide && on) _wide = true
        else if (_wide && off) {
            // A compose in the pane goes to the stack as the top page, with
            // everything typed; the pane would be too narrow to use. Nothing
            // closes it in the core.
            const carried = paneCompose > 0 && pane.item !== null
                ? { id: paneCompose, state: pane.item.snapshot() } : null
            _wide = false
            if (carried !== null) {
                paneCompose = 0
                stack.push(composePage, { compose: carried.id }).restore(carried.state)
            }
            // The narrow list has no reader to go with an open row.
            if (core.openMessage > 0) core.setOpenMessage(0)
        }
    }
    onWidthChanged: checkWide()
    onThemeChanged: checkWide()
    onPaneComposeChanged: checkWide()
    Connections {
        target: root.theme
        function onGridUnitChanged() { root.checkWide() }
    }
    // The compose the pane shows (an id), 0 for none. `core` has no compose
    // id of its own.
    property int paneCompose: 0

    // Compose pages: the core made the compose, the page follows it. Wide,
    // over the list (nothing on top of it), it opens in the pane.
    function composeOpened(compose) {
        if (wide && stack.depth === 1 && rootKind === "inbox") {
            // One compose in the pane: the one that was there closes first
            // (its draft stays in Drafts), and its page goes with it.
            if (paneCompose > 0 && paneCompose !== compose && pane.item !== null) pane.item.leave()
            paneCompose = compose
        } else stack.push(composePage, { compose: compose })
    }
    // A tap on a row: the pane when wide, else the reader page.
    function openMessageFromRow(message) {
        if (wide) core.setOpenMessage(message)
        else stack.push(readerPage, { messageId: message })
    }
    // A tap on a local draft's row: Compose opens on it at once.
    function openDraft(draftId) {
        const id = core.startCompose("draft", 0, "", draftId)
        if (id > 0) composeOpened(id)
    }
    function newMessage() {
        const id = core.startCompose("new", 0, composeAccount, "")
        if (id > 0) composeOpened(id)
    }
    // A mailto: link waits until the core is up and there is an account to
    // write from; then the core fills the compose from it.
    property alias uriSource: links.sourceFile
    readonly property var uriSchemes: links.schemes
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
    // [{ paths, transfer }]
    property var pendingShare: []
    // compose id -> paths: attached once the core's worker has recorded the
    // compose (`composeReady`); attach runs on another thread and fails for a
    // compose the worker does not know yet.
    property var shareWaiting: ({})
    // Main turns this on: only the real app listens to the Content Hub.
    property bool receiveShares: false
    function openShared(paths, transfer) {
        pendingShare = pendingShare.concat([{ paths: paths, transfer: transfer }])
        startPendingShare()
    }
    function startPendingShare() {
        if (pendingShare.length === 0 || !core.ready || accountCount === 0 || rootKind !== "inbox") return
        const entries = pendingShare
        pendingShare = []
        const id = core.startCompose("new", 0, composeAccount, "")
        if (id <= 0) return
        let paths = []
        for (const e of entries) paths = paths.concat(e.paths)
        shareIn.track(id, paths.length, entries.map(function(e) { return e.transfer }))
        const waiting = shareWaiting
        waiting[id] = paths
        shareWaiting = waiting
        composeOpened(id)
    }
    // The compose ended before it was ready (refused, or Back): nothing is
    // attached, so the transfer is done with. Once the files are handed to
    // the core they settle through its Attached and AttachFailed instead.
    function dropShare(compose) {
        if (shareWaiting[compose] === undefined) return
        delete shareWaiting[compose]
        shareIn.cancel(compose)
    }
    // Attaching follows composeReady, so the page's fill() has run before
    // any Attached arrives (the attach result is an event queued after it).
    function attachShared(compose) {
        const paths = shareWaiting[compose]
        if (paths === undefined) return
        delete shareWaiting[compose]
        for (const p of paths) core.attach(compose, p)
    }

    // For tests: where a transfer is handed in (`shares.accept(transfer)`).
    readonly property alias shares: shareIn
    ShareIn {
        id: shareIn
        listening: root.receiveShares
        onFilesShared: function(paths, transfer) { root.openShared(paths, transfer) }
    }

    Platform.Uri {
        id: links
        schemes: ["mailto"]
        onOpened: function(uri) { root.openMailto(uri) }
    }

    // The browser's redirect after a Microsoft sign-in comes back as an
    // `envio-oauth:` link; it waits until the core is up, then goes to it. The
    // core ignores one it did not ask for.
    property string pendingRedirect: ""
    function openRedirect(link) {
        pendingRedirect = link
        sendPendingRedirect()
    }
    function sendPendingRedirect() {
        if (pendingRedirect === "" || !core.ready) return
        const link = pendingRedirect
        pendingRedirect = ""
        core.oauthRedirect(link)
    }
    Connections {
        target: root.core
        function onReadyChanged() { root.sendPendingRedirect() }
    }
    Platform.Uri {
        id: redirects
        objectName: "redirectUri"
        schemes: ["envio-oauth"]
        sourceFile: links.sourceFile
        onOpened: function(uri) { root.openRedirect(uri) }
    }
    // For tests: where a redirect is offered.
    readonly property alias redirectUri: redirects

    // The page that leaves is hidden at once; the new one slides in a little
    // and fades in. The style's own transitions keep both pages (whose roots
    // are transparent) on screen together for about half a second.
    AirmailFrame {
        id: frame
        theme: root.theme
        anchors.fill: parent
        StackView {
            id: stack
            objectName: "stack"
            // Beside the pane the stack is the list's 50 gu and ends at the
            // hairline; pages above the inbox hide the pane and get it all.
            anchors { top: parent.top; bottom: parent.bottom; left: parent.left }
            width: pane.visible ? root.theme.gu(50) : parent.width
            clip: pane.visible
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

        // The hairline between the list and the pane.
        Rectangle {
            objectName: "paneHairline"
            visible: pane.visible
            x: root.theme.gu(50)
            width: 1
            height: parent.height
            color: root.theme.hairline
            z: 1
        }
        // The right pane of the wide layout, beside the inbox at the root.
        // Pages above the inbox (Search, Outbox, Settings, Accounts) cover
        // the whole window, so the pane is not shown over them.
        Loader {
            id: pane
            objectName: "pane"
            visible: root.wide && stack.depth === 1 && root.rootKind === "inbox"
            active: root.wide
            anchors { top: parent.top; bottom: parent.bottom; right: parent.right }
            width: parent.width - root.theme.gu(50)
            // Opaque: the pages are transparent, so without it the list
            // shows through.
            Rectangle {
                objectName: "paneBackground"
                anchors.fill: parent
                z: -1
                color: root.theme.ground
            }
            sourceComponent: root.paneCompose > 0 ? paneComposeC : root.core.openMessage > 0 ? paneReaderC : emptyC
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

    Component { id: emptyC; ReaderPane { theme: root.theme } }
    Component {
        id: paneReaderC
        ReaderPage {
            wide: true
            theme: root.theme
            core: root.core
            accounts: root.accounts
            folders: root.folders
            // openMessage is a qlonglong in the glue; a real property does
            // not take it unconverted.
            messageId: Number(root.core.openMessage)
            onBackRequested: root.core.setOpenMessage(0)
            onComposeOpened: function(compose) { root.composeOpened(compose) }
        }
    }
    Component {
        id: paneComposeC
        ComposePage {
            wide: true
            theme: root.theme
            core: root.core
            accounts: root.accounts
            suggestions: root.suggestions
            compose: root.paneCompose
            onClosed: { root.dropShare(compose); root.paneCompose = 0 }
            onAttachStarted: function(count, transfer) { shareIn.expectPick(compose, count, transfer) }
            onSent: function(text) { root.paneCompose = 0; sentToast.show(text) }
        }
    }
    Component {
        id: inboxPage
        InboxPage {
            // The list pane when wide, else the whole page (the stack does
            // not resize an item that sets its own size).
            width: root.wide ? root.theme.gu(50) : stack.width
            height: stack.height
            theme: root.theme
            core: root.core
            accounts: root.accounts
            folders: root.folders
            messages: root.messages
            outbox: root.outbox
            onMessageRequested: root.openMessageFromRow(message)
            onDraftRequested: function(draftId) { root.openDraft(draftId) }
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
            onSignInAgainRequested: stack.push(signInAgainPage, { account: account })
        }
    }
    Component {
        id: signInAgainPage
        SignInAgainPage {
            objectName: "signInAgainPage"
            theme: root.theme
            core: root.core
            accounts: root.accounts
            onClosed: stack.pop()
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
            accounts: root.accounts
            onAccountsRequested: stack.push(accountsPage)
            onSignInAgainRequested: function(account) { stack.push(signInAgainPage, { account: account }) }
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
            onClosed: { root.dropShare(compose); stack.pop() }
            onAttachStarted: function(count, transfer) { shareIn.expectPick(compose, count, transfer) }
            // Queued arrives late: close this page, not whatever is on top
            // by now. A page below the top closes when it is shown again.
            onSent: function(text) {
                if (stack.currentItem === page) stack.pop()
                else page.done = true
                sentToast.show(text)
            }
        }
    }

    // Ctrl+N: a new message, from the list.
    readonly property alias newShortcut: newKey
    Shortcut {
        id: newKey
        sequence: "Ctrl+N"
        enabled: root.core.ready && root.rootKind === "inbox" && stack.depth === 1 && root.paneCompose === 0
        onActivated: root.newMessage()
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
        function onComposeReady(compose) { root.attachShared(compose) }
        function onComposeRefused(compose) { root.dropShare(compose) }
        function onAttached(compose) { shareIn.settle(compose) }
        function onAttachFailed(compose) { shareIn.settle(compose) }
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
    Component.onCompleted: { checkWide(); if (core.ready) route() }
}
