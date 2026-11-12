import QtQuick 2.15
import "../platform"

// Airmail's border: a red and blue striped band (board: airmail-*) between
// frameOuterRadius (bottom corners; the top ones are frameTopOuterRadius, square
// against the system bar) and frameInnerRadius, frameInset wide, around its content.
// In Carbon the inset is 0 and nothing is drawn, so the content fills the
// frame. The ring sits above the content, so the content's square corners
// are covered by the band's inner rounded edge and never show through.
Item {
    id: root
    required property Theme theme
    // Extra space inside the ring, around the content.
    property real padding: 0
    default property alias content: inner.data
    // For tests.
    readonly property Item area: inner
    readonly property Item ring: canvas

    implicitWidth: inner.childrenRect.width + 2 * (theme.frameInset + padding)
    implicitHeight: inner.childrenRect.height + 2 * (theme.frameInset + padding)

    // The paper under the content, shaped like the ring's outer edge: rounded
    // at the bottom, with a square-cornered top half over it.
    Rectangle {
        anchors.fill: parent
        visible: root.theme.airmail
        radius: root.theme.frameOuterRadius
        color: root.theme.ground
    }
    Rectangle {
        width: parent.width
        height: parent.height / 2
        visible: root.theme.airmail && root.theme.frameTopOuterRadius === 0
        color: root.theme.ground
    }

    Item {
        id: inner
        x: root.theme.frameInset + root.padding
        y: x
        width: root.width - 2 * x
        height: root.height - 2 * y
    }

    Canvas {
        id: canvas
        objectName: "frameCanvas"
        anchors.fill: parent
        visible: root.theme.airmail
        enabled: false
        z: 1
        readonly property color red: root.theme.chevron
        readonly property color blue: root.theme.chevronBlue
        readonly property color paper: root.theme.ground
        readonly property real inset: root.theme.frameInset
        readonly property real outerR: root.theme.frameOuterRadius
        readonly property real topR: root.theme.frameTopOuterRadius
        readonly property real innerR: root.theme.frameInnerRadius
        onRedChanged: requestPaint()
        onBlueChanged: requestPaint()
        onPaperChanged: requestPaint()
        onInsetChanged: requestPaint()
        onOuterRChanged: requestPaint()
        onTopRChanged: requestPaint()
        onInnerRChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onVisibleChanged: if (visible) requestPaint()

        // A rectangle as one closed subpath, clockwise or not, with radius rt
        // on the top corners and rb on the bottom ones. The moveTo first keeps
        // the first arc from joining a stray line to the previous point.
        function roundedRect(ctx, x, y, w, h, rt, rb, clockwise) {
            const pi = Math.PI
            ctx.moveTo(x + rt, y)
            if (clockwise) {
                ctx.arc(x + w - rt, y + rt, rt, -pi / 2, 0, false)
                ctx.arc(x + w - rb, y + h - rb, rb, 0, pi / 2, false)
                ctx.arc(x + rb, y + h - rb, rb, pi / 2, pi, false)
                ctx.arc(x + rt, y + rt, rt, pi, 3 * pi / 2, false)
            } else {
                ctx.arc(x + rt, y + rt, rt, -pi / 2, -pi, true)
                ctx.arc(x + rb, y + h - rb, rb, pi, pi / 2, true)
                ctx.arc(x + w - rb, y + h - rb, rb, pi / 2, 0, true)
                ctx.arc(x + w - rt, y + rt, rt, 0, -pi / 2, true)
            }
            ctx.closePath()
        }

        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            if (!visible || width <= 0 || height <= 0) return
            // The band: the outer shape clockwise, the inner one counter-
            // clockwise, so both fill rules cut the inner one out.
            ctx.beginPath()
            roundedRect(ctx, 0, 0, width, height, topR, outerR, true)
            roundedRect(ctx, inset, inset, width - 2 * inset, height - 2 * inset, innerR, innerR, false)
            ctx.clip()
            // 135 degrees: the stripes run along x + y = const, 14 px wide
            // measured across, in the order red, paper, blue, paper.
            const colours = [red, paper, blue, paper]
            const step = 14 * Math.SQRT2
            for (let k = 0; k * step < width + height; ++k) {
                const s0 = k * step, s1 = s0 + step
                ctx.fillStyle = colours[k % 4]
                ctx.beginPath()
                ctx.moveTo(s0, 0)
                ctx.lineTo(s1, 0)
                ctx.lineTo(s1 - height, height)
                ctx.lineTo(s0 - height, height)
                ctx.closePath()
                ctx.fill()
            }
        }
    }
}
