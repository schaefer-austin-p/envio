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

    // What is selected now, from the rows' `selected` role.
    property int selectedCount: 0
    property var selectedAccounts: []
    readonly property alias view: list
    signal messageRequested(var message)

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
        onAtYEndChanged: if (atYEnd && count > 0) root.core.more()
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
            onClicked: {
                if (root.selectedCount > 0) root.core.select(row.model.id, !row.model.selected)
                else root.messageRequested(row.model.id)
            }
            onPressAndHold: if (!row.model.selected) root.core.select(row.model.id, true)
            Component.onCompleted: if (row.index > list.count - 10) root.core.more()
        }
    }
}
