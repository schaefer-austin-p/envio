import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A switch: 52 x 30, on = primary fill. `checked`/`toggled` come from
// AbstractButton; `text` is the accessible name.
AbstractButton {
    id: root
    required property Theme theme

    checkable: true
    implicitWidth: theme.gu(6.5)
    implicitHeight: theme.touch
    opacity: enabled ? 1 : 0.45
    Accessible.role: Accessible.CheckBox
    Accessible.name: text
    Accessible.checked: checked

    contentItem: Item {}
    background: Item {
        Rectangle {
            anchors.centerIn: parent
            width: root.theme.gu(6.5)
            height: root.theme.gu(3.75)
            radius: height / 2
            color: root.checked ? root.theme.primary : "transparent"
            border.width: root.checked ? 0 : 1.5
            border.color: root.theme.field
            Rectangle {
                width: root.checked ? parent.height - 6 : parent.height - 9
                height: width
                radius: width / 2
                anchors.verticalCenter: parent.verticalCenter
                x: root.checked ? parent.width - width - 3 : 4.5
                color: root.checked ? root.theme.primaryText : root.theme.secondary
            }
        }
    }
}
