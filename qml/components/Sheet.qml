import QtQuick 2.15
import QtQuick.Controls 2.15 as QC
import "../platform"
import "../bindle/Platform" as Platform

// A bottom sheet over the scrim, with a grab handle. Children go below the
// handle; a tap on the scrim closes it.
QC.Popup {
    id: root
    required property Theme theme
    property string title: ""
    default property alias content: column.data

    parent: QC.Overlay.overlay
    modal: true
    closePolicy: QC.Popup.CloseOnEscape | QC.Popup.CloseOnPressOutside
    // The keyboard in window coordinates (bindle KeyboardInset; tests set
    // it). Where it covers the screen's bottom the sheet rests on its top
    // edge, not behind it.
    readonly property Platform.KeyboardInset keyboardArea: Platform.KeyboardInset {
        id: keyboardArea
        target: root.parent
    }
    property alias keyboard: keyboardArea.keyboard
    readonly property real keyboardInset: keyboardArea.inset
    // In Airmail the sheet lies inside the screen ring's inner area (the ring
    // is the shell's, drawn once); the keyboard, when up, is its floor.
    readonly property real ringInset: keyboardInset > 0 ? 0 : theme.frameInset
    x: theme.frameInset
    width: parent ? parent.width - 2 * theme.frameInset : 0
    // Follow the content as it grows after open (a popup left to itself kept
    // its first height on the phone and rested far too low).
    // Capped at the window less the ring's top band; past that the content
    // scrolls (#83), so the last rows stay reachable.
    height: parent ? Math.min(implicitHeight, parent.height - theme.frameInset) : implicitHeight
    y: parent ? Math.max(theme.frameInset, parent.height - height - keyboardInset - ringInset) : 0
    padding: 0

    // The scrim covers the inner area only: the ring stays undimmed.
    QC.Overlay.modal: Item {
        Rectangle {
            anchors.fill: parent
            anchors.margins: root.theme.frameInset
            radius: root.theme.frameInnerRadius
            color: root.theme.scrim
        }
    }

    background: Rectangle {
        color: root.theme.ground
        radius: root.theme.sheetRadius
        // Square off the bottom corners: the sheet sits on the screen edge.
        // (Airmail: its bottom corners follow the ring's inner corner.)
        Rectangle {
            visible: !root.theme.airmail
            anchors.bottom: parent.bottom
            width: parent.width
            height: parent.radius
            color: parent.color
        }
    }

    contentItem: Flickable {
        id: flick
        objectName: "sheetFlick"
        // Only a sheet that scrolls clips; one that fits looks as before.
        clip: interactive
        implicitHeight: column.implicitHeight
        contentWidth: width
        contentHeight: column.implicitHeight
        // A sheet that fits stays still.
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick

        Column {
            id: column
            width: flick.width
            spacing: root.theme.gu(1)
            topPadding: root.theme.gu(1.25)
            bottomPadding: root.theme.gu(2)
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: root.theme.gu(5)
                height: 4
                radius: height / 2
                color: root.theme.field
            }
            Text {
                textFormat: Text.PlainText
                visible: root.title !== ""
                x: root.theme.gu(2.5)
                width: parent.width - root.theme.gu(5)
                text: root.title
                elide: Text.ElideRight
                color: root.theme.heading
                font.family: root.theme.fontTitle
                font.pixelSize: 18
                font.weight: Font.DemiBold
            }
        }
    }
}
