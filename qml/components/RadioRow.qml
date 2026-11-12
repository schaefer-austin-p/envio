import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// One choice of a group: a label, an optional caption and a radio dot.
// The page owns the group: it sets `selected` on the selected row and
// reacts to `activated`.
AbstractButton {
    id: root
    required property Theme theme
    property string caption: ""
    // Not AbstractButton.checked: Qt turns checkable on once that is set, and
    // every tap would then toggle it off under the page's binding.
    property bool selected: false
    signal activated()

    implicitHeight: Math.max(theme.touch, col.implicitHeight + theme.gu(2))
    Accessible.role: Accessible.RadioButton
    Accessible.checked: selected
    onClicked: activated()

    contentItem: Item {
        Column {
            id: col
            anchors.left: parent.left
            anchors.leftMargin: root.theme.gu(2)
            anchors.right: dot.left
            anchors.rightMargin: root.theme.gu(1)
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                textFormat: Text.PlainText
                width: parent.width
                text: root.text
                elide: Text.ElideRight
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 16
                font.weight: Font.Medium
            }
            Text {
                textFormat: Text.PlainText
                width: parent.width
                visible: root.caption !== ""
                text: root.caption
                wrapMode: Text.WordWrap
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 13
            }
        }
        Rectangle {
            id: dot
            anchors.right: parent.right
            anchors.rightMargin: root.theme.gu(2)
            anchors.verticalCenter: parent.verticalCenter
            width: 22
            height: 22
            radius: width / 2
            color: "transparent"
            border.width: 2
            border.color: root.theme.primary
            Rectangle {
                visible: root.selected
                anchors.centerIn: parent
                width: 10
                height: 10
                radius: width / 2
                color: root.theme.primary
            }
        }
    }
    background: Rectangle {
        color: root.down ? root.theme.selected : "transparent"
    }
}
