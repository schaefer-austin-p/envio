import QtQuick 2.15
import "../../platform"
import "../../components"

// Selection mode's top bar (board 9): Back leaves, then the count (counted
// text from Rust), Archive, Delete, Move, and a menu for read/unread and
// flag. Each action goes to `core.act` on the selection, except Move, which
// first asks for a folder (`moveRequested`).
TopBar {
    id: root
    objectName: "selectionBar"
    required property var core
    property int count: 0
    // Move needs one account: the server cannot move across accounts.
    property bool canMove: true
    signal leaveRequested()
    signal moveRequested()

    selecting: true
    showBack: true
    title: root.core.countText("selected", root.count)
    onBackClicked: root.leaveRequested()

    IconButton {
        objectName: "act-archive"
        theme: root.theme
        iconName: "archive"
        text: qsTr("Archive")
        onClicked: root.core.act("archive", 0)
    }
    IconButton {
        objectName: "act-delete"
        theme: root.theme
        iconName: "trash"
        text: qsTr("Delete")
        onClicked: root.core.act("delete", 0)
    }
    IconButton {
        objectName: "act-move"
        theme: root.theme
        iconName: "move"
        text: qsTr("Move to folder")
        enabled: root.canMove
        onClicked: root.moveRequested()
    }
    IconButton {
        objectName: "act-more"
        theme: root.theme
        iconName: "more"
        text: qsTr("More actions")
        onClicked: more.open()
    }

    Sheet {
        id: more
        objectName: "moreSheet"
        theme: root.theme
        Repeater {
            model: [
                { action: "read", label: qsTr("Mark as read") },
                { action: "unread", label: qsTr("Mark as unread") },
                { action: "flag", label: qsTr("Flag") },
                { action: "unflag", label: qsTr("Remove flag") }
            ]
            delegate: ListRow {
                id: item
                required property var modelData
                objectName: "more-" + modelData.action
                theme: root.theme
                width: more.width
                onClicked: {
                    more.close()
                    root.core.act(item.modelData.action, 0)
                }
                Text {
                    x: root.theme.gu(2.5)
                    y: root.theme.gu(1.75)
                    text: item.modelData.label
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 16
                }
            }
        }
    }
}
