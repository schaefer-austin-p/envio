import QtQuick 2.15
import "../platform"
import "iconShapes.js" as Shapes

// A line icon drawn on a canvas in a theme colour, so no icon set (and no
// Suru) is needed. The shapes are in iconShapes.js (mostly Lucide, ISC; see
// NOTICES.md); `name` is one of its `names`: back, forward, down, close,
// check, plus, send, search, menu, alert, info, archive, trash, move, more,
// flag, clip, mail, mailopen, pencil, offline, external, and the Lucide
// names of the sheet and menu items (trash-2, folder-input, external-link,
// ...).
Canvas {
    id: root
    required property Theme theme
    property string name: ""
    property color color: theme.text
    property real size: theme.iconSize

    implicitWidth: size
    implicitHeight: size
    onNameChanged: requestPaint()
    onColorChanged: requestPaint()
    onSizeChanged: requestPaint()

    onPaint: {
        const ctx = getContext("2d")
        ctx.reset()
        const k = width / 24
        ctx.scale(k, k)
        ctx.strokeStyle = root.color
        ctx.lineWidth = 2
        ctx.lineCap = "round"
        ctx.lineJoin = "round"
        const name = Shapes.alias[root.name] || root.name
        if (Shapes.svg[name] !== undefined) {
            ctx.path = Shapes.svg[name]
            ctx.stroke()
            return
        }
        const pts = Shapes.points[name] || []
        ctx.beginPath()
        let start = true
        for (const p of pts) {
            if (p === null) { start = true; continue }
            if (start) ctx.moveTo(p[0], p[1]); else ctx.lineTo(p[0], p[1])
            start = false
        }
        // A ring starts its own subpath; arc() alone would join it to the
        // last point with a stray line.
        const ring = Shapes.rings[name]
        if (ring !== undefined) {
            ctx.moveTo(ring[0] + ring[2], ring[1])
            ctx.arc(ring[0], ring[1], ring[2], 0, 2 * Math.PI)
        }
        ctx.stroke()
    }
}
