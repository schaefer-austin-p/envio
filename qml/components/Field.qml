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
    // False: no box, for a field that sits inside a row of its own.
    property bool framed: true
    // Space before the text: inside the box, or none without one, so a row's
    // text lines up with its column.
    property real inset: framed ? theme.gu(1.75) : 0
    readonly property int effectiveHints: input.inputMethodHints
    // Extra hints; a password field always keeps no-prediction and sensitive.
    property int inputMethodHints: Qt.ImhNone
    property alias readOnly: input.readOnly
    // For tests: the inner field and its menu.
    readonly property alias input: input
    readonly property alias editMenu: menu
    signal accepted()

    spacing: theme.gu(0.75)
    opacity: enabled ? 1 : 0.45

    Text {
        textFormat: Text.PlainText
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
        radius: root.theme.fieldRadius
        color: root.framed ? root.theme.surface : "transparent"
        border.width: !root.framed ? 0 : input.activeFocus ? 2 : 1
        border.color: input.activeFocus ? root.theme.primary : root.theme.field

        TextField {
            id: input
            objectName: "fieldInput"
            background: null
            topPadding: 0
            bottomPadding: 0
            selectByMouse: true
            persistentSelection: true
            leftPadding: root.inset
            rightPadding: root.theme.gu(1.75)
            anchors.fill: parent
            verticalAlignment: TextInput.AlignVCenter
            clip: true
            color: root.theme.text
            selectionColor: root.theme.tintB
            selectedTextColor: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 16
            echoMode: root.password ? TextInput.Password : TextInput.Normal
            inputMethodHints: root.password
                ? root.inputMethodHints | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase | Qt.ImhSensitiveData
                : root.inputMethodHints
            onAccepted: root.accepted()
            Text {
                textFormat: Text.PlainText
                id: hint
                visible: input.text === "" && !input.activeFocus
                anchors.fill: parent
                leftPadding: input.leftPadding
                verticalAlignment: Text.AlignVCenter
                color: root.theme.secondary
                font: input.font
            }
        }
        EditMenu {
            id: menu
            theme: root.theme
            field: input
            password: root.password
        }
        HoldSelection { field: input; menu: menu }
    }
}
