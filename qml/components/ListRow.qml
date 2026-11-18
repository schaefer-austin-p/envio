import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A list row: tap opens (`clicked`), long-press selects (`pressAndHold`).
// There is deliberately no swipe: Ubuntu Touch keeps the edge swipes for the
// launcher and the app switcher. Row content goes in as children. A row that
// opens a page or a picker sets `chevron` (pointing `chevronDirection`,
// "forward" or "down") and keeps its content clear of the trailing
// `chevronSpace`.
AbstractButton {
    id: root
    required property Theme theme
    property bool chevron: false
    property string chevronDirection: "forward"
    readonly property real chevronSpace: theme.gu(6)
    property bool selected: false
    property bool separator: true
    default property alias content: holder.data

    implicitHeight: Math.max(theme.touch, holder.childrenRect.height + theme.gu(3.5))
    implicitWidth: parent ? parent.width : theme.gu(40)

    background: Rectangle {
        color: root.selected || root.down ? root.theme.selected : "transparent"
        Rectangle {
            visible: root.separator
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: root.theme.hairline
        }
    }
    contentItem: Item {
        id: holder
        implicitHeight: childrenRect.height
    }
    // A sideways drag is not a tap. A horizontal-only DragHandler with no
    // target moves nothing; once the finger travels past the drag distance it
    // takes the touch point's grab, the button's press is cancelled and no
    // `clicked` follows. Vertical drags never activate it, so the list's
    // scroll and press-and-hold are untouched, and the row's own `enabled`
    // is not written (a bound `enabled` keeps working). Every row has this,
    // so a swipe (an edge gesture gone slightly wrong) opens nothing.
    DragHandler {
        id: swipeWatch
        target: null
        yAxis.enabled: false
    }
    // Built only for a row that has one, so a plain row carries no chevron.
    Loader {
        active: root.chevron
        anchors.right: parent.right
        anchors.rightMargin: root.theme.gu(2)
        anchors.verticalCenter: parent.verticalCenter
        sourceComponent: Chevron {
            theme: root.theme
            direction: root.chevronDirection
        }
    }
}
