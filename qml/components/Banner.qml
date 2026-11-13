import QtQuick 2.15
import "../platform"

// An inline status under the top bar: offline, images blocked, Outbox
// waiting. `kind` "warning" uses tint A and an accent alert icon, "info"
// the selected colour. The text comes ready-made (counts from Rust).
Rectangle {
    id: root
    required property Theme theme
    property string kind: "info"
    property string text: ""

    implicitHeight: Math.max(theme.touch, label.implicitHeight + theme.gu(3))
    color: kind === "warning" ? theme.tintA : theme.selected

    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: root.theme.hairline
    }
    Icon {
        id: icon
        theme: root.theme
        name: root.kind === "warning" ? "alert" : "info"
        size: root.theme.iconButton
        color: root.kind === "warning" ? root.theme.accent : root.theme.text
        anchors.left: parent.left
        anchors.leftMargin: root.theme.gu(2)
        anchors.verticalCenter: parent.verticalCenter
    }
    Text {
        textFormat: Text.PlainText
        id: label
        anchors.left: icon.right
        anchors.leftMargin: root.theme.gu(1.25)
        anchors.right: parent.right
        anchors.rightMargin: root.theme.gu(2)
        anchors.verticalCenter: parent.verticalCenter
        text: root.text
        wrapMode: Text.Wrap
        lineHeight: 1.3
        color: root.theme.text
        font.family: root.theme.fontUi
        font.pixelSize: 14
    }
}
