import QtQuick 2.15
import "../platform"

// One key of a shortcut as a small outlined mono chip (two-pane board,
// frame 7): "Ctrl" and "N" are two chips.
Rectangle {
    id: root
    objectName: "keyChip"
    required property Theme theme
    property string text: ""

    implicitWidth: Math.max(implicitHeight, label.implicitWidth + theme.gu(1.5))
    implicitHeight: label.implicitHeight + theme.gu(0.75)
    width: implicitWidth
    height: implicitHeight
    radius: theme.chipRadius
    color: theme.surface
    border.color: theme.hairline

    Text {
        id: label
        anchors.centerIn: parent
        textFormat: Text.PlainText
        text: root.text
        color: root.theme.text
        font.family: root.theme.fontLabel
        font.pixelSize: 12
    }
}
