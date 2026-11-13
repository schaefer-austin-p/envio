import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// An account tab or filter chip. Carbon: outlined, filled when `selected`.
// Airmail: underlined tab. 44 px high. `iconName` puts an icon (in
// `iconColor`) left of the label.
AbstractButton {
    id: root
    required property Theme theme
    property bool selected: false
    property string iconName: ""
    property color iconColor: theme.text
    readonly property real iconSpace: iconName !== "" ? theme.gu(2) + theme.gu(0.75) : 0

    implicitHeight: theme.touch
    implicitWidth: label.implicitWidth + iconSpace + theme.gu(3)
    Accessible.role: Accessible.RadioButton
    Accessible.checked: selected

    contentItem: Text {
        textFormat: Text.PlainText
        id: label
        text: root.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        leftPadding: root.iconSpace
        Icon {
            objectName: "chipIcon"
            visible: root.iconName !== ""
            theme: root.theme
            name: root.iconName
            color: root.iconColor
            size: root.theme.gu(2)
            // Centred in the label's own width, so the pair stays centred.
            x: (parent.width - parent.contentWidth - root.iconSpace) / 2
            anchors.verticalCenter: parent.verticalCenter
        }
        color: root.theme.airmail ? (root.selected ? root.theme.heading : root.theme.secondary)
                                  : (root.selected ? root.theme.primaryText : root.theme.text)
        font.family: root.theme.fontLabel
        font.pixelSize: root.theme.airmail ? 15 : 13
        font.weight: root.selected ? Font.DemiBold : Font.Medium
    }
    background: Item {
        Rectangle {
            visible: !root.theme.airmail
            anchors.fill: parent
            radius: root.theme.chipRadius
            color: root.selected ? root.theme.primary : "transparent"
            border.width: 1
            border.color: root.selected ? root.theme.primary : root.theme.field
        }
        Rectangle {
            visible: root.theme.airmail && root.selected
            anchors.bottom: parent.bottom
            width: parent.width
            height: 3
            color: root.theme.heading
        }
    }
}
