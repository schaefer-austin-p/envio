import QtQuick 2.15
import QtQuick.Window 2.15

// The window's visibility for the Fullscreen setting (#383). The phone's
// part is `ut/PhoneFullscreen.qml`, loaded here only where it exists
// (Suru: not on the desktop, not in the tests); `source` is the hook the
// tests point at a stand-in. `available` says whether the switch belongs
// in Settings at all: on the phone, where fullscreen hides the indicator
// panel; on the desktop the window is left as the compositor has it.
// Main binds `visibility` to its window.
Item {
    id: root
    visible: false

    // The setting, from the core.
    property bool fullscreen: false
    property url source: Qt.resolvedUrl("ut/PhoneFullscreen.qml")

    readonly property bool available: loader.status === Loader.Ready
    readonly property int visibility: available && fullscreen
        ? loader.item.visibilityOn : Window.AutomaticVisibility

    Loader {
        id: loader
        source: root.source
    }
}
