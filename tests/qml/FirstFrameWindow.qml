import QtQuick 2.15
import QtQuick.Window 2.15
import Envio 1.0
import "../../qml/platform"

// A window wired as Main.qml wires its own to the real EnvioCore (#463):
// the visibility from WindowMode (a phone session, with the stand-in for
// platform/ut/), the colour from Theme. It records what the window had when
// it was first shown, and counts every change after that.
Window {
    id: win
    width: 360
    height: 640
    visible: true
    visibility: mode.visibility
    color: theme.ground

    property alias core: core
    property bool snapped: false
    property int firstVisibility: -1
    property color firstColor: "transparent"
    property string firstTheme: ""
    property int changes: 0

    EnvioCore { id: core }
    WindowMode { id: mode; phoneSession: true; source: Qt.resolvedUrl("StubFullscreen.qml"); fullscreen: core.fullscreen }
    Theme {
        id: theme
        name: core.theme !== "" ? core.theme : "carbon"
        darkMode: core.darkMode !== "" ? core.darkMode : "system"
    }

    function snap() {
        if (snapped || !visible) return
        snapped = true
        firstVisibility = mode.visibility
        firstColor = theme.ground
        firstTheme = theme.name
    }
    onVisibleChanged: snap()
    Component.onCompleted: snap()

    // Anything the binding inputs do after the first show is a visible jump.
    Connections { target: mode; function onVisibilityChanged() { if (win.snapped) win.changes++ } }
    Connections { target: theme; function onNameChanged() { if (win.snapped) win.changes++ } }
    Connections { target: theme; function onDarkModeChanged() { if (win.snapped) win.changes++ } }
    onColorChanged: if (snapped) changes++
}
