import QtQuick 2.15
import "../platform"
import "../components"

// The right pane when nothing is open (two-pane board, frames 4 and 7): an
// open envelope in the hairline colour, a sentence, and the new-message
// hint with its key chips.
Item {
    id: root
    objectName: "emptyReader"
    required property Theme theme
    // The hint with %1 where the key chips go (the chips are drawn here).
    readonly property string shortcutHint: qsTr("Or press %1 to write one.")
    readonly property var hintParts: shortcutHint.split("%1")

    Rectangle { anchors.fill: parent; color: root.theme.ground }

    Column {
        anchors.centerIn: parent
        spacing: root.theme.gu(2)
        Icon {
            objectName: "emptyIcon"
            anchors.horizontalCenter: parent.horizontalCenter
            theme: root.theme
            name: "mailopen"
            size: root.theme.gu(8)
            color: root.theme.hairline
        }
        Text {
            objectName: "emptyTitle"
            anchors.horizontalCenter: parent.horizontalCenter
            textFormat: Text.PlainText
            text: qsTr("Select a message to read it")
            color: root.theme.heading
            font.family: root.theme.fontUi
            font.pixelSize: 18
        }
        // The board's line: 14 px text, the chips set between plain spaces
        // (the spaces are in the translated sentence), 4 px between the chips.
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 0
            Text {
                objectName: "emptyHintBefore"
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.hintParts[0]
                color: root.theme.secondary
                font.family: root.theme.fontBody
                font.pixelSize: 14
            }
            Row {
                objectName: "emptyHintChips"
                anchors.verticalCenter: parent.verticalCenter
                spacing: root.theme.gu(0.5)
                KeyChip { theme: root.theme; text: qsTr("Ctrl") }
                KeyChip { theme: root.theme; text: "N" }
            }
            Text {
                objectName: "emptyHintAfter"
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.hintParts.slice(1).join("%1")
                color: root.theme.secondary
                font.family: root.theme.fontBody
                font.pixelSize: 14
            }
        }
    }
}
