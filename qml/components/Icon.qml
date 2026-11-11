import QtQuick 2.15
import "../platform"

// A line icon drawn on a canvas in a theme colour, so no icon set (and no
// Suru) is needed. `name` is one of: back, forward, down, close, check,
// plus, search, menu, alert, info.
Canvas {
    id: root
    required property Theme theme
    property string name: ""
    property color color: theme.text
    property real size: 24

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
        const paths = {
            "back": [[15, 5], [8, 12], [15, 19]],
            "forward": [[9, 5], [16, 12], [9, 19]],
            "down": [[5, 9], [12, 16], [19, 9]],
            "close": [[6, 6], [18, 18], null, [18, 6], [6, 18]],
            "check": [[5, 12], [10, 17], [19, 7]],
            "plus": [[12, 5], [12, 19], null, [5, 12], [19, 12]],
            "menu": [[4, 6], [20, 6], null, [4, 12], [20, 12], null, [4, 18], [20, 18]],
            "alert": [[12, 6], [12, 13], null, [12, 17], [12, 17.1]],
            "info": [[12, 11], [12, 17], null, [12, 7], [12, 7.1]],
            "search": [[15.5, 15.5], [20, 20]]
        }
        const pts = paths[root.name] || []
        ctx.beginPath()
        let start = true
        for (const p of pts) {
            if (p === null) { start = true; continue }
            if (start) ctx.moveTo(p[0], p[1]); else ctx.lineTo(p[0], p[1])
            start = false
        }
        if (root.name === "search") ctx.arc(11, 11, 6.5, 0, 2 * Math.PI)
        if (root.name === "alert" || root.name === "info") ctx.arc(12, 12, 9.5, 0, 2 * Math.PI)
        ctx.stroke()
    }
}
