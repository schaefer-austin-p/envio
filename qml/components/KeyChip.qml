import QtQuick 2.15
import "../platform"

// One key of a shortcut drawn as a keycap (#434, variant B on the two-pane
// board, frame 7): "Ctrl" and "N" are two chips. The root is the skirt, a
// darker base that shows 3 px below the face; the face has a vertical
// gradient, a hairline border and a 1 px lighter top edge. Plain Rectangles,
// no shaders. Light skirts derive from hairline, dark ones from ground (the
// dark hairline is barely darker than the surface).
Rectangle {
    id: root
    objectName: "keyChip"
    required property Theme theme
    property string text: ""

    readonly property int skirt: 3

    implicitWidth: Math.max(implicitHeight, label.implicitWidth + 14)
    implicitHeight: face.height + skirt
    width: implicitWidth
    height: implicitHeight
    radius: theme.chipRadius
    color: theme.dark ? Qt.darker(theme.ground, 1.55) : Qt.darker(theme.hairline, 1.6)

    Rectangle {
        id: face
        objectName: "keyFace"
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: Math.max(22, label.implicitHeight + 6)
        radius: root.theme.chipRadius
        border.color: root.theme.hairline
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.lighter(root.theme.surface, 1.10) }
            GradientStop { position: 1; color: Qt.darker(root.theme.surface, root.theme.dark ? 1.08 : 1.05) }
        }

        // The lighter 1 px top edge.
        Rectangle {
            objectName: "keyTop"
            anchors { left: parent.left; right: parent.right; top: parent.top }
            anchors.leftMargin: root.radius / 2
            anchors.rightMargin: root.radius / 2
            anchors.topMargin: 1
            height: 1
            color: Qt.rgba(1, 1, 1, root.theme.dark ? 0.14 : 0.9)
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
}
