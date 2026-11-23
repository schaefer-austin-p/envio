import QtQuick 2.15
import "../../platform"

// The pull-to-refresh indicator (board: pull to refresh): one line, a disc
// on the left and the words on its right. The disc is a hollow ring with an
// arrow whose arc fills as the list is pulled; it turns into a filled disc
// with an up arrow when letting go will refresh, and into a ring with a
// turning arc while it runs.
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
    readonly property real disc: theme.gu(3)
    // The track and arc colours: the text colour, so a small sweep stands out
    // on the light and the dark ground alike.
    readonly property color track: Qt.rgba(theme.secondary.r, theme.secondary.g, theme.secondary.b, 0.4)

    y: 0
    height: gap
    clip: true
    visible: shown && gap > 0
    Accessible.role: Accessible.Indicator
    Accessible.name: label

    Row {
        id: line
        objectName: "pullLine"
        anchors.centerIn: parent
        spacing: root.theme.gu(1)
        Item {
            id: glyph
            objectName: "pullGlyph"
            width: root.disc
            height: root.disc
            anchors.verticalCenter: parent.verticalCenter
            // Only the armed state is filled.
            Rectangle {
                anchors.fill: parent
                radius: width / 2
                color: root.theme.primary
                visible: root.armed && !root.busy
            }
            Canvas {
                id: ring
                anchors.fill: parent
                readonly property color trackColor: root.track
                readonly property color arcColor: root.busy ? root.theme.primary : root.theme.text
                readonly property real sweep: root.busy ? 0.7 : root.progress
                readonly property bool filled: root.armed && !root.busy
                onTrackColorChanged: requestPaint()
                onArcColorChanged: requestPaint()
                onSweepChanged: requestPaint()
                onFilledChanged: requestPaint()
                onWidthChanged: requestPaint()
                onPaint: {
                    const ctx = getContext("2d")
                    ctx.reset()
                    if (filled) return
                    ctx.lineWidth = 2
                    ctx.lineCap = "round"
                    const r = width / 2 - 1.5
                    ctx.strokeStyle = trackColor
                    ctx.beginPath()
                    ctx.arc(width / 2, height / 2, r, 0, Math.PI * 2)
                    ctx.stroke()
                    if (sweep <= 0) return
                    ctx.strokeStyle = arcColor
                    ctx.beginPath()
                    ctx.arc(width / 2, height / 2, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * sweep)
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
                color: root.armed ? root.theme.primaryText : root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: root.disc * 0.6
                Behavior on rotation { NumberAnimation { duration: 120 } }
            }
        }
        Text {
            objectName: "pullLabel"
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: root.label
            color: root.armed && !root.busy ? root.theme.text : root.theme.secondary
            font.family: root.theme.fontLabel
            font.pixelSize: 13
            font.weight: root.armed && !root.busy ? Font.DemiBold : Font.Normal
        }
    }
}
