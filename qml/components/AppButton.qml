import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A full-width-capable text button: `primary` is filled (primary role),
// otherwise outlined (1 px primary). At least 44 px high.
AbstractButton {
    id: root
    required property Theme theme
    property bool primary: false
    // Optional leading icon name (see Icon.qml).
    property string iconName: ""
    // The drawn pill's height; the tap target stays implicitHeight.
    property real visibleHeight: theme.controlHeight

    implicitHeight: theme.controlHeight
    implicitWidth: Math.max(theme.gu(6), row.implicitWidth + theme.gu(4))
    opacity: enabled ? 1 : 0.45

    // The Control sizes and places its contentItem itself, so the centred
    // Row lives inside an Item that fills the button.
    contentItem: Item {
        implicitWidth: row.implicitWidth
        implicitHeight: row.implicitHeight
    Row {
        id: row
        spacing: root.theme.gu(1)
        anchors.centerIn: parent
        Icon {
            visible: root.iconName !== ""
            theme: root.theme
            name: root.iconName
            size: root.theme.iconButton
            color: label.color
            anchors.verticalCenter: parent.verticalCenter
        }
        Text {
            textFormat: Text.PlainText
            id: label
            objectName: "buttonLabel"
            text: root.text
            color: root.primary ? root.theme.primaryText : root.theme.primary
            font: root.theme.controlFont
            anchors.verticalCenter: parent.verticalCenter
        }
    }
    }

    background: Rectangle {
        height: root.visibleHeight
        y: (parent.height - height) / 2
        radius: root.visibleHeight < root.theme.controlHeight ? root.theme.compactControlRadius : root.theme.controlRadius
        color: root.primary ? root.theme.primary : (root.down ? root.theme.selected : "transparent")
        border.width: root.primary ? 0 : 1
        border.color: root.theme.primary
    }
}
