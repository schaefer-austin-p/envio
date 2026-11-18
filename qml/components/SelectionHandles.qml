import QtQuick 2.15
import "../platform"

// Two drag handles on a read-only TextEdit's selection, to extend it by touch
// (a plain TextEdit has none). Visible only while something is selected. The
// start never passes the end, and a handle drag does not scroll the page.
Item {
    id: root
    required property Theme theme
    required property Item edit
    // The Flickable to freeze while a handle is dragged, if any.
    property Item flick: null

    readonly property bool active: edit.selectionStart !== edit.selectionEnd
    anchors.fill: edit
    visible: active

    // Move one end of the selection to the line above the finger at `pt`
    // (the finger is under the knob).
    function dragTo(atStart, pt) {
        var p = edit.positionAt(pt.x, pt.y - theme.gu(2.5))
        var s = edit.selectionStart
        var e = edit.selectionEnd
        if (atStart) edit.select(Math.min(p, e - 1), e)
        else edit.select(s, Math.max(p, s + 1))
    }

    component Handle: Rectangle {
        id: knob
        property bool atStart: false
        readonly property rect caret: root.edit.positionToRectangle(
            atStart ? root.edit.selectionStart : root.edit.selectionEnd)
        width: root.theme.gu(2.5)
        height: width
        radius: width / 2
        color: root.theme.link
        x: caret.x - width / 2
        y: caret.y + caret.height
        MouseArea {
            // A bigger touch target than the knob.
            anchors.centerIn: parent
            width: root.theme.touch
            height: root.theme.touch
            preventStealing: true
            onPressed: if (root.flick) root.flick.interactive = false
            onReleased: if (root.flick) root.flick.interactive = true
            onCanceled: if (root.flick) root.flick.interactive = true
            onPositionChanged: root.dragTo(knob.atStart, mapToItem(root.edit, mouse.x, mouse.y))
        }
    }

    Handle { objectName: "startHandle"; atStart: true }
    Handle { objectName: "endHandle" }
}
