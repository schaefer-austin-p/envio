import QtQuick 2.15
import "../../platform"

// The pull-to-refresh indicator (board: pull to refresh): a disc with a ring
// that fills as the list is pulled and an arrow that turns when letting go
// will refresh, then a turning arc while it runs, and a line saying which.
// An Item over the list's top, in the list's coordinates (`y: 0`, as high as
// the room the pull opens, `gap`), clipped; it is not part of the list's
// content. It only draws: the PullToRefresh behaviour in bindle decides.
Item {
    id: root
    objectName: "pullIndicator"
    required property Theme theme
    // The room above the first row, in px (the behaviour's `gap`).
    property real gap: 0
    property real progress: 0
    property bool armed: false
    property bool busy: false
    property bool shown: true

    // The words, also the accessible name.
    readonly property string label: busy ? qsTr("Refreshing")
        : armed ? qsTr("Release to refresh") : qsTr("Pull to refresh")
    readonly property real disc: theme.gu(4)

    y: 0
    height: gap
    clip: true
    visible: shown && gap > 0
    Accessible.role: Accessible.Indicator
    Accessible.name: label

    Column {
        anchors.centerIn: parent
        spacing: root.theme.gu(0.5)
        Item {
            id: glyph
            objectName: "pullGlyph"
            width: root.disc
            height: root.disc
            anchors.horizontalCenter: parent.horizontalCenter
            Rectangle {
                anchors.fill: parent
                radius: width / 2
                color: root.armed || root.busy ? root.theme.primary : root.theme.field
                opacity: root.armed || root.busy ? 1 : 0.35
            }
            Canvas {
                id: ring
                anchors.fill: parent
                readonly property color stroke: root.theme.secondary
                readonly property real sweep: root.busy ? 0.25 : root.progress
                onStrokeChanged: requestPaint()
                onSweepChanged: requestPaint()
                onWidthChanged: requestPaint()
                onPaint: {
                    const ctx = getContext("2d")
                    ctx.reset()
                    if (sweep <= 0) return
                    ctx.strokeStyle = root.busy ? root.theme.primaryText : stroke
                    ctx.lineWidth = 2
                    ctx.lineCap = "round"
                    ctx.beginPath()
                    ctx.arc(width / 2, height / 2, width / 2 - 2, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * sweep)
                    ctx.stroke()
                }
                RotationAnimator on rotation {
                    from: 0
                    to: 360
                    duration: 900
                    loops: Animation.Infinite
                    running: root.busy && root.visible
                }
            }
            Text {
                objectName: "pullArrow"
                anchors.centerIn: parent
                visible: !root.busy
                textFormat: Text.PlainText
                text: "↓"
                rotation: root.armed ? 180 : 0
                color: root.armed ? root.theme.primaryText : root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: root.disc * 0.5
                Behavior on rotation { NumberAnimation { duration: 120 } }
            }
        }
        Text {
            objectName: "pullLabel"
            anchors.horizontalCenter: parent.horizontalCenter
            textFormat: Text.PlainText
            text: root.label
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 13
        }
    }
}
