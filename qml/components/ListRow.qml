import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A list row: tap opens (`clicked`), long-press selects (`pressAndHold`).
// There is deliberately no swipe: Ubuntu Touch keeps the edge swipes for the
// launcher and the app switcher. Row content goes in as children.
AbstractButton {
    id: root
    required property Theme theme
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
}
