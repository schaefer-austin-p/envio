import QtQuick 2.15
import "../../platform"
import "../../components"

// The Subject line: a row like From and To, with the label in the same left
// column and the text typed inline, no box. `text` holds the subject.
Item {
    id: root
    required property Theme theme
    property alias text: field.text
    // For tests: the inner field and its input.
    readonly property alias field: field
    readonly property alias input: field.input

    implicitHeight: root.theme.touch + 1

    Text {
        objectName: "subjectLabel"
        textFormat: Text.PlainText
        width: root.theme.gu(6.5)
        height: root.theme.touch
        verticalAlignment: Text.AlignVCenter
        leftPadding: root.theme.gu(2)
        text: qsTr("Subject")
        color: root.theme.secondary
        font.family: root.theme.fontUi
        font.pixelSize: 14
        // Tapping the label focuses the input too.
        MouseArea { anchors.fill: parent; onClicked: field.input.forceActiveFocus() }
    }
    Field {
        id: field
        x: root.theme.gu(6.5)
        width: root.width - root.theme.gu(6.5) - root.theme.gu(1)
        theme: root.theme
        framed: false
    }
    Rectangle { y: root.theme.touch; width: parent.width; height: 1; color: root.theme.hairline }
}
