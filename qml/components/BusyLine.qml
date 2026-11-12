import QtQuick 2.15
import "../platform"

// A spinner and a status line, drawn in QtQuick so no Lomiri or Suru is
// needed: an open arc in the accent role turning beside `text` in the
// secondary role. Hidden, and not animating, unless `running`.
Item {
    id: root
    required property Theme theme
    property bool running: false
    property string text: ""
    property alias textItem: label

    readonly property real spinnerSize: theme.iconSize
    readonly property real gap: theme.gu(1.5)

    visible: running
    implicitWidth: spinnerSize + gap + label.implicitWidth
    implicitHeight: Math.max(spinnerSize, label.implicitHeight)

    Canvas {
        id: spinner
        objectName: "busySpinner"
        readonly property color stroke: root.theme.accent
        width: root.spinnerSize
        height: root.spinnerSize
        anchors.verticalCenter: parent.verticalCenter
        onStrokeChanged: requestPaint()
        onWidthChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            ctx.strokeStyle = stroke
            ctx.lineWidth = 2
            ctx.lineCap = "round"
            ctx.beginPath()
            ctx.arc(width / 2, height / 2, width / 2 - 2, 0, Math.PI * 1.5)
            ctx.stroke()
        }
        RotationAnimator on rotation {
            from: 0
            to: 360
            duration: 900
            loops: Animation.Infinite
            running: root.running && root.visible
        }
    }
    Text {
        id: label
        objectName: "busyText"
        anchors.verticalCenter: parent.verticalCenter
        x: root.spinnerSize + root.gap
        width: Math.max(0, root.width - x)
        textFormat: Text.PlainText
        text: root.text
        wrapMode: Text.WordWrap
        color: root.theme.secondary
        font.family: root.theme.fontUi
        font.pixelSize: 15
    }
}
