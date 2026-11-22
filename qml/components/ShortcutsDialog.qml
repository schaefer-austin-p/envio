import QtQuick 2.15
import "../platform"

// The keyboard shortcuts (two-pane board, frame 7): six rows, each a
// sentence and its key chips. Key names are translated.
AppDialog {
    id: root
    objectName: "shortcutsDialog"
    title: qsTr("Keyboard shortcuts")
    secondaryText: qsTr("Close")
    compactActions: true
    // A narrow card, centred (the mockup's ~340 px of content).
    width: parent ? Math.min(parent.width - 2 * theme.frameInset - theme.gu(4), theme.gu(45)) : 0
    x: parent ? (parent.width - width) / 2 : 0
    // Board frame 7: 22 px at the sides, 18 px above the title.
    leftPadding: theme.gu(2.75)
    rightPadding: theme.gu(2.75)
    topPadding: theme.gu(2.25)
    bottomPadding: 10
    // The title is 16 px, 6 px above the first row; the rows touch (their
    // own padding spaces them) and 12 px (6 here, 6 in the spacer) lie
    // above Close, a 40 px button with 20 px sides, 14 px mono, in Carbon.
    titleSize: 16
    titleWeight: Font.DemiBold
    contentSpacing: theme.gu(0.75)
    extraSpacing: 0
    actionHeight: theme.airmail ? theme.controlHeight : 40
    actionPadding: theme.airmail ? theme.gu(2) : 20
    actionFont: Qt.font({ "family": theme.fontLabel, "pixelSize": 14, "weight": theme.controlFont.weight })

    // The board's card: the surface colour (white in Carbon light), a
    // field-coloured border and the card radius (8 px in Carbon, 10 in Airmail).
    background: Rectangle {
        objectName: "shortcutCard"
        color: root.theme.surface
        radius: root.theme.cardRadius
        border.width: 1
        border.color: root.theme.field
    }

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
            // 11 px above and below the text, then the 1 px rule.
            height: Math.max(label.implicitHeight, chips.height) + root.theme.gu(2.75) + 1
            Rectangle {
                objectName: "shortcutRule"
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                height: 1
                color: root.theme.hairline
            }
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
                spacing: root.theme.gu(0.75)
                Repeater {
                    model: line.modelData.keys
                    delegate: KeyChip { required property string modelData; theme: root.theme; text: modelData }
                }
            }
        }
    }
    // With the 6 px content spacing, 12 px lie between the last rule and Close.
    Item { objectName: "shortcutsGap"; width: 1; height: root.theme.gu(0.75) }
}
