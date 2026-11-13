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
    // A sideways drag is not a tap: once the finger travels past the drag
    // distance this handler takes the grab, the button cancels, and no
    // `clicked` follows. It moves nothing, and vertical drags still scroll.
    DragHandler {
        target: null
        xAxis.enabled: true
        yAxis.enabled: false
        dragThreshold: Qt.styleHints.startDragDistance
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
