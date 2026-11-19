import QtQuick 2.15
import "../platform"

// A line icon drawn on a canvas in a theme colour, so no icon set (and no
// Suru) is needed. `name` is one of: back, forward, down, close, check,
// plus, send, search, menu, alert, info, archive, trash, move, more, flag,
// clip, mail, mailopen, pencil, offline, uploadoff.
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
            "mail": [[4, 6], [20, 6], [20, 18], [4, 18], [4, 6], null, [4, 6], [12, 13], [20, 6]],
            "send": [[4, 12], [20, 4], [14, 20], [11, 14], [4, 12], null, [11, 14], [20, 4]],
            "mailopen": [[4, 10], [12, 4], [20, 10], [20, 19], [4, 19], [4, 10], null, [4, 10], [12, 15], [20, 10]],
            "pencil": [[4, 20], [5, 15], [16, 4], [20, 8], [9, 19], [4, 20]]
        }
        // Icons with arcs are SVG path strings (same 24 grid). Arc flags stay
        // space-separated: Qt's path parser misreads the packed "0 01" form.
        const svg = {
            "clip": "M20 11.5 l-8.2 8.2 a5 5 0 0 1 -7.1 -7.1 l8.5 -8.5 a3.3 3.3 0 0 1 4.7 4.7 l-8.5 8.5 a1.7 1.7 0 0 1 -2.4 -2.4 l7.8 -7.8",
            "clock": "M3 12 a9 9 0 1 0 18 0 a9 9 0 1 0 -18 0 M12 7 v5 l3 2",
            "pin": "M12 21 s7 -6 7 -11 a7 7 0 1 0 -14 0 c0 5 7 11 7 11 z M9.5 10 a2.5 2.5 0 1 0 5 0 a2.5 2.5 0 1 0 -5 0",
            "person": "M8 8 a4 4 0 1 0 8 0 a4 4 0 1 0 -8 0 M4 21 c0 -4 4 -6 8 -6 s8 2 8 6",
            "warning": "M12 3 L22 21 L2 21 z M12 10 L12 15 M12 18 L12 18.5",
            "lock": "M7 11 L17 11 a2 2 0 0 1 2 2 L19 19 a2 2 0 0 1 -2 2 L7 21 a2 2 0 0 1 -2 -2 L5 13 a2 2 0 0 1 2 -2 z M8 11 L8 8 a4 4 0 0 1 8 0 L16 11",
            "external": "M14 4 L20 4 L20 10 M20 4 L11 13 M18 14 L18 20 L4 20 L4 6 L10 6",
            "uploadoff": "M12 15 V4 M8 8 l4 -4 4 4 M4 14 v6 h16 v-6 M4 4 l16 16",
            "offline": "m2 2 20 20 M5.782 5.782A7 7 0 0 0 9 19h8.5a4.5 4.5 0 0 0 1.307-.193 "
                     + "M21.532 16.5A4.5 4.5 0 0 0 17.5 10h-1.79A7.008 7.008 0 0 0 10 5.07"
        }
        if (svg[root.name] !== undefined) {
            ctx.path = svg[root.name]
            ctx.stroke()
            return
        }
        const pts = paths[root.name] || []
        ctx.beginPath()
        let start = true
        for (const p of pts) {
            if (p === null) { start = true; continue }
            if (start) ctx.moveTo(p[0], p[1]); else ctx.lineTo(p[0], p[1])
            start = false
        }
        // A ring starts its own subpath; arc() alone would join it to the
        // last point with a stray line.
        function ring(cx, cy, r) {
            ctx.moveTo(cx + r, cy)
            ctx.arc(cx, cy, r, 0, 2 * Math.PI)
        }
        if (root.name === "search") ring(11, 11, 6.5)
        if (root.name === "alert" || root.name === "info") ring(12, 12, 9.5)
        ctx.stroke()
    }
}
