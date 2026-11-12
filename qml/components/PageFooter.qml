import QtQuick 2.15
import QtQuick.Window 2.15
import "../platform"
import "keyboardRect.js" as KeyboardRect

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
    // The keyboard in window coordinates. Tests set it; on the phone it is
    // Qt's, in logical pixels (Ubuntu Touch reports device pixels).
    property rect keyboard: KeyboardRect.logical(Qt.inputMethod.keyboardRectangle,
        Screen.width, Screen.devicePixelRatio)
    // How far the keyboard covers the bottom of the parent.
    readonly property real lift: {
        if (keyboard.height <= 0 || !parent) return 0
        const bottom = parent.mapToItem(null, 0, parent.height).y
        return Math.max(0, Math.min(parent.height, bottom - keyboard.y))
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
    }
}
