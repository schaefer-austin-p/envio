import QtQuick 2.15
import "../bindle/Behaviours"

// The two-pane layout's splitter for the list pane (#192, frame 10): a strip
// over the hairline, 1 gu wide for a mouse and 3 gu for a touch, with a grip
// for a finger. Owns the live drag and the width shown; the saved width, its
// bounds and the default come from the core, and `room` is what the window
// leaves (the app's clamp).
SplitHandle {
    id: root
    objectName: "splitter"

    property var theme: null
    property var core: null
    // The most the window allows, in gu (the reader and drawer keep theirs).
    property real room: 0
    // Where the drawer ends, in px (0 when it is not inline).
    property real leftEdge: 0
    // Whether the layout has a list pane beside the reader at all.
    property bool active: true

    // The live drag in gu, -1 when none: it stays until the core echoes, so
    // the list does not jump back meanwhile.
    property real dragGu: -1
    readonly property int minGu: core ? core.listPaneMinGu() : 36
    readonly property int maxGu: core ? core.listPaneMaxGu() : 70
    readonly property int defaultGu: core ? core.listPaneDefaultGu() : 50
    readonly property real _upper: Math.min(maxGu, room)
    // The width the list shows, in gu, and in px.
    readonly property real shownGu: Math.max(minGu, Math.min(_upper,
        dragGu >= 0 ? dragGu : (core && core.listPaneGu > 0 ? core.listPaneGu : defaultGu)))
    readonly property real shownWidth: theme.gu(shownGu)
    // Whether the last pointer was a finger (TouchSafeHover's `touched`,
    // watched over the whole window by the shell).
    property bool touchSeen: false

    readonly property real hit: touchSeen ? theme.gu(3) : theme.gu(1)
    visible: active && usable
    x: leftEdge + shownWidth + 0.5 - hit / 2
    width: hit
    z: 2
    value: shownGu
    minimum: minGu
    maximum: _upper
    defaultValue: defaultGu
    hideAtDefault: true
    pxPerUnit: theme.gu(1)

    // Dragging works in px; the core is told whole gu, and the live width is
    // whole gu as well, so the release does not snap.
    onMoved: function(size) { dragGu = Math.round(size) }
    onCommitted: function(size) {
        _saving = true
        dragGu = Math.round(size)
        core.setListPane(Math.round(size))
    }
    // A drag that ends where it began commits nothing: its width must not
    // linger (a saved 70 shown as 60 would stay 60 when the window grows).
    property bool _saving: false
    onDraggingChanged: {
        if (dragging) _saving = false
        else if (!_saving) dragGu = -1
    }
    // Any settings echo, changed or not (a failed save echoes the old
    // width), ends the drag's hold on the width; one that arrives mid-drag
    // is stale and left for the drag's own commit.
    Connections {
        target: root.core
        function onSettingsEchoed() {
            if (root.dragging) return
            root._saving = false
            root.dragGu = -1
        }
    }

    Rectangle {
        objectName: "splitterLine"
        anchors { top: parent.top; bottom: parent.bottom; horizontalCenter: parent.horizontalCenter }
        width: 2
        color: root.theme.accent
        visible: root.hovered || root.dragging
    }
    Rectangle {
        objectName: "splitterGrip"
        anchors.centerIn: parent
        width: 4
        height: 32
        radius: width / 2
        color: root.theme.secondary
        visible: root.touchSeen
    }
    Rectangle {
        objectName: "splitterFocus"
        anchors.fill: parent
        color: "transparent"
        border { width: 2; color: root.theme.text }
        visible: root.focusVisible
    }
}
