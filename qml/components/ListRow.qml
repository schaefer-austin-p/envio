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
    // A sideways drag is not a tap. A passive handler watches the finger;
    // once it travels past the drag distance the row is disabled and enabled
    // again, which drops the button's grab so it cancels and emits no
    // `clicked`. It never takes a grab itself, so press-and-hold and the
    // list's vertical scroll are untouched. Every row has this, so a swipe
    // (an edge gesture gone slightly wrong) opens nothing.
    PointHandler {
        id: swipeWatch
        property bool cut: false
        onActiveChanged: if (active) cut = false
        onPointChanged: {
            if (!active || cut) return
            if (Math.abs(point.position.x - point.pressPosition.x) > Qt.styleHints.startDragDistance) {
                cut = true
                root.enabled = false
                root.enabled = true
            }
        }
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
