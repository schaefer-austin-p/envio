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
        radius: root.theme.gu(1)
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
            leftPadding: root.theme.gu(1.75)
            rightPadding: root.theme.gu(1.75)
            // The press that starts a hold moves the cursor and drops the
            // selection; keep it, so Copy and Cut have something to act on.
            property int heldStart: 0
            property int heldEnd: 0
            property bool held: false
            function remember() {
                if (selectionStart !== selectionEnd) {
                    heldStart = selectionStart
                    heldEnd = selectionEnd
                }
            }
            onSelectionStartChanged: remember()
            onSelectionEndChanged: remember()
            onTextChanged: heldStart = heldEnd = 0
            onPressed: held = false
            // A plain tap deselects for good; only a hold brings it back.
            onReleased: { if (!held) heldStart = heldEnd = 0 }
            onPressAndHold: function (event) {
                held = true
                if (selectedText === "" && heldStart !== heldEnd)
                    select(heldStart, heldEnd)
                menu.popup(input, event.x, event.y)
            }
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
    }
}
