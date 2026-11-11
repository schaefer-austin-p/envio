import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A 48 px square icon button (top bars). `text` is the accessible name.
AbstractButton {
    id: root
    required property Theme theme
    property string iconName: ""

    implicitWidth: theme.touch
    implicitHeight: theme.touch
    opacity: enabled ? 1 : 0.45
    Accessible.role: Accessible.Button
    Accessible.name: text

    contentItem: Icon {
        theme: root.theme
        name: root.iconName
        color: root.theme.text
        anchors.centerIn: parent
    }
    background: Rectangle {
        radius: width / 2
        color: root.down ? root.theme.selected : "transparent"
    }
}
