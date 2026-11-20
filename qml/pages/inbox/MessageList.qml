import QtQuick 2.15
import "../../platform"
import "../../components"

// The list over `core.messages`. Rust owns the rows and the selection; this
// reads the `selected` role to know whether selection mode is on. Near the
// end it asks for the next page (the model merges it in, so the list never
// resets under the finger); pulling past the top refreshes. At the end the
// footer offers older mail from the server.
Item {
    id: root
    required property Theme theme
    required property var core
    required property var messages
    property bool showAccount: false
    // Whether the open row is marked (not while a compose is in the wide pane).
    property bool showOpen: true

    // What is selected now, from the rows' `selected` role.
    property int selectedCount: 0
    property var selectedAccounts: []
    readonly property alias view: list
    signal messageRequested(var message)
    // A local draft's row (`opens` is "compose"): Compose opens at once.
    signal draftRequested(string draftId)

    // The row menu's Move to folder: the message and its account.
    signal moveRequested(int id, string account)

    function positionViewAtEnd() { list.positionViewAtEnd() }

    function recount() {
        let n = 0
        const accounts = []
        for (let i = 0; i < probe.count; i++) {
            const it = probe.itemAt(i)
            if (it === null || !it.selected) continue
            n++
            if (accounts.indexOf(it.account) < 0) accounts.push(it.account)
        }
        selectedCount = n
        selectedAccounts = accounts
    }

    // One tiny object per row, reporting its `selected` and `account` to
    // `recount`. (A ListView builds only the rows in view.)
    Repeater {
        id: probe
        model: root.messages
        delegate: Item {
            visible: false
            required property var model
            readonly property bool selected: model.selected
            readonly property string account: model.account
            onSelectedChanged: root.recount()
        }
        onItemAdded: root.recount()
        onItemRemoved: root.recount()
    }

    ListView {
        id: list
        objectName: "messageList"
        anchors.fill: parent
        clip: true
        model: root.messages
        boundsBehavior: Flickable.DragOverBounds
        // The end of the list asks for the next cached page and, once the
        // list is at the end of the cache, for older mail from the server
        // (the core asks for one batch only until rows arrive).
        // The check runs after layout (Qt.callLater), so it reads the
        // state the new rows or footer produced, not the one before.
        function atEnd() { return count > 0 && atYEnd }
        function nearEnd() { return count > 0 && contentHeight - (contentY + height) < root.theme.touch * 2 }
        function askAtEnd() {
            if (!atEnd()) return
            root.core.more()
            root.core.loadOlderNearEnd()
        }
        function askOlder() { if (nearEnd()) root.core.loadOlderNearEnd() }
        onAtYEndChanged: if (atYEnd) Qt.callLater(askAtEnd)
        // The list follows the keyboard focus, also onto a row with no
        // delegate yet; a delegate made for the focused row does not scroll.
        Connections {
            target: root.core
            function onFocusMessageChanged() {
                const at = Number(root.core.messageIndex(Number(root.core.focusMessage)))
                if (at >= 0) list.positionViewAtIndex(at, ListView.Contain)
            }
        }
        Connections {
            target: root.core
            // The row turns Ready under a list already at its end (the footer
            // grows the list, so it is no longer at the very end).
            function onOlderRowChanged() { Qt.callLater(list.askOlder) }
        }
        onDragEnded: if (contentY < -root.theme.gu(10)) root.core.refresh("")
        footer: LoadOlderRow {
            theme: root.theme
            width: list.width
            state: root.core.olderRow
            onLoadRequested: root.core.loadOlder()
        }
        delegate: MessageRow {
            id: row
            theme: root.theme
            width: list.width
            selecting: root.selectedCount > 0
            showAccount: root.showAccount
            showOpen: root.showOpen
            onClicked: {
                if (root.selectedCount > 0) root.core.select(row.model.id, !row.model.selected)
                else if (row.model.opens === "compose") root.draftRequested(row.model.draftId)
                else root.messageRequested(row.model.id)
            }
            onContextRequested: function(id, px, py) {
                // A local draft has no server copy to act on.
                if (row.model.opens === "compose") return
                const at = row.mapToItem(rowMenu.parent, px, py)
                rowMenu.messageId = id
                rowMenu.unread = row.model.unread === true
                rowMenu.account = row.model.account
                rowMenu.openAt(at.x, at.y)
            }
            onPressAndHold: if (!row.model.selected) root.core.select(row.model.id, true)
            Component.onCompleted: if (row.index > list.count - 10) root.core.more()
        }
    }

    RowMenu {
        id: rowMenu
        theme: root.theme
        onSelectRequested: root.core.select(messageId, true)
        onMarkRequested: function(read) { root.core.actOn([messageId], read ? "read" : "unread", 0) }
        onMoveRequested: root.moveRequested(messageId, account)
        onArchiveRequested: root.core.actOn([messageId], "archive", 0)
        onDeleteRequested: root.core.actOn([messageId], "delete", 0)
    }
}
