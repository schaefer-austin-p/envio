import QtQuick 2.15
import QtQuick.Window 2.15

// The window's visibility for the Fullscreen setting (#383). The phone's
// part is `ut/PhoneFullscreen.qml`; `source` is the hook the tests point at
// a stand-in. `available` says whether the switch belongs in Settings at
// all: only in a Lomiri session (`lomiriSession`, set by main.cpp from
// GRID_UNIT_PX), where fullscreen hides the indicator panel. A desktop,
// even one that has Suru installed, is left as the compositor has it.
// Main binds `visibility` to its window.
Item {
    id: root
    visible: false

    // The setting, from the core.
    property bool fullscreen: false
    // Whether this is the phone's session; tests set it.
    property bool phoneSession: typeof lomiriSession !== "undefined" && lomiriSession === true
    property url source: Qt.resolvedUrl("ut/PhoneFullscreen.qml")

    readonly property bool available: phoneSession && loader.status === Loader.Ready
    readonly property int visibility: available && fullscreen
        ? loader.item.visibilityOn : Window.AutomaticVisibility

    Loader {
        id: loader
        source: root.phoneSession ? root.source : ""
    }
}
