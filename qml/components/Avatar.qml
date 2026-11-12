import QtQuick 2.15
import "../platform"

// Initials in a circle (Carbon) or a perforated, tinted stamp (Airmail).
// `initials` is computed by the caller; `blue` picks Airmail's blue tint
// over the red (see platform/stampTint.js).
Rectangle {
    id: root
    required property Theme theme
    property string initials: ""
    property bool blue: false
    property real size: theme.avatarSize
    // The Canvas stamp is drawn only in Airmail.
    readonly property bool stampVisible: theme.airmail

    implicitWidth: theme.airmail ? theme.stampWidth : size
    implicitHeight: theme.airmail ? theme.stampHeight : size
    radius: theme.avatarRadius(size)
    color: theme.airmail ? "transparent" : theme.tintB

    // The board's mask: 3 px holes centred on the edge every 8 px.
    Canvas {
        id: stamp
        objectName: "stampCanvas"
        anchors.fill: parent
        visible: root.stampVisible
        readonly property color tint: root.blue ? root.theme.tintB : root.theme.tintA
        onTintChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onVisibleChanged: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            if (!root.stampVisible) return
            ctx.fillStyle = tint
            ctx.fillRect(0, 0, width, height)
            ctx.globalCompositeOperation = "destination-out"
            ctx.fillStyle = "#000000"
            for (var x = 0; x <= width; x += 8) {
                for (var edge = 0; edge < 2; edge++) {
                    ctx.beginPath()
                    ctx.arc(x, edge === 0 ? 0 : height, 3, 0, 2 * Math.PI)
                    ctx.fill()
                }
            }
            for (var y = 8; y < height; y += 8) {
                for (var side = 0; side < 2; side++) {
                    ctx.beginPath()
                    ctx.arc(side === 0 ? 0 : width, y, 3, 0, 2 * Math.PI)
                    ctx.fill()
                }
            }
        }
    }

    Text {
        textFormat: Text.PlainText
        anchors.centerIn: parent
        text: root.initials
        color: root.theme.airmail ? (root.blue ? root.theme.heading : root.theme.accent) : root.theme.text
        font.family: root.theme.fontTitle
        font.pixelSize: root.theme.airmail ? 17 : root.size * 0.34
        font.weight: Font.DemiBold
    }
}
