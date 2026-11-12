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

    // Settings, bound from EnvioCore in Main.qml: "carbon" or "airmail",
    // and "system", "light" or "dark".
    property string name: "carbon"
    property string darkMode: "system"
    readonly property bool airmail: name === "airmail"

    readonly property color systemWindow: loader.item ? loader.item.windowColour : palette.window
    // The system's switch. Only tests assign it: an assignment breaks the
    // binding to the system.
    property bool systemDark: systemWindow.hslLightness < 0.5
    readonly property bool dark: darkMode === "system" ? systemDark : darkMode === "dark"

    // Fonts ship once, inside the app (qrc from third_party/fonts, all OFL, licences included); Ubuntu is the
    // fallback while one is missing.
    readonly property string fontUi: ui.name !== "" ? ui.name : "Ubuntu"
    readonly property string fontMono: mono.name !== "" ? mono.name : "Ubuntu Mono"
    // Titles: Lora in Airmail, IBM Plex Mono in Carbon.
    readonly property string fontTitle: airmail ? (lora.name !== "" ? lora.name : "Ubuntu") : fontMono
    // Hosts, link parts and the certificate fingerprint: mono in both themes,
    // so look-alike characters (I/l, 0/O, rn/m) stay distinct where a real
    // address is shown. Pages read this, never fontMono (qml-lint).
    readonly property string fontAddress: fontMono
    // Small labels, meta, dates and sizes: mono in Carbon, Inter in Airmail.
    readonly property string fontLabel: airmail ? fontUi : fontMono
    // Reading and writing text (reader body, compose body, quote): Inter in
    // Carbon, Lora in Airmail (Inter while Lora fails to load).
    readonly property string fontBody: airmail ? (lora.name !== "" ? lora.name : fontUi) : fontUi
    readonly property real bodySize: 16
    readonly property real bodyLineHeight: airmail ? 1.6 : 1.0
    // Section labels ("ATTACHMENTS"): separate fields, as a QML font has no
    // case or spacing in a value of its own.
    readonly property string eyebrowFamily: airmail ? fontUi : fontMono
    readonly property real eyebrowSize: airmail ? 12 : 11
    readonly property int eyebrowWeight: airmail ? Font.DemiBold : Font.Normal
    readonly property bool eyebrowCaps: true
    readonly property real eyebrowSpacing: airmail ? 1.2 : 0.6

    // Grid unit in px: Suru's on Ubuntu Touch, 8 elsewhere.
    readonly property real gridUnit: grid.item ? grid.item.px : 8
    function gu(n) { return Math.round(n * gridUnit) }
    // The least height of anything tappable (at least 44 px).
    readonly property real touch: Math.max(44, gu(6))
    // Icon sizes: the regular one (buttons, bars) and the small one (inside
    // a row, a chip or a banner).
    readonly property real iconSize: gu(3)
    readonly property real iconSmall: gu(2)
    readonly property real iconButton: gu(2.5)

    // Top bar (boards): a touch-high button row under 12 px of padding, no
    // hairline. Titles: Carbon 20 mono, Airmail 26 Lora, both semibold; the
    // compose bar's are 18 and 22.
    readonly property real barPadTop: gu(1.5)
    readonly property real barHeight: touch + barPadTop
    readonly property font barTitleFont: Qt.font({ "family": fontTitle, "pixelSize": airmail ? 26 : 20,
                                                   "weight": Font.DemiBold })
    readonly property font barTitleCompactFont: Qt.font({ "family": fontTitle, "pixelSize": airmail ? 22 : 18,
                                                          "weight": Font.DemiBold })

    // Shapes (boards: airmail-prompts, airmail-compose, carbon-*). Carbon
    // keeps what the components drew before; Airmail's controls are pills.
    // Buttons: 52 px high in both themes at gu 8. The Toast is a pill in
    // Airmail by its own height / 2 (it is touch + gu(1) high).
    readonly property real controlHeight: airmail ? Math.max(touch, 52) : gu(6.5)
    readonly property real controlRadius: airmail ? controlHeight / 2 : gu(1)
    // A pill drawn 44 px high (the compose bar's Send); Airmail's is round.
    readonly property real compactControlHeight: 44
    readonly property real compactControlRadius: airmail ? compactControlHeight / 2 : gu(1)
    // The pinned footer (PageFooter, board): 10 px above its button, 16 below.
    readonly property real footerPadTop: 10
    readonly property real footerPadBottom: 16
    // A button's label: Inter 15 semibold in Airmail, as before in Carbon.
    readonly property font controlFont: Qt.font({ "family": fontLabel, "pixelSize": 15,
                                                  "weight": airmail ? Font.DemiBold : Font.Medium })
    readonly property real fieldRadius: airmail ? 10 : gu(1)
    // Chip is an underline tab in Airmail (no rounded box), so the Carbon
    // value stays; the board's 16 is the recipient chip, already a pill.
    readonly property real chipRadius: 6
    readonly property real dialogRadius: airmail ? 20 : gu(1.5)
    readonly property real sheetRadius: airmail ? 20 : gu(2)
    // Airmail's red and blue ring around every page, Sheet and dialog: its
    // band is frameInset wide between the outer and inner radius (the inner
    // one is the visible corner of a Sheet or dialog). Carbon has none.
    readonly property real frameInset: airmail ? 10 : 0
    // The ring's outer radius at the bottom corners, which meet the phone's
    // rounded display corners.
    readonly property real frameOuterRadius: airmail ? 30 : 0
    // 0: the top edge meets the system bar (indicator or title bar), straight.
    readonly property real frameTopOuterRadius: 0
    readonly property real frameInnerRadius: airmail ? 20 : 0
    // Cards (a found-at-host card, the reader's body card) and popup menus.
    readonly property real cardRadius: airmail ? 10 : gu(1)
    readonly property real menuRadius: airmail ? 10 : gu(1.5)
    // The theme choice cards in Settings.
    readonly property real optionRadius: airmail ? 10 : 6
    // The selection box in a message row (the unread dot is a circle).
    readonly property real checkRadius: 4
    // Avatars: circles in Carbon, a small-cornered square in Airmail.
    function avatarRadius(size) { return airmail ? 4 : size / 2 }
    // Carbon's avatar circle; Airmail's perforated stamp is 40 x 48 (boards).
    readonly property real avatarSize: gu(5.5)
    readonly property real stampWidth: 40
    readonly property real stampHeight: 48
    // The initials in an avatar: 17 px in a stamp, 34% of the circle in Carbon.
    function stampInitialSize(size) { return airmail ? 17 : size * 0.34 }
    // Airmail's stamp stands in for the unread dot; unread rows are semibold.
    readonly property bool showUnreadDot: !airmail
    readonly property int unreadWeight: Font.DemiBold

    readonly property var roles: ({
        "carbon": {
            "light": { "ground": "#F8F6F1", "surface": "#FFFFFF", "text": "#1E2A38", "heading": "#1E2A38",
                       "secondary": "#5A5D62", "hairline": "#DAD7CE", "field": "#C9C6BD", "accent": "#B8342C",
                       "primary": "#1E2A38", "primaryText": "#F8F6F1", "tintA": "#F4E3E1", "tintB": "#E3E6EA",
                       "selected": "#ECE9E1", "scrim": Qt.rgba(30 / 255, 42 / 255, 56 / 255, 0.45),
                       "chevron": "#5A5D62", "chevronBlue": "#5A5D62" },
            "dark":  { "ground": "#15181D", "surface": "#1F242B", "text": "#E8E5DE", "heading": "#E8E5DE",
                       "secondary": "#A2A5AA", "hairline": "#2E343C", "field": "#3A4049", "accent": "#E36A60",
                       "primary": "#E8E5DE", "primaryText": "#15181D", "tintA": "#3A2526", "tintB": "#26303B",
                       "selected": "#232932", "scrim": Qt.rgba(0, 0, 0, 0.55),
                       "chevron": "#A2A5AA", "chevronBlue": "#A2A5AA" }
        },
        "airmail": {
            "light": { "ground": "#FBF8F1", "surface": "#FFFFFF", "text": "#22303F", "heading": "#1F3A5F",
                       "secondary": "#5D6A78", "hairline": "#DDD5C4", "field": "#CFC6B1", "accent": "#B23A33",
                       "primary": "#1F3A5F", "primaryText": "#FBF8F1", "tintA": "#F3E2DF", "tintB": "#E0E7F0",
                       "selected": "#F1EADB", "scrim": Qt.rgba(34 / 255, 48 / 255, 63 / 255, 0.45),
                       "chevron": "#B23A33", "chevronBlue": "#1F3A5F" },
            "dark":  { "ground": "#141C27", "surface": "#1C2633", "text": "#ECE5D6", "heading": "#A9C1E3",
                       "secondary": "#A3AEBB", "hairline": "#2C3747", "field": "#3A475A", "accent": "#E58C84",
                       "primary": "#A9C1E3", "primaryText": "#141C27", "tintA": "#3A2427", "tintB": "#1F2D42",
                       "selected": "#1F2A38", "scrim": Qt.rgba(0, 0, 0, 0.55),
                       "chevron": "#C4473F", "chevronBlue": "#5B7FB3" }
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
    // Links in a message body: no board shows one, so Carbon takes accent and
    // Airmail its heading blue (readable on the dark ground too).
    readonly property color link: airmail ? heading : accent
    readonly property color primary: current.primary
    readonly property color primaryText: current.primaryText
    readonly property color tintA: current.tintA
    readonly property color tintB: current.tintB
    readonly property color selected: current.selected
    readonly property color scrim: current.scrim
    // Airmail's red and blue border; in Carbon the disclosure chevron.
    readonly property color chevron: current.chevron
    readonly property color chevronBlue: current.chevronBlue

    FontLoader { id: ui; source: "../../third_party/fonts/Inter/Inter-Regular.ttf" }
    FontLoader { source: "../../third_party/fonts/Inter/Inter-Medium.ttf" }
    FontLoader { source: "../../third_party/fonts/Inter/Inter-SemiBold.ttf" }
    FontLoader { id: mono; source: "../../third_party/fonts/IBMPlexMono/IBMPlexMono-Regular.ttf" }
    FontLoader { source: "../../third_party/fonts/IBMPlexMono/IBMPlexMono-Medium.ttf" }
    FontLoader { source: "../../third_party/fonts/IBMPlexMono/IBMPlexMono-SemiBold.ttf" }
    FontLoader { id: lora; source: "../../third_party/fonts/Lora/Lora-Regular.ttf" }
    FontLoader { source: "../../third_party/fonts/Lora/Lora-SemiBold.ttf" }

    Loader {
        id: loader
        source: "ut/SystemColours.qml"
    }
    Loader { id: grid; source: "ut/GridUnit.qml" }
}
