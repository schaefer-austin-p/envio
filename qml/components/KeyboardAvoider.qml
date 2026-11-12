import QtQuick 2.15
import QtQuick.Window 2.15
import "keyboardRect.js" as KeyboardRect

// Keeps the focused field of a Flickable above the on-screen keyboard: the
// keyboard's overlap becomes the Flickable's bottom margin (so the content
// can scroll clear of it) and the focused field, or the cursor line of a
// long text, is scrolled into view. Put one inside the Flickable's parent.
// Where the shell already shrinks the window for the keyboard there is no
// overlap and nothing moves. Never reads or logs the text.
Item {
    id: root
    required property Flickable flickable
    // The keyboard in window coordinates. Tests set it; on the phone it is
    // Qt's.
    property rect keyboard: KeyboardRect.logical(Qt.inputMethod.keyboardRectangle,
        Screen.width, Screen.devicePixelRatio)
    // Space kept between the field and the keyboard.
    property real gap: 16
    // For tests: how far the keyboard covers the Flickable.
    readonly property real covered: flickable.bottomMargin
    readonly property Item focused: Window.activeFocusItem
    visible: false

    function overlap() {
        if (keyboard.height <= 0) return 0
        const bottom = flickable.mapToItem(null, 0, flickable.height).y
        return Math.max(0, Math.min(flickable.height, bottom - keyboard.y))
    }
    function inside(item) {
        for (let p = item; p; p = p.parent) if (p === flickable.contentItem) return true
        return false
    }
    function reveal() {
        flickable.bottomMargin = overlap()
        const item = focused
        if (!item || !inside(item)) return
        // A long text: its cursor line, not the whole text.
        const lines = item.lineCount !== undefined && item.cursorRectangle !== undefined
        const r = lines ? item.cursorRectangle : Qt.rect(0, 0, item.width, item.height)
        const top = item.mapToItem(flickable.contentItem, r.x, r.y).y - gap
        const bottom = top + r.height + 2 * gap
        const view = flickable.height - flickable.bottomMargin
        if (bottom > flickable.contentY + view) flickable.contentY = bottom - view
        if (top < flickable.contentY) flickable.contentY = top
    }

    onKeyboardChanged: reveal()
    onFocusedChanged: reveal()
    Connections {
        target: root.flickable
        function onHeightChanged() { root.reveal() }
    }
    Connections {
        target: root.focused
        ignoreUnknownSignals: true
        function onCursorRectangleChanged() { root.reveal() }
    }
}
