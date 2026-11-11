import QtQuick 2.15
import "../platform"

// Initials in a circle (Carbon) or a tinted stamp (Airmail). `initials` is
// computed by the caller; `blue` picks Airmail's blue tint over the red.
Rectangle {
    id: root
    required property Theme theme
    property string initials: ""
    property bool blue: false
    property real size: theme.gu(5.5)

    implicitWidth: size
    implicitHeight: size
    radius: theme.airmail ? 4 : size / 2
    color: theme.airmail ? (blue ? theme.tintB : theme.tintA) : theme.tintB

    Text {
        anchors.centerIn: parent
        text: root.initials
        color: root.theme.airmail ? (root.blue ? root.theme.heading : root.theme.accent) : root.theme.text
        font.family: root.theme.fontTitle
        font.pixelSize: root.size * 0.34
        font.weight: Font.DemiBold
    }
}
