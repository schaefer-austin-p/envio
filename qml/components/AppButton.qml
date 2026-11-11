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

    implicitHeight: theme.gu(6.5)
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
            size: root.theme.iconSmall
            color: label.color
            anchors.verticalCenter: parent.verticalCenter
        }
        Text {
            textFormat: Text.PlainText
            id: label
            objectName: "buttonLabel"
            text: root.text
            color: root.primary ? root.theme.primaryText : root.theme.primary
            font.family: root.theme.airmail ? root.theme.fontUi : root.theme.fontMono
            font.pixelSize: 15
            font.weight: Font.Medium
            anchors.verticalCenter: parent.verticalCenter
        }
    }
    }

    background: Rectangle {
        radius: root.theme.airmail ? root.theme.gu(1.5) : root.theme.gu(1)
        color: root.primary ? root.theme.primary : (root.down ? root.theme.selected : "transparent")
        border.width: root.primary ? 0 : 1
        border.color: root.theme.primary
    }
}
