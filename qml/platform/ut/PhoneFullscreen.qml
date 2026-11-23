import QtQuick 2.15
import QtQuick.Window 2.15
import QtQuick.Controls.Suru 2.2

// Ubuntu Touch: a fullscreen window hides Lomiri's indicator panel until a
// swipe down from the top edge brings it back. The Suru import is what
// makes this file load only on the phone (WindowMode probes for it).
Item {
    readonly property int visibilityOn: Window.FullScreen
    readonly property real gridUnit: Suru.units.gu(1)
}
