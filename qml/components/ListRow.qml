import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// A list row: tap opens (`clicked`), long-press selects (`pressAndHold`).
// There is deliberately no swipe: Ubuntu Touch keeps the edge swipes for the
// launcher and the app switcher. Row content goes in as children. A row that
// opens a page or a picker sets `chevron` (pointing `chevronDirection`,
// "forward" or "down") and keeps its content clear of the trailing
// `chevronSpace`. A row with `iconName` (see Icon.qml) shows that icon at its
// left inset, centred on the row, and shifts its content right by the icon
// and the gap (60 px in a sheet); the row's height still comes from the
// content, with the 48 px minimum. `iconColor` is the text colour, or
// secondary on a disabled row; a destructive row sets it to accent.
AbstractButton {
    id: root
    required property Theme theme
    property bool chevron: false
    property string chevronDirection: "forward"
    readonly property real chevronSpace: theme.gu(6)
    property bool selected: false
    // The keyboard-focus row: the hover fill (the caller draws the outline).
    property bool cued: false
    property bool separator: true
    // The lighter inner rule the boards draw between message-list rows; every
    // other row keeps the hairline.
    property bool lightRule: false
    property string iconName: ""
    property real iconSize: theme.iconSize
    property real iconGap: theme.gu(2)
    property real iconInset: theme.gu(2.5)
    property color iconColor: enabled ? theme.text : theme.secondary
    // The icon item, or null for a row without one.
    readonly property Item iconItem: iconLoader.item
    default property alias content: holder.data

    implicitHeight: Math.max(theme.touch, holder.childrenRect.height + theme.gu(3.5))
    implicitWidth: parent ? parent.width : theme.gu(40)
    // Callers place content at the usual 20 px inset (x: gu(2.5)); the
    // padding moves it to 60 px (icon at 20, 24 wide, 16 gap) minus that
    // inset, so no caller does the sum.
    leftPadding: iconName !== "" ? iconSize + iconGap : 0

    background: Rectangle {
        color: root.selected || root.down ? root.theme.selected : (root.cued ? root.theme.hover : "transparent")
        Rectangle {
            objectName: "rowRule"
            visible: root.separator
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: root.lightRule ? root.theme.rowRule : root.theme.hairline
        }
    }
    contentItem: Item {
        id: holder
        implicitHeight: childrenRect.height
    }
    // Built only for a row that has an icon.
    Loader {
        id: iconLoader
        active: root.iconName !== ""
        x: root.iconInset
        anchors.verticalCenter: parent.verticalCenter
        sourceComponent: Icon {
            theme: root.theme
            name: root.iconName
            size: root.iconSize
            color: root.iconColor
        }
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
