import QtQuick 2.15
import QtQuick.Window 2.15

// The keyboard shortcuts of the wide layout (two-pane board, frame 7), as
// signals the shell answers. Off unless `active` (wide, at the list, with a
// keyboard); nothing here knows what a key does. The keys that a text field
// also uses (the cursor keys, Enter, Delete, Escape, "?") are ignored while
// an editable text field has focus. A read-only text item (the reader body)
// does not count, so they still work there.
Item {
    id: root
    objectName: "shortcutLayer"
    property bool active: false

    signal moveFocus(int delta)
    signal openFocusedRequested()
    signal deleteRequested()
    signal newRequested()
    signal replyRequested()
    signal closeRequested()
    signal helpRequested()

    // An editable TextInput or TextEdit has the keyboard.
    function typing() {
        const w = root.Window.window
        const f = w ? w.activeFocusItem : null
        return f !== null && f !== undefined && f.readOnly === false
    }

    readonly property alias upKey: up
    readonly property alias downKey: down
    readonly property alias openKey: open
    readonly property alias deleteKey: del
    readonly property alias newKey: newMessage
    readonly property alias replyKey: reply
    readonly property alias closeKey: close
    readonly property alias helpKey: help

    Shortcut { id: up; sequence: "Up"; enabled: root.active; onActivated: if (!root.typing()) root.moveFocus(-1) }
    Shortcut { id: down; sequence: "Down"; enabled: root.active; onActivated: if (!root.typing()) root.moveFocus(1) }
    Shortcut { id: open; sequences: ["Return", "Enter"]; enabled: root.active; onActivated: if (!root.typing()) root.openFocusedRequested() }
    Shortcut { id: del; sequence: "Delete"; enabled: root.active; onActivated: if (!root.typing()) root.deleteRequested() }
    Shortcut { id: newMessage; sequence: "Ctrl+N"; enabled: root.active; onActivated: root.newRequested() }
    Shortcut { id: reply; sequence: "Ctrl+R"; enabled: root.active; onActivated: root.replyRequested() }
    Shortcut { id: close; sequence: "Escape"; enabled: root.active; onActivated: if (!root.typing()) root.closeRequested() }
    Shortcut { id: help; sequence: "?"; enabled: root.active; onActivated: if (!root.typing()) root.helpRequested() }
}
