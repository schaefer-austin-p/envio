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
    // A handle drag began / ended (the page hides and re-opens its menu).
    signal dragStarted()
    signal dragFinished()

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

    // Where the edit menu's top left goes, in `edit`'s coordinates: above the
    // selection's first line, or below the end handle's touch target when
    // there is no room above inside `view` (an Item whose box bounds what
    // the user can see; null means unbounded). The menu never sits on a
    // handle, so both stay draggable.
    function menuSpot(menuW, menuH, view) {
        const a = edit.positionToRectangle(edit.selectionStart)
        const b = edit.positionToRectangle(edit.selectionEnd)
        const gap = theme.gu(1)
        const knob = theme.gu(2.5)
        const above = a.y - menuH - gap
        const belowEnd = b.y + b.height + knob / 2 + Math.max(knob, theme.touch) / 2 + gap
        let y = above
        if (view) {
            const top = edit.mapFromItem(view, 0, 0).y
            const bottom = edit.mapFromItem(view, 0, view.height).y
            if (above < top && belowEnd + menuH <= bottom) y = belowEnd
        } else if (above < 0) {
            y = belowEnd
        }
        const x = Math.max(0, Math.min(a.x, edit.width - menuW))
        return Qt.point(x, y)
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
            onPressed: {
                if (root.flick) root.flick.interactive = false
                root.dragStarted()
            }
            onReleased: {
                if (root.flick) root.flick.interactive = true
                root.dragFinished()
            }
            onCanceled: {
                if (root.flick) root.flick.interactive = true
                root.dragFinished()
            }
            onPositionChanged: root.dragTo(knob.atStart, mapToItem(root.edit, mouse.x, mouse.y))
        }
    }

    Handle { objectName: "startHandle"; atStart: true }
    Handle { objectName: "endHandle" }
}
