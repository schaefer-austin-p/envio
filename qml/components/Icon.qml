import QtQuick 2.15
import "../platform"

// A line icon drawn on a canvas in a theme colour, so no icon set (and no
// Suru) is needed. `name` is one of: back, forward, down, close, check,
// plus, search, menu, alert, info, archive, trash, move, more, flag, clip,
// mail, pencil.
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
            "search": [[15.5, 15.5], [20, 20]],
            "archive": [[4, 5], [20, 5], [20, 9], [4, 9], [4, 5], null, [5, 9], [5, 19], [19, 19], [19, 9], null, [10, 13], [14, 13]],
            "trash": [[5, 7], [19, 7], null, [9, 7], [9, 4], [15, 4], [15, 7], null, [7, 7], [8, 20], [16, 20], [17, 7]],
            "move": [[3, 7], [10, 7], [12, 9], [21, 9], [21, 19], [3, 19], [3, 7], null, [9, 14], [15, 14], null, [13, 12], [15, 14], [13, 16]],
            "more": [[12, 5], [12, 5.1], null, [12, 12], [12, 12.1], null, [12, 19], [12, 19.1]],
            "flag": [[6, 21], [6, 4], null, [6, 5], [18, 5], [15, 9], [18, 13], [6, 13]],
            "clip": [[19, 11], [12, 18], [8, 18], [6, 16], [6, 12], [14, 4], [17, 4], [19, 6], [19, 9], [11, 17]],
            "mail": [[4, 6], [20, 6], [20, 18], [4, 18], [4, 6], null, [4, 6], [12, 13], [20, 6]],
            "pencil": [[4, 20], [5, 15], [16, 4], [20, 8], [9, 19], [4, 20]]
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
