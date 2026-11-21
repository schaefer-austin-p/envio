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
        dropItems()
        stack.replace(null, want === "wizard" ? wizardPage : inboxPage)
    }
    // Back from the wizard: where it came from, or the inbox once the first
    // account exists. With none, there is nowhere else to go.
    function wizardClosed() {
        if (accountCount === 0) return
        if (stack.depth > 1) stack.pop()
        else {
            rootKind = "inbox"
            dropItems()
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
        if (!_wide && on) toWide()
        else if (_wide && off) toNarrow()
        // The third column (the folders drawer) from 180 gu, back to the
        // overlay drawer below 176 gu; only ever while wide.
        const von = width >= theme.gu(180), voff = width < theme.gu(176)
        if (!_veryWide && _wide && von) _veryWide = true
        else if (_veryWide && (!_wide || voff)) _veryWide = false
        layoutPane()
    }
    readonly property bool veryWide: _veryWide
    property bool _veryWide: false
    // The inbox page (the stack's root while the inbox is the root), which
    // the inline drawer steers.
    property Item inboxItem: null
    // The inline drawer's width, the room it takes from the list and pane.
    readonly property real drawerWidth: veryWide ? theme.gu(30) : 0
    // The pane's width is set here, after the decision above, not bound to
    // the window: a binding would squeeze a compose in the pane for a moment
    // before it is moved, and the text would re-wrap and scroll (#332).
    property real paneWidth: 0
    function layoutPane() { paneWidth = Math.max(0, frame.area.width - theme.gu(50) - drawerWidth) }
    onWidthChanged: checkWide()
    onThemeChanged: checkWide()
    Connections {
        target: root.theme
        function onGridUnitChanged() { root.checkWide() }
        function onFrameInsetChanged() { root.checkWide() }
    }
    // The compose the pane shows (an id), 0 for none. `core` has no compose
    // id of its own.
    property int paneCompose: 0
    onPaneComposeChanged: syncPane()

    // The reader and the compose are single items, made once and moved
    // between the stack (narrow) and the pane (wide) when the window crosses
    // the breakpoint: nothing reopens, nothing reloads, and the core is never
    // told (`closeCompose` is only for Close, Send, a discard and a row click).
    // A page pushed from a component (Search's reader, a second compose) is
    // not one of these and stays a stack page.
    property Item readerItem: null
    property Item composeItem: null
    // How long a retired item lives on: the stack's pop transition may still
    // be drawing it.
    readonly property int retireMs: 600

    // Sized by its host, not by the stack (which sizes only items that did
    // not set a size).
    function fit(item, host) {
        item.x = 0
        item.y = 0
        item.width = Qt.binding(function() { return host.width })
        item.height = Qt.binding(function() { return host.height })
    }
    function toPane(item) {
        item.parent = pane
        item.wide = true
        item.visible = true
        item.opacity = 1
        fit(item, pane)
    }
    function toStack(item) {
        item.wide = false
        item.visible = true
        item.opacity = 1
        fit(item, stack)
        stack.push(item, StackView.Immediate)
    }
    function retire(item) {
        item.visible = false
        // Out of the pane at once (a new item may take its place); on the
        // stack it stays until its pop transition is over.
        if (item.parent === pane) item.parent = null
        item.destroy(retireMs)
    }
    function retireReader() {
        if (readerItem === null) return
        const item = readerItem
        readerItem = null
        retire(item)
    }
    // The stack is about to be replaced: the items that were on it (or in the
    // pane) go, and a later wide `syncPane` makes them again.
    function dropItems() {
        retireReader()
        retireCompose()
    }
    function retireCompose() {
        if (composeItem === null) return
        const item = composeItem
        composeItem = null
        retire(item)
    }
    // The pane shows the compose, else the reader of the open message, else
    // the empty state (a Loader). A reader under a compose waits, hidden.
    function syncPane() {
        if (!wide) return
        if (paneCompose > 0 && (composeItem === null || composeItem.compose !== paneCompose)) {
            retireCompose()
            composeItem = composePage.createObject(root, { compose: paneCompose })
            toPane(composeItem)
        }
        const open = Number(core.openMessage)
        if (open > 0 && readerItem === null) {
            readerItem = readerPage.createObject(root, { messageId: open })
            toPane(readerItem)
        } else if (readerItem !== null && readerItem.parent === pane) {
            if (open > 0) readerItem.messageId = open
            else retireReader()
        }
        // A compose that is no longer the pane's goes (whoever cleared
        // paneCompose closed it in the core).
        if (paneCompose === 0 && composeItem !== null && composeItem.parent === pane) retireCompose()
        if (composeItem !== null && composeItem.parent === pane) composeItem.visible = paneCompose > 0
        if (readerItem !== null && readerItem.parent === pane) readerItem.visible = paneCompose === 0
    }
    Connections {
        target: root.core
        function onOpenMessageChanged() { root.syncPane() }
    }
    // Narrowing: the reader that was beside the list and the compose go onto
    // the stack, the reader first so Back from the compose returns to it.
    // The state is read before `_wide` flips: the layout change moves and
    // hides the items (focus and scroll go with it).
    function toNarrow() {
        // Another page above the list (Settings, Search) covers the window:
        // the reader stays in the pane, hidden, and keeps the open message,
        // rather than being pushed over that page.
        const reader = stack.depth === 1 && readerItem !== null && readerItem.parent === pane && core.openMessage > 0 ? readerItem : null
        const compose = paneCompose > 0 && composeItem !== null && composeItem.parent === pane ? composeItem : null
        const view = compose !== null ? compose.viewState() : null
        if (reader !== null) reader.holdScroll()
        _wide = false
        paneCompose = 0
        if (reader !== null) { toStack(reader); reader.restoreScroll() }
        if (compose !== null) { toStack(compose); compose.restoreView(view) }
    }
    // Widening: when only the reader and the compose are above the list they
    // move into the pane and the stack pops to the list; with another page on
    // top they stay, as the stack's pages.
    function toWide() {
        let items = []
        let only = stack.depth >= 2 && rootKind === "inbox"
        for (let i = 1; only && i < stack.depth; i++) {
            const page = stack.get(i, StackView.DontLoad)
            if (page !== readerItem && page !== composeItem) only = false
            items.push(page)
        }
        if (!only) { _wide = true; syncPane(); return }
        const view = composeItem !== null && items.indexOf(composeItem) >= 0 ? composeItem.viewState() : null
        for (const page of items) if (page === readerItem) page.holdScroll()
        _wide = true
        stack.pop(null, StackView.Immediate)
        for (const page of items) {
            if (page === readerItem) {
                toPane(page)
                if (Number(core.openMessage) !== page.messageId) core.setOpenMessage(page.messageId)
                page.restoreScroll()
            } else {
                toPane(page)
                paneCompose = page.compose
                page.restoreView(view)
            }
        }
        syncPane()
    }

    // Compose pages: the core made the compose, the page follows it. Wide,
    // over the list (nothing on top of it), it opens in the pane.
    function composeOpened(compose) {
        const atList = stack.depth === 1 && rootKind === "inbox"
        if (wide && atList) {
            // One compose in the pane: the one that was there closes first
            // (its draft stays in Drafts), and its page goes with it.
            if (paneCompose > 0 && paneCompose !== compose && composeItem !== null) composeItem.leave()
            paneCompose = compose
        } else if (atList && composeItem === null) {
            composeItem = composePage.createObject(root, { compose: compose })
            toStack(composeItem)
        } else stack.push(composePage, { compose: compose })
    }
    // A tap on a row: the pane when wide, else the reader page. With a compose
    // in the pane the compose closes first, as `leave()` does, and its draft
    // stays in Drafts.
    function openMessageFromRow(message) {
        if (!wide) {
            if (stack.depth === 1 && readerItem === null) {
                readerItem = readerPage.createObject(root, { messageId: message })
                readerItem.wide = false
                readerItem.visible = true
                fit(readerItem, stack)
                stack.push(readerItem)
            } else stack.push(readerPage, { messageId: message })
        } else if (paneCompose > 0 && composeItem !== null) {
            // A message on its way out is in the Outbox: closing it here
            // would put a second copy in Drafts. Leave the click alone.
            if (composeItem.sending) return
            // leave() hands the core the fields (a typed address included)
            // and the core's close saves the draft at once.
            composeItem.leave()
            core.setOpenMessage(message)
            sentToast.show(qsTr("Draft saved"))
        } else core.setOpenMessage(message)
    }
    // Back (or a message that is gone) on a reader page.
    function readerBack(page) {
        if (page !== readerItem) { stack.pop(); return }
        if (page.parent === pane) core.setOpenMessage(0)
        else {
            stack.pop()
            retireReader()
            if (core.openMessage > 0) core.setOpenMessage(0)
        }
    }
    // The compose page is done (Close, Discard, Send): in the pane the pane
    // empties, on the stack the page pops. `sentToast` is the caller's.
    function composeDone(page, text) {
        if (page === composeItem && page.parent === pane) {
            paneCompose = 0
            retireCompose()
        } else if (stack.currentItem === page) {
            stack.pop()
            if (page === composeItem) retireCompose()
        } else if (text !== undefined) page.done = true
        else {
            stack.pop()
            if (page === composeItem) retireCompose()
        }
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
    // `enviooauth:` link; it waits until the core is up, then goes to it. The
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
        schemes: ["enviooauth"]
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
            anchors { top: parent.top; bottom: parent.bottom; left: parent.left; leftMargin: inlineDrawer.visible ? inlineDrawer.width : 0 }
            width: pane.visible ? root.theme.gu(50) : parent.width - anchors.leftMargin
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

        // The folders drawer as a third column on very wide windows, beside
        // the inbox at the root only (full-window pages cover it, as they
        // cover the pane). The overlay drawer of the inbox page stays closed.
        FoldersDrawer {
            id: inlineDrawer
            objectName: "inlineDrawer"
            inline: true
            visible: root.veryWide && stack.depth === 1 && root.rootKind === "inbox" && root.inboxItem !== null
            anchors { top: parent.top; bottom: parent.bottom; left: parent.left }
            width: root.theme.gu(30)
            theme: root.theme
            core: root.core
            accounts: root.accounts
            folders: root.folders
            outboxCount: root.inboxItem !== null ? root.inboxItem.outboxCount : 0
            unifiedSelected: root.inboxItem !== null && root.inboxItem.unified
            selectedFolder: root.inboxItem !== null ? root.inboxItem.folderId : -1
            onUnifiedRequested: if (root.inboxItem) root.inboxItem.showUnified()
            onFolderRequested: function(folder, label) { if (root.inboxItem) root.inboxItem.showFolder(folder, label) }
            onOutboxRequested: stack.push(outboxPage)
            onAccountsRequested: stack.push(accountsPage)
            onSettingsRequested: stack.push(settingsPage)
            Rectangle {
                anchors { top: parent.top; bottom: parent.bottom; right: parent.right }
                width: 1
                color: root.theme.hairline
            }
        }

        // The hairline between the list and the pane.
        Rectangle {
            objectName: "paneHairline"
            visible: pane.visible
            x: root.drawerWidth + root.theme.gu(50)
            width: 1
            height: parent.height
            color: root.theme.hairline
            z: 1
        }
        // The right pane of the wide layout, beside the inbox at the root.
        // Pages above the inbox (Search, Outbox, Settings, Accounts) cover
        // the whole window, so the pane is not shown over them. It holds the
        // reader and the compose (moved in and out, see `toPane`) or the
        // empty state.
        Item {
            id: pane
            objectName: "pane"
            visible: root.wide && stack.depth === 1 && root.rootKind === "inbox"
            anchors { top: parent.top; bottom: parent.bottom; right: parent.right }
            width: root.paneWidth
            // Opaque: the pages are transparent, so without it the list
            // shows through.
            Rectangle {
                objectName: "paneBackground"
                anchors.fill: parent
                z: -1
                color: root.theme.ground
            }
            Loader {
                anchors.fill: parent
                active: root.wide && root.paneCompose === 0 && Number(root.core.openMessage) === 0
                sourceComponent: emptyC
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
    }

    Component { id: emptyC; ReaderPane { theme: root.theme } }
    Component {
        id: inboxPage
        InboxPage {
            id: inboxRoot
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
            showOpen: root.paneCompose === 0
            drawerInline: root.veryWide
            Component.onCompleted: root.inboxItem = inboxRoot
            Component.onDestruction: if (root.inboxItem === inboxRoot) root.inboxItem = null
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
            id: reader
            theme: root.theme
            core: root.core
            accounts: root.accounts
            folders: root.folders
            onBackRequested: root.readerBack(reader)
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
                    Qt.callLater(function() { if (stack.currentItem === page) root.composeDone(page) })
            }
            theme: root.theme
            core: root.core
            accounts: root.accounts
            suggestions: root.suggestions
            onClosed: { root.dropShare(compose); root.composeDone(page) }
            onAttachStarted: function(count, transfer) { shareIn.expectPick(compose, count, transfer) }
            // Queued arrives late: close this page, not whatever is on top
            // by now. A page below the top closes when it is shown again.
            onSent: function(text) {
                root.composeDone(page, text)
                sentToast.show(text)
            }
        }
    }

    // Ctrl+N: a new message, from the list. Wide, the ShortcutLayer has it.
    readonly property alias newShortcut: newKey
    Shortcut {
        id: newKey
        sequence: "Ctrl+N"
        enabled: !root.wide && root.core.ready && root.rootKind === "inbox" && stack.depth === 1 && root.paneCompose === 0
        onActivated: root.newMessage()
    }

    // The wide layout's keys (frame 7). Not over a page (Search, Settings,
    // Outbox) and not under the shortcuts dialog.
    readonly property alias shortcuts: keys
    readonly property alias shortcutsDialog: help
    ShortcutLayer {
        id: keys
        active: root.wide && root.core.ready && root.rootKind === "inbox" && stack.depth === 1 && root.theme.popupsUp === 0
        onMoveFocus: function(delta) { root.core.stepFocus(delta) }
        onOpenFocusedRequested: {
            const id = Number(root.core.openFocused())
            const draft = String(root.core.draftIdOf(id))
            if (draft !== "") root.openDraft(draft)
            else if (id > 0) root.openMessageFromRow(id)
        }
        // The open message, not the checked rows; nothing while a compose
        // fills the pane (no message is shown then).
        onDeleteRequested: {
            const id = Number(root.core.openMessage)
            if (root.paneCompose === 0 && id > 0) root.core.actOn([id], "delete", 0)
        }
        onNewRequested: if (root.paneCompose === 0) root.newMessage()
        onReplyRequested: {
            const r = root.readerItem
            if (root.paneCompose === 0 && r !== null && r.parent === pane && r.view !== null && !r.isDraft) r.compose("reply")
        }
        // Compose asks first (Discard or Save draft); else the reader closes.
        onCloseRequested: {
            if (root.paneCompose > 0 && root.composeItem !== null) root.composeItem.askClose()
            else if (Number(root.core.openMessage) > 0) root.core.setOpenMessage(0)
        }
        onHelpRequested: help.open()
    }
    ShortcutsDialog { id: help; theme: root.theme }

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
