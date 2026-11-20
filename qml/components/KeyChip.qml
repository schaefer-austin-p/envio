import QtQuick 2.15
import "../platform"

// One key of a shortcut as a small outlined mono chip (two-pane board,
// frame 7): "Ctrl" and "N" are two chips.
Rectangle {
    id: root
    objectName: "keyChip"
    required property Theme theme
    property string text: ""

    implicitWidth: Math.max(implicitHeight, label.implicitWidth + theme.gu(2))
    implicitHeight: label.implicitHeight + theme.gu(0.75) + 2
    width: implicitWidth
    height: implicitHeight
    radius: theme.chipRadius
    color: theme.surface
    border.color: theme.hairline

    // The keycap's raised bottom edge, darker than the hairline.
    Rectangle {
        objectName: "keyEdge"
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        anchors.leftMargin: root.radius / 2
        anchors.rightMargin: root.radius / 2
        height: 2
        color: Qt.darker(root.theme.hairline, 1.8)
    }

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
