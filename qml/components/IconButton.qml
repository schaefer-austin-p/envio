import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A 48 px square icon button (top bars). `text` is the accessible name.
AbstractButton {
    id: root
    required property Theme theme
    property string iconName: ""
    // The glyph turns once per 900 ms (as the pull indicator's disc) while true.
    property bool iconRotating: false

    implicitWidth: theme.touch
    implicitHeight: theme.touch
    opacity: enabled ? 1 : 0.45
    Accessible.role: Accessible.Button
    Accessible.name: text

    // The Control stretches its content to the button; the icon keeps its
    // own size, centred.
    contentItem: Item {
        Icon {
            id: glyph
            objectName: "icon"
            theme: root.theme
            name: root.iconName
            color: root.theme.text
            size: root.theme.iconSize
            anchors.centerIn: parent
            RotationAnimator {
                target: glyph
                from: 0
                to: 360
                duration: 900
                loops: Animation.Infinite
                // Finish the turn, so it stops upright (360 looks like 0)
                // without the GUI thread writing `rotation` against the
                // animator on the render thread.
                alwaysRunToEnd: true
                running: root.iconRotating && root.visible
            }
        }
    }
    background: Rectangle {
        radius: width / 2
        color: root.down ? root.theme.selected : "transparent"
    }
}
