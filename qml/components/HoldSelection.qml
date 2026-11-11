import QtQuick 2.15

// Keeps a field's selection through a press and hold, so Copy and Cut have
// something to act on. The press that starts a hold moves the cursor and
// drops the selection; this remembers it and brings it back before the edit
// menu opens. Shared by Field and TextBlock. A plain tap deselects for good.
QtObject {
    id: root
    // The TextField or TextArea, and the EditMenu to open on a hold.
    property Item field
    property var menu

    property int heldStart: 0
    property int heldEnd: 0
    property bool held: false

    function remember() {
        if (field.selectionStart !== field.selectionEnd) {
            heldStart = field.selectionStart
            heldEnd = field.selectionEnd
        }
    }
    function forget() { heldStart = heldEnd = 0 }

    property Connections watch: Connections {
        target: root.field
        function onSelectionStartChanged() { root.remember() }
        function onSelectionEndChanged() { root.remember() }
        function onTextChanged() { root.forget() }
        function onPressed() { root.held = false }
        function onReleased() { if (!root.held) root.forget() }
        function onPressAndHold(event) {
            root.held = true
            if (root.field.selectedText === "" && root.heldStart !== root.heldEnd)
                root.field.select(root.heldStart, root.heldEnd)
            root.menu.popup(root.field, event.x, event.y)
        }
    }
}
