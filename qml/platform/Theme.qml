import QtQuick 2.15
import QtQuick.Controls 2.15

// The app's colour roles (docs/10-design.md, "Colour roles"), following
// the system's light/dark switch live. On Ubuntu Touch Suru answers
// (ut/SystemColours.qml); where Suru is missing (desktop) the load fails
// and the palette's window lightness decides. A Control, as Qt 5's Item
// has no palette.
Control {
    id: root
    visible: false

    // Settings: the alternative theme.
    property bool airmail: false
    // null follows the system; true or false forces it (tests, screenshots).
    property var darkOverride: null

    readonly property color systemWindow: loader.item ? loader.item.windowColour : palette.window
    readonly property bool dark: darkOverride !== null ? darkOverride : systemWindow.hslLightness < 0.5

    readonly property var roles: ({
        "carbon": {
            "light": { "ground": "#F8F6F1", "surface": "#FFFFFF", "text": "#1E2A38", "heading": "#1E2A38",
                       "secondary": "#5A5D62", "hairline": "#DAD7CE", "field": "#C9C6BD", "accent": "#B8342C",
                       "primary": "#1E2A38", "primaryText": "#F8F6F1", "tintA": "#F4E3E1", "tintB": "#E3E6EA",
                       "selected": "#ECE9E1" },
            "dark":  { "ground": "#15181D", "surface": "#1F242B", "text": "#E8E5DE", "heading": "#E8E5DE",
                       "secondary": "#A2A5AA", "hairline": "#2E343C", "field": "#3A4049", "accent": "#E36A60",
                       "primary": "#E8E5DE", "primaryText": "#15181D", "tintA": "#3A2526", "tintB": "#26303B",
                       "selected": "#232932" }
        },
        "airmail": {
            "light": { "ground": "#FBF8F1", "surface": "#FFFFFF", "text": "#22303F", "heading": "#1F3A5F",
                       "secondary": "#5D6A78", "hairline": "#DDD5C4", "field": "#CFC6B1", "accent": "#B23A33",
                       "primary": "#1F3A5F", "primaryText": "#FBF8F1", "tintA": "#F3E2DF", "tintB": "#E0E7F0",
                       "selected": "#F1EADB" },
            "dark":  { "ground": "#141C27", "surface": "#1C2633", "text": "#ECE5D6", "heading": "#A9C1E3",
                       "secondary": "#A3AEBB", "hairline": "#2C3747", "field": "#3A475A", "accent": "#E58C84",
                       "primary": "#A9C1E3", "primaryText": "#141C27", "tintA": "#3A2427", "tintB": "#1F2D42",
                       "selected": "#1F2A38" }
        }
    })
    readonly property var current: roles[airmail ? "airmail" : "carbon"][dark ? "dark" : "light"]

    readonly property color ground: current.ground
    readonly property color surface: current.surface
    readonly property color text: current.text
    readonly property color heading: current.heading
    readonly property color secondary: current.secondary
    readonly property color hairline: current.hairline
    readonly property color field: current.field
    readonly property color accent: current.accent
    readonly property color primary: current.primary
    readonly property color primaryText: current.primaryText
    readonly property color tintA: current.tintA
    readonly property color tintB: current.tintB
    readonly property color selected: current.selected

    Loader {
        id: loader
        source: "ut/SystemColours.qml"
    }
}
