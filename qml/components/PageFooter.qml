import QtQuick 2.15
import "../platform"
import "../bindle/Platform" as Platform

// The primary action pinned to the bottom of a page (board: a bottom bar):
// 10 px above the button, 16 below, the button controlHeight high, a hairline
// above in Carbon and none in Airmail. Children fill the width inside gu(2)
// side margins, as the inbox's "New message" does. Anchor a page's content to
// `top` so it ends above the footer. The footer rides on the on-screen
// keyboard (#61): it sits above Qt's keyboardRectangle, so the button stays
// visible and the page's KeyboardAvoider only sees the room that is left.
Item {
    id: root
    objectName: "pageFooter"
    required property Theme theme
    default property alias content: body.data
    // While `busy` a BusyLine with `busyText` takes the place of the content
    // (the same height, so nothing jumps); the content is hidden, not removed.
    property bool busy: false
    property string busyText: ""
    // The keyboard in window coordinates (bindle KeyboardInset). Tests set it.
    property alias keyboard: inset.keyboard
    // How far the keyboard covers the bottom of the parent.
    readonly property real lift: inset.inset

    Platform.KeyboardInset {
        id: inset
        target: root.parent
    }

    x: 0
    width: parent ? parent.width : 0
    y: (parent ? parent.height : 0) - height - lift
    height: theme.footerPadTop + theme.controlHeight + theme.footerPadBottom

    Rectangle { anchors.fill: parent; color: root.theme.ground }
    Rectangle {
        objectName: "footerHairline"
        width: parent.width
        height: 1
        visible: !root.theme.airmail
        color: root.theme.hairline
    }
    Item {
        id: body
        x: root.theme.gu(2)
        y: root.theme.footerPadTop
        width: parent.width - 2 * x
        height: root.theme.controlHeight
        visible: !root.busy
    }
    BusyLine {
        objectName: "footerBusy"
        theme: root.theme
        running: root.busy
        text: root.busyText
        x: body.x
        y: body.y
        width: body.width
        height: body.height
    }
}
