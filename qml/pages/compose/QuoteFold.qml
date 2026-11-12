import QtQuick 2.15
import "../../platform"
import "../../components"

// The quoted original of a reply or forward, folded away. It goes out with
// the message unless the user removes it. The quote is another person's
// text: plain text, never rich.
Column {
    id: root
    required property Theme theme
    property string quoted: ""
    property bool expanded: false
    signal removed()

    spacing: theme.gu(1)
    padding: theme.gu(2)

    function reset() { expanded = false }

    AppButton {
        objectName: "quoteShow"
        visible: !root.expanded
        theme: root.theme
        text: qsTr("Show quoted text")
        onClicked: root.expanded = true
    }
    Text {
        textFormat: Text.PlainText
        objectName: "quoteText"
        visible: root.expanded
        width: parent.width - root.theme.gu(4)
        text: root.quoted
        wrapMode: Text.Wrap
        color: root.theme.secondary
        font.family: root.theme.fontUi
        font.pixelSize: 14
    }
    Row {
        visible: root.expanded
        spacing: root.theme.gu(1)
        AppButton {
            objectName: "quoteHide"
            theme: root.theme
            text: qsTr("Hide")
            onClicked: root.expanded = false
        }
        AppButton {
            objectName: "quoteRemove"
            theme: root.theme
            text: qsTr("Remove quoted text")
            onClicked: root.removed()
        }
    }
}
