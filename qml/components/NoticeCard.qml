import QtQuick 2.15
import "../platform"

// A card on a page with a short title and a text: tint A with the accent
// (`kind` "warning": a problem, or a question the user must answer) or tint B
// (`kind` "info": a result, nothing wrong). The title may be empty. Unlike
// Banner it lies in the page's content, not under the top bar. Text comes
// ready-made and is shown as plain text.
Rectangle {
    id: root
    required property Theme theme
    property string kind: "info"
    property string title: ""
    property string text: ""

    implicitHeight: body.implicitHeight + theme.gu(3)
    radius: theme.cardRadius
    color: kind === "warning" ? theme.tintA : theme.tintB

    Icon {
        id: icon
        theme: root.theme
        name: root.kind === "warning" ? "warning" : "info"
        size: 20
        color: root.kind === "warning" ? root.theme.accent : root.theme.text
        x: root.theme.gu(1.75)
        y: root.theme.gu(1.5)
    }
    Column {
        id: body
        x: icon.x + icon.width + root.theme.gu(1.25)
        y: root.theme.gu(1.5)
        width: root.width - x - root.theme.gu(1.75)
        spacing: 4
        Text {
            textFormat: Text.PlainText
            objectName: "noticeTitle"
            width: parent.width
            visible: root.title !== ""
            text: root.title
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 14
            font.weight: Font.DemiBold
            lineHeight: 1.2
        }
        Text {
            textFormat: Text.PlainText
            objectName: "noticeText"
            width: parent.width
            visible: root.text !== ""
            text: root.text
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 14
            lineHeight: 1.3
        }
    }
}
