import QtQuick 2.15
import QtQuick.Controls 2.15
import "../../platform"

// An underlined text link with a 44 px tap target (the invite card's
// "Add a note", "Hide note" and "Change", board 4).
AbstractButton {
    id: root
    required property Theme theme

    implicitHeight: Math.max(44, theme.touch)
    implicitWidth: label.implicitWidth + theme.gu(1)

    contentItem: Text {
        id: label
        textFormat: Text.PlainText
        text: root.text
        verticalAlignment: Text.AlignVCenter
        color: root.theme.link
        font.family: root.theme.fontUi
        font.pixelSize: 14
        font.weight: Font.DemiBold
        font.underline: true
    }
}
