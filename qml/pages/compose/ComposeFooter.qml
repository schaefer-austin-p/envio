import QtQuick 2.15
import "../../platform"
import "../../components"
import "../../bindle/Platform" as Platform

// The line under the message (carbon-compose, airmail-compose): a check and
// "Draft saved 09:41" on the left once the draft has been written, and
// "Plain text" on the right. `savedAtMs` is the core's save time (Unix
// milliseconds, `draftStored`); 0 until the first save, when only the
// marker shows. The time is the locale's short time.
Item {
    id: root
    objectName: "composeFooter"
    required property Theme theme
    property real savedAtMs: 0
    // In the wide pane: the fields' 5 gu gutter, 10 px above, 16 below
    // (carbon-two-pane); on the phone 16 px sides, 8 above, 14 below.
    property bool wide: false
    readonly property real sideMargin: theme.gu(wide ? 5 : 2)
    readonly property real topMargin: theme.gu(wide ? 1.25 : 1)
    readonly property real bottomMargin: theme.gu(wide ? 2 : 1.75)

    readonly property string savedText: savedAtMs > 0
        ? qsTr("Draft saved %1").arg(Qt.formatTime(new Date(savedAtMs), Qt.locale().timeFormat(Locale.ShortFormat)))
        : ""

    // The keyboard in window coordinates (bindle KeyboardInset). Tests set it.
    property alias keyboard: inset.keyboard
    // How far the keyboard covers the bottom of the parent.
    readonly property real lift: inset.inset

    // Rides on the on-screen keyboard like PageFooter (#61): it sits above
    // Qt's keyboardRectangle, so the page's KeyboardAvoider only sees the
    // room that is left.
    Platform.KeyboardInset {
        id: inset
        target: root.parent
    }
    y: (parent ? parent.height : 0) - height - lift

    Rectangle { anchors.fill: parent; color: root.theme.ground }

    implicitHeight: marker.implicitHeight + topMargin + bottomMargin
    height: implicitHeight

    // "Plain text" keeps its room on the right; the left text takes the
    // rest and is elided, so a long translation never runs into it. Anchors
    // follow LayoutMirroring (RTL).
    Icon {
        id: check
        objectName: "savedCheck"
        theme: root.theme
        name: "check"
        size: 14
        color: root.theme.secondary
        visible: root.savedAtMs > 0
        anchors { left: parent.left; leftMargin: root.sideMargin; verticalCenter: marker.verticalCenter }
    }
    Text {
        objectName: "savedText"
        textFormat: Text.PlainText
        visible: root.savedAtMs > 0
        text: root.savedText
        elide: Text.ElideRight
        color: root.theme.secondary
        font.family: root.theme.fontLabel
        font.pixelSize: 12
        anchors {
            left: check.right; leftMargin: root.theme.gu(0.75)
            right: marker.left; rightMargin: root.theme.gu(1)
            verticalCenter: marker.verticalCenter
        }
    }
    Text {
        id: marker
        objectName: "plainText"
        textFormat: Text.PlainText
        anchors { right: parent.right; rightMargin: root.sideMargin; top: parent.top; topMargin: root.topMargin }
        text: qsTr("Plain text")
        color: root.theme.secondary
        font.family: root.theme.fontLabel
        font.pixelSize: 12
    }
}
