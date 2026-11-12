import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// An account tab or filter chip. Carbon: outlined, filled when `selected`.
// Airmail: underlined tab. 44 px high.
AbstractButton {
    id: root
    required property Theme theme
    property bool selected: false

    implicitHeight: theme.touch
    implicitWidth: label.implicitWidth + theme.gu(3)
    Accessible.role: Accessible.RadioButton
    Accessible.checked: selected

    contentItem: Text {
        textFormat: Text.PlainText
        id: label
        text: root.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        color: root.theme.airmail ? (root.selected ? root.theme.heading : root.theme.secondary)
                                  : (root.selected ? root.theme.primaryText : root.theme.text)
        font.family: root.theme.airmail ? root.theme.fontUi : root.theme.fontMono
        font.pixelSize: root.theme.airmail ? 15 : 13
        font.weight: root.selected ? Font.DemiBold : Font.Medium
    }
    background: Item {
        Rectangle {
            visible: !root.theme.airmail
            anchors.fill: parent
            radius: 6
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
