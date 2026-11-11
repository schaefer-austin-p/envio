import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A labelled single-line text field. `password` hides the text and turns
// prediction off; the text is read from `text`, nothing else holds it.
Column {
    id: root
    required property Theme theme
    property string label: ""
    property alias text: input.text
    property alias placeholder: hint.text
    property bool password: false
    property alias inputMethodHints: input.inputMethodHints
    property alias readOnly: input.readOnly
    signal accepted()

    spacing: theme.gu(0.75)
    opacity: enabled ? 1 : 0.45

    Text {
        visible: root.label !== ""
        text: root.label
        color: root.theme.secondary
        font.family: root.theme.fontUi
        font.pixelSize: 13
        font.weight: Font.Medium
    }
    Rectangle {
        width: root.width
        height: root.theme.touch
        radius: root.theme.gu(1)
        color: root.theme.surface
        border.width: input.activeFocus ? 2 : 1
        border.color: input.activeFocus ? root.theme.primary : root.theme.field

        TextInput {
            id: input
            anchors.fill: parent
            anchors.leftMargin: root.theme.gu(1.75)
            anchors.rightMargin: root.theme.gu(1.75)
            verticalAlignment: TextInput.AlignVCenter
            clip: true
            color: root.theme.text
            selectionColor: root.theme.tintB
            selectedTextColor: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 16
            echoMode: root.password ? TextInput.Password : TextInput.Normal
            inputMethodHints: root.password ? Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase | Qt.ImhSensitiveData
                                            : Qt.ImhNone
            onAccepted: root.accepted()
            Text {
                id: hint
                visible: input.text === "" && !input.activeFocus
                anchors.fill: parent
                verticalAlignment: Text.AlignVCenter
                color: root.theme.secondary
                font: input.font
            }
        }
    }
}
