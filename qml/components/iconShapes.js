.pragma library

// Icon shapes on a 24 grid, drawn with a 2 px stroke, round caps and joins.
// Hoisted here so Icon.qml's onPaint does not rebuild them on every paint.
//
// Most paths are Lucide 0.460.0 (https://lucide.dev, ISC; see NOTICES.md),
// each shape converted to path data. Arc flags stay space-separated: Qt's
// path parser misreads the packed "0 01" form. The few hand-drawn icons
// (clip, clock, pin, person, warning, lock) keep their own shapes.

// Short names the app uses for Lucide icons of another name.
var alias = {
    "trash": "trash-2",
    "move": "folder-input",
    "mailopen": "mail-open",
    "external": "external-link"
}

// Icons drawn as point lists: [x, y] is a point, null starts a new subpath.
var points = {
    "back": [[15, 5], [8, 12], [15, 19]],
    "forward": [[9, 5], [16, 12], [9, 19]],
    "down": [[5, 9], [12, 16], [19, 9]],
    "close": [[6, 6], [18, 18], null, [18, 6], [6, 18]],
    "plus": [[12, 5], [12, 19], null, [5, 12], [19, 12]],
    "menu": [[4, 6], [20, 6], null, [4, 12], [20, 12], null, [4, 18], [20, 18]],
    "alert": [[12, 6], [12, 13], null, [12, 17], [12, 17.1]],
    "info": [[12, 11], [12, 17], null, [12, 7], [12, 7.1]],
    "search": [[15.5, 15.5], [20, 20]],
    "more": [[12, 5], [12, 5.1], null, [12, 12], [12, 12.1], null, [12, 19], [12, 19.1]]
}

// Rings added to a point list: [cx, cy, r].
var rings = {
    "search": [11, 11, 6.5],
    "alert": [12, 12, 9.5],
    "info": [12, 12, 9.5]
}

// Icons as SVG path strings.
var svg = {
    "clip": "M20 11.5 l-8.2 8.2 a5 5 0 0 1 -7.1 -7.1 l8.5 -8.5 a3.3 3.3 0 0 1 4.7 4.7 l-8.5 8.5 a1.7 1.7 0 0 1 -2.4 -2.4 l7.8 -7.8",
    "clock": "M3 12 a9 9 0 1 0 18 0 a9 9 0 1 0 -18 0 M12 7 v5 l3 2",
    "pin": "M12 21 s7 -6 7 -11 a7 7 0 1 0 -14 0 c0 5 7 11 7 11 z M9.5 10 a2.5 2.5 0 1 0 5 0 a2.5 2.5 0 1 0 -5 0",
    "person": "M8 8 a4 4 0 1 0 8 0 a4 4 0 1 0 -8 0 M4 21 c0 -4 4 -6 8 -6 s8 2 8 6",
    "warning": "M12 3 L22 21 L2 21 z M12 10 L12 15 M12 18 L12 18.5",
    "lock": "M7 11 L17 11 a2 2 0 0 1 2 2 L19 19 a2 2 0 0 1 -2 2 L7 21 a2 2 0 0 1 -2 -2 L5 13 a2 2 0 0 1 2 -2 z M8 11 L8 8 a4 4 0 0 1 8 0 L16 11",
    "offline": "m2 2 20 20 M5.782 5.782A7 7 0 0 0 9 19h8.5a4.5 4.5 0 0 0 1.307-.193 "
             + "M21.532 16.5A4.5 4.5 0 0 0 17.5 10h-1.79A7.008 7.008 0 0 0 10 5.07",
    // Upload with a slash (a draft not on the server yet); drawn here, not Lucide.
    "uploadoff": "M12 15 V4 M8 8 l4 -4 4 4 M4 14 v6 h16 v-6 M4 4 l16 16",
    "pencil": "M 21.174 6.812 a 1 1 0 0 0 -3.986 -3.987 L 3.842 16.174 a 2 2 0 0 0 -0.5 0.83 l -1.321 4.352 a 0.5 0.5 0 0 0 0.623 0.622 l 4.353 -1.32 a 2 2 0 0 0 0.83 -0.497 z M 15 5 l 4 4",
    "key-round": "M 2.586 17.414 A 2 2 0 0 0 2 18.828 V 21 a 1 1 0 0 0 1 1 h 3 a 1 1 0 0 0 1 -1 v -1 a 1 1 0 0 1 1 -1 h 1 a 1 1 0 0 0 1 -1 v -1 a 1 1 0 0 1 1 -1 h 0.172 a 2 2 0 0 0 1.414 -0.586 l 0.814 -0.814 a 6.5 6.5 0 1 0 -4 -4 z M 16 7.5 a 0.5 0.5 0 1 0 1 0 a 0.5 0.5 0 1 0 -1 0",
    "refresh-cw": "M 3 12 a 9 9 0 0 1 9 -9 a 9.75 9.75 0 0 1 6.74 2.74 L 21 8 M 21 3 v 5 h -5 M 21 12 a 9 9 0 0 1 -9 9 a 9.75 9.75 0 0 1 -6.74 -2.74 L 3 16 M 8 16 H 3 v 5",
    "log-in": "M 15 3 h 4 a 2 2 0 0 1 2 2 v 14 a 2 2 0 0 1 -2 2 h -4 M 10 17 L 15 12 L 10 7 M 15 12 L 3 12",
    "trash-2": "M 3 6 h 18 M 19 6 v 14 c 0 1 -1 2 -2 2 H 7 c -1 0 -2 -1 -2 -2 V 6 M 8 6 V 4 c 0 -1 1 -2 2 -2 h 4 c 1 0 2 1 2 2 v 2 M 10 11 L 10 17 M 14 11 L 14 17",
    "mail-open": "M 21.2 8.4 c 0.5 0.38 0.8 0.97 0.8 1.6 v 10 a 2 2 0 0 1 -2 2 H 4 a 2 2 0 0 1 -2 -2 V 10 a 2 2 0 0 1 0.8 -1.6 l 8 -6 a 2 2 0 0 1 2.4 0 l 8 6 Z M 22 10 l -8.97 5.7 a 1.94 1.94 0 0 1 -2.06 0 L 2 10",
    "mail": "M 4 4 h 16 a 2 2 0 0 1 2 2 v 12 a 2 2 0 0 1 -2 2 h -16 a 2 2 0 0 1 -2 -2 v -12 a 2 2 0 0 1 2 -2 z M 22 7 l -8.97 5.7 a 1.94 1.94 0 0 1 -2.06 0 L 2 7",
    "flag": "M 4 15 s 1 -1 4 -1 s 5 2 8 2 s 4 -1 4 -1 V 3 s -1 1 -4 1 s -5 -2 -8 -2 s -4 1 -4 1 z M 4 22 L 4 15",
    "flag-off": "M 8 2 c 3 0 5 2 8 2 s 4 -1 4 -1 v 11 M 4 22 V 4 M 4 15 s 1 -1 4 -1 s 5 2 8 2 M 2 2 L 22 22",
    "reply-all": "M 7 17 L 2 12 L 7 7 M 12 17 L 7 12 L 12 7 M 22 18 v -2 a 4 4 0 0 0 -4 -4 H 7",
    "folder-input": "M 2 9 V 5 a 2 2 0 0 1 2 -2 h 3.9 a 2 2 0 0 1 1.69 0.9 l 0.81 1.2 a 2 2 0 0 0 1.67 0.9 H 20 a 2 2 0 0 1 2 2 v 10 a 2 2 0 0 1 -2 2 H 4 a 2 2 0 0 1 -2 -2 v -1 M 2 13 h 10 M 9 16 l 3 -3 l -3 -3",
    "user-plus": "M 16 21 v -2 a 4 4 0 0 0 -4 -4 H 6 a 4 4 0 0 0 -4 4 v 2 M 5 7 a 4 4 0 1 0 8 0 a 4 4 0 1 0 -8 0 M 19 8 L 19 14 M 22 11 L 16 11",
    "inbox": "M 22 12 L 16 12 L 14 15 L 10 15 L 8 12 L 2 12 M 5.45 5.11 L 2 12 v 6 a 2 2 0 0 0 2 2 h 16 a 2 2 0 0 0 2 -2 v -6 l -3.45 -6.89 A 2 2 0 0 0 16.76 4 H 7.24 a 2 2 0 0 0 -1.79 1.11 z",
    "send": "M 14.536 21.686 a 0.5 0.5 0 0 0 0.937 -0.024 l 6.5 -19 a 0.496 0.496 0 0 0 -0.635 -0.635 l -19 6.5 a 0.5 0.5 0 0 0 -0.024 0.937 l 7.93 3.18 a 2 2 0 0 1 1.112 1.11 z M 21.854 2.147 l -10.94 10.939",
    "file-pen": "M 12.5 22 H 18 a 2 2 0 0 0 2 -2 V 7 l -5 -5 H 6 a 2 2 0 0 0 -2 2 v 9.5 M 14 2 v 4 a 2 2 0 0 0 2 2 h 4 M 13.378 15.626 a 1 1 0 1 0 -3.004 -3.004 l -5.01 5.012 a 2 2 0 0 0 -0.506 0.854 l -0.837 2.87 a 0.5 0.5 0 0 0 0.62 0.62 l 2.87 -0.837 a 2 2 0 0 0 0.854 -0.506 z",
    "archive": "M 3 3 h 18 a 1 1 0 0 1 1 1 v 3 a 1 1 0 0 1 -1 1 h -18 a 1 1 0 0 1 -1 -1 v -3 a 1 1 0 0 1 1 -1 z M 4 8 v 11 a 2 2 0 0 0 2 2 h 12 a 2 2 0 0 0 2 -2 V 8 M 10 12 h 4",
    "shield-alert": "M 20 13 c 0 5 -3.5 7.5 -7.66 8.95 a 1 1 0 0 1 -0.67 -0.01 C 7.5 20.5 4 18 4 13 V 6 a 1 1 0 0 1 1 -1 c 2 0 4.5 -1.2 6.24 -2.72 a 1.17 1.17 0 0 1 1.52 0 C 14.51 3.81 17 5 19 5 a 1 1 0 0 1 1 1 z M 12 8 v 4 M 12 16 h 0.01",
    "folder": "M 20 20 a 2 2 0 0 0 2 -2 V 8 a 2 2 0 0 0 -2 -2 h -7.9 a 2 2 0 0 1 -1.69 -0.9 L 9.6 3.9 A 2 2 0 0 0 7.93 3 H 4 a 2 2 0 0 0 -2 2 v 13 a 2 2 0 0 0 2 2 Z",
    "external-link": "M 15 3 h 6 v 6 M 10 14 L 21 3 M 18 13 v 6 a 2 2 0 0 1 -2 2 H 5 a 2 2 0 0 1 -2 -2 V 8 a 2 2 0 0 1 2 -2 h 6",
    "download": "M 21 15 v 4 a 2 2 0 0 1 -2 2 H 5 a 2 2 0 0 1 -2 -2 v -4 M 7 10 L 12 15 L 17 10 M 12 15 L 12 3",
    "share-2": "M 15 5 a 3 3 0 1 0 6 0 a 3 3 0 1 0 -6 0 M 3 12 a 3 3 0 1 0 6 0 a 3 3 0 1 0 -6 0 M 15 19 a 3 3 0 1 0 6 0 a 3 3 0 1 0 -6 0 M 8.59 13.51 L 15.42 17.49 M 15.41 6.51 L 8.59 10.49",
    "copy": "M 10 8 h 10 a 2 2 0 0 1 2 2 v 10 a 2 2 0 0 1 -2 2 h -10 a 2 2 0 0 1 -2 -2 v -10 a 2 2 0 0 1 2 -2 z M 4 16 c -1.1 0 -2 -0.9 -2 -2 V 4 c 0 -1.1 0.9 -2 2 -2 h 10 c 1.1 0 2 0.9 2 2",
    "circle-user-round": "M 18 20 a 6 6 0 0 0 -12 0 M 8 10 a 4 4 0 1 0 8 0 a 4 4 0 1 0 -8 0 M 2 12 a 10 10 0 1 0 20 0 a 10 10 0 1 0 -20 0",
    "save": "M 15.2 3 a 2 2 0 0 1 1.4 0.6 l 3.8 3.8 a 2 2 0 0 1 0.6 1.4 V 19 a 2 2 0 0 1 -2 2 H 5 a 2 2 0 0 1 -2 -2 V 5 a 2 2 0 0 1 2 -2 z M 17 21 v -7 a 1 1 0 0 0 -1 -1 H 8 a 1 1 0 0 0 -1 1 v 7 M 7 3 v 4 a 1 1 0 0 0 1 1 h 7",
    "scissors": "M 3 6 a 3 3 0 1 0 6 0 a 3 3 0 1 0 -6 0 M 8.12 8.12 L 12 12 M 20 4 L 8.12 15.88 M 3 18 a 3 3 0 1 0 6 0 a 3 3 0 1 0 -6 0 M 14.8 14.8 L 20 20",
    "clipboard-paste": "M 15 2 H 9 a 1 1 0 0 0 -1 1 v 2 c 0 0.6 0.4 1 1 1 h 6 c 0.6 0 1 -0.4 1 -1 V 3 c 0 -0.6 -0.4 -1 -1 -1 Z M 8 4 H 6 a 2 2 0 0 0 -2 2 v 14 a 2 2 0 0 0 2 2 h 12 a 2 2 0 0 0 2 -2 M 16 4 h 2 a 2 2 0 0 1 2 2 v 2 M 11 14 h 10 M 17 10 l 4 4 l -4 4",
    "text-select": "M 5 3 a 2 2 0 0 0 -2 2 M 19 3 a 2 2 0 0 1 2 2 M 21 19 a 2 2 0 0 1 -2 2 M 5 21 a 2 2 0 0 1 -2 -2 M 9 3 h 1 M 9 21 h 1 M 14 3 h 1 M 14 21 h 1 M 3 9 v 1 M 21 9 v 1 M 3 14 v 1 M 21 14 v 1 M 7 8 L 15 8 M 7 12 L 17 12 M 7 16 L 13 16",
    "square-check": "M 5 3 h 14 a 2 2 0 0 1 2 2 v 14 a 2 2 0 0 1 -2 2 h -14 a 2 2 0 0 1 -2 -2 v -14 a 2 2 0 0 1 2 -2 z M 9 12 l 2 2 l 4 -4",
    "check": "M 20 6 L 9 17 l -5 -5"
}

// Every name Icon.qml draws.
var names = Object.keys(points).concat(Object.keys(svg), Object.keys(alias))
