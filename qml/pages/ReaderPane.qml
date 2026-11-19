import QtQuick 2.15
import "../platform"
import "../components"

// The right pane when nothing is open (two-pane board, frame 4): a large
// envelope in the hairline colour, a sentence, and the new-message shortcutHint with
// its key chip.
Item {
    id: root
    objectName: "emptyReader"
    required property Theme theme
    // The key that writes a new message, as the chip shows it.
    property string shortcutKey: qsTr("Ctrl+N")
    // The sentence under the title, with the key spelled out.
    readonly property string shortcutHint: qsTr("Or press %1 to write one").arg(shortcutKey)

    Rectangle { anchors.fill: parent; color: root.theme.ground }

    Column {
        anchors.centerIn: parent
        spacing: root.theme.gu(2)
        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            theme: root.theme
            name: "mail"
            size: root.theme.gu(8)
            color: root.theme.hairline
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            textFormat: Text.PlainText
            text: qsTr("Select a message to read it")
            color: root.theme.heading
            font: root.theme.barTitleFont
        }
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: root.theme.gu(0.75)
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.shortcutHint.split(root.shortcutKey)[0]
                color: root.theme.secondary
                font.family: root.theme.fontBody
                font.pixelSize: root.theme.bodySize
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: chip.width + root.theme.gu(1.5)
                height: chip.height + root.theme.gu(0.75)
                radius: root.theme.chipRadius
                color: root.theme.surface
                border.color: root.theme.hairline
                Text {
                    id: chip
                    anchors.centerIn: parent
                    textFormat: Text.PlainText
                    text: root.shortcutKey
                    color: root.theme.text
                    font.family: root.theme.fontBody
                    font.pixelSize: root.theme.bodySize
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                text: root.shortcutHint.split(root.shortcutKey).slice(1).join(root.shortcutKey)
                color: root.theme.secondary
                font.family: root.theme.fontBody
                font.pixelSize: root.theme.bodySize
            }
        }
    }
}
