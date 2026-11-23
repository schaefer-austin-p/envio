import QtQuick 2.15
import QtQuick.Window 2.15

// A stand-in for platform/ut/PhoneFullscreen.qml: the same surface, loadable
// anywhere.
Item {
    readonly property int visibilityOn: Window.FullScreen
}
