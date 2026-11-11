import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A multi-line text area with the edit menu, for a message body. It grows
// with its text (the page scrolls, not the area). Like Field, a press and
// hold keeps the selection, so Copy and Cut have something to act on.
Rectangle {
    id: root
    required property Theme theme
    property alias text: input.text
    property alias placeholder: hint.text
    readonly property alias input: input
    readonly property alias editMenu: menu

    implicitHeight: Math.max(theme.touch * 3, input.contentHeight + theme.gu(3))
    color: "transparent"

    TextArea {
        id: input
        objectName: "textBlockInput"
        anchors.fill: parent
        background: null
        topPadding: root.theme.gu(1.5)
        bottomPadding: root.theme.gu(1.5)
        leftPadding: root.theme.gu(2)
        rightPadding: root.theme.gu(2)
        wrapMode: TextEdit.Wrap
        selectByMouse: true
        persistentSelection: true
        color: root.theme.text
        selectionColor: root.theme.tintB
        selectedTextColor: root.theme.text
        font.family: root.theme.fontUi
        font.pixelSize: 16

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
        onReleased: { if (!held) heldStart = heldEnd = 0 }
        onPressAndHold: function (event) {
            held = true
            if (selectedText === "" && heldStart !== heldEnd)
                select(heldStart, heldEnd)
            menu.popup(input, event.x, event.y)
        }
        Text {
            id: hint
            textFormat: Text.PlainText
            visible: input.text === "" && !input.activeFocus
            x: input.leftPadding
            y: input.topPadding
            color: root.theme.secondary
            font: input.font
        }
    }
    EditMenu {
        id: menu
        theme: root.theme
        field: input
    }
}
