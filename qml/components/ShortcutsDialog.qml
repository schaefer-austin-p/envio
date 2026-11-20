import QtQuick 2.15
import "../platform"

// The keyboard shortcuts (two-pane board, frame 7): six rows, each a
// sentence and its key chips. Key names are translated.
AppDialog {
    id: root
    objectName: "shortcutsDialog"
    title: qsTr("Keyboard shortcuts")
    secondaryText: qsTr("Close")

    // Each row: what it does and the chips of its keys.
    readonly property var rows: [
        { text: qsTr("Move the selection"), keys: ["↑", "↓"] },
        { text: qsTr("Open the message"), keys: [qsTr("Enter")] },
        { text: qsTr("Delete"), keys: [qsTr("Del")] },
        { text: qsTr("New message"), keys: [qsTr("Ctrl"), "N"] },
        { text: qsTr("Reply"), keys: [qsTr("Ctrl"), "R"] },
        { text: qsTr("Close the reader or compose"), keys: [qsTr("Esc")] }
    ]

    Repeater {
        model: root.rows
        delegate: Item {
            id: line
            required property var modelData
            objectName: "shortcutRow"
            width: parent.width
            height: Math.max(label.implicitHeight, chips.height) + root.theme.gu(0.5)
            Text {
                id: label
                anchors { left: parent.left; right: chips.left; rightMargin: root.theme.gu(1); verticalCenter: parent.verticalCenter }
                textFormat: Text.PlainText
                text: line.modelData.text
                wrapMode: Text.Wrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 15
            }
            Row {
                id: chips
                anchors { right: parent.right; verticalCenter: parent.verticalCenter }
                spacing: root.theme.gu(0.5)
                Repeater {
                    model: line.modelData.keys
                    delegate: KeyChip { required property string modelData; theme: root.theme; text: modelData }
                }
            }
        }
    }
}
