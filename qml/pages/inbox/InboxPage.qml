import QtQuick 2.15
import "../../platform"
import "../../components"

// The unified inbox (board 1) and any other folder's list: top bar (or the
// selection bar), account tabs, banners, the message list, "New message",
// the Undo and notice toasts, the folders menu and the move sheet. All the
// state that matters lives in Rust; this page shows rows and forwards taps.
Item {
    id: root
    objectName: "inboxPage"
    required property Theme theme
    required property var core
    // The models Main attached (roles: crates/envio-ui/src/rows.rs).
    required property var accounts
    required property var folders
    required property var messages
    required property var outbox
    // "" is the unified inbox, else an account key (its Inbox) or "folder".
    property string tab: ""
    property string title: qsTr("Inbox")
    property bool unified: true
    // Whether the row of the open message is marked; the wide shell turns it off while a compose is in the pane.
    property bool showOpen: true
    // The account whose Inbox the next folder list should open.
    property string pendingInbox: ""
    property int undoId: 0

    signal messageRequested(var message)
    signal draftRequested(string draftId)
    signal composeRequested()
    // The folder the list shows, -1 for a unified or not yet opened one.
    property int folderId: -1
    signal searchRequested(int folder)
    signal accountsRequested()
    signal settingsRequested()
    signal outboxRequested()

    readonly property int outboxCount: outboxRows.count
    // Messages still to go out: sent-but-not-filed ones are not.
    readonly property int waitingCount: waitingProbe.toSend
    readonly property bool selecting: list.selectedCount > 0

    function showUnified() {
        tab = ""
        title = qsTr("Inbox")
        unified = true
        folderId = -1
        pendingInbox = ""
        core.openList("inboxes", 0)
    }
    function showFolder(id, name) {
        tab = "folder"
        title = name
        unified = false
        folderId = id
        pendingInbox = ""
        core.openList("folder", id)
    }
    // The Inbox's id is among the account's folders: ask, then open it
    // when they arrive.
    function showAccount(key) {
        tab = key
        title = qsTr("Inbox")
        unified = false
        folderId = -1
        pendingInbox = key
        core.listFolders(key)
    }

    Repeater {
        id: outboxRows
        model: root.outbox
        delegate: Item { visible: false }
    }
    Repeater {
        id: folderWatch
        model: root.folders
        delegate: Item {
            visible: false
            required property var model
        }
        onItemAdded: {
            if (root.pendingInbox === "") return
            if (item.model.role !== "inbox" || item.model.account !== root.pendingInbox) return
            root.pendingInbox = ""
            root.folderId = item.model.id
            root.core.openList("folder", item.model.id)
        }
    }
    // A Folders reply that has no Inbox row for the account the tab waits on:
    // stop waiting, so a later listFolders (the drawer's, the move sheet's)
    // never switches the list.
    Connections {
        target: root.folders
        function onModelReset() { Qt.callLater(root.stopWaiting) }
        function onRowsInserted() { Qt.callLater(root.stopWaiting) }
    }
    function stopWaiting() { pendingInbox = "" }
    // The account whose Inbox tab is shown was removed: back to the
    // unified inbox, which loads its list.
    Repeater {
        model: root.accounts
        delegate: Item {
            visible: false
            required property var model
            readonly property string key: model.key
        }
        onItemRemoved: (index, item) => { if (root.tab !== "" && root.tab !== "folder" && item.key === root.tab) root.showUnified() }
    }
    Repeater {
        id: waitingProbe
        property int toSend: 0
        function recount() {
            let n = 0
            for (let i = 0; i < count; i++) if (itemAt(i) !== null && itemAt(i).isWaiting) n++
            toSend = n
        }
        model: root.outbox
        delegate: Item {
            visible: false
            required property var model
            readonly property bool isWaiting: model.status === "waiting" || model.status === "sending"
            onIsWaitingChanged: waitingProbe.recount()
        }
        onItemAdded: Qt.callLater(recount)
        onItemRemoved: Qt.callLater(recount)
    }
    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        visible: !root.selecting
        title: root.title
        IconButton {
            objectName: "searchButton"
            theme: root.theme
            iconName: "search"
            text: qsTr("Search")
            onClicked: root.searchRequested(root.folderId)
        }
        IconButton {
            objectName: "menuButton"
            theme: root.theme
            iconName: "menu"
            text: qsTr("Open menu")
            onClicked: { drawer.visible = true }
        }
    }
    SelectionBar {
        id: selection
        theme: root.theme
        core: root.core
        width: parent.width
        visible: root.selecting
        count: list.selectedCount
        canMove: list.selectedAccounts.length === 1
        onLeaveRequested: root.core.clearSelection()
        onMoveRequested: move.openFor(list.selectedAccounts[0])
    }

    AccountTabs {
        id: tabs
        theme: root.theme
        anchors.top: bar.bottom
        width: parent.width
        accounts: root.accounts
        current: root.tab === "folder" ? "-" : root.tab
        onChosen: key === "" ? root.showUnified() : root.showAccount(key)
        onAttentionRequested: root.accountsRequested()
    }
    Column {
        id: banners
        anchors.top: tabs.bottom
        width: parent.width
        Banner {
            id: waiting
            objectName: "outboxBanner"
            theme: root.theme
            width: parent.width
            visible: root.waitingCount > 0
            height: visible ? implicitHeight : 0
            text: root.core.countText("waiting", root.waitingCount)
            MouseArea { anchors.fill: parent; onClicked: root.outboxRequested() }
        }
    }

    MessageList {
        id: list
        theme: root.theme
        core: root.core
        messages: root.messages
        showAccount: root.unified
        showOpen: root.showOpen
        anchors { top: banners.bottom; bottom: footer.top; left: parent.left; right: parent.right }
        onMessageRequested: root.messageRequested(message)
        onDraftRequested: function(draftId) { root.draftRequested(draftId) }
    }

    PageFooter {
        id: footer
        theme: root.theme
        AppButton {
            id: newMessage
            objectName: "newMessage"
            theme: root.theme
            width: parent.width
            primary: true
            iconName: "pencil"
            text: qsTr("New message")
            onClicked: root.composeRequested()
        }
    }

    Toast {
        id: undoToast
        objectName: "undoToast"
        theme: root.theme
        anchors { left: parent.left; right: parent.right; bottom: footer.top; margins: root.theme.gu(2) }
        onActionTriggered: root.core.undo(root.undoId)
    }
    Toast {
        id: noticeToast
        objectName: "noticeToast"
        theme: root.theme
        anchors { left: parent.left; right: parent.right; bottom: footer.top; margins: root.theme.gu(2) }
    }

    FoldersDrawer {
        id: drawer
        anchors.fill: parent
        visible: false
        theme: root.theme
        core: root.core
        accounts: root.accounts
        folders: root.folders
        outboxCount: root.outboxCount
        onCloseRequested: visible = false
        onUnifiedRequested: { visible = false; root.showUnified() }
        onFolderRequested: { visible = false; root.showFolder(folder, label) }
        onOutboxRequested: { visible = false; root.outboxRequested() }
        onAccountsRequested: { visible = false; root.accountsRequested() }
        onSettingsRequested: { visible = false; root.settingsRequested() }
    }
    MoveSheet {
        id: move
        theme: root.theme
        core: root.core
        folders: root.folders
    }

    Connections {
        target: root.core
        function onUndoOffer(undo, text, scope) {
            // An invite answer's offer belongs to the reader.
            if (scope === "reader") return
            root.undoId = undo
            undoToast.show(text, qsTr("Undo"))
        }
        function onUndoClosed(undo) { if (undo === root.undoId) undoToast.hide() }
        function onNotice(text) { noticeToast.show(text) }
    }
}
