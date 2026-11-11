import QtQuick 2.15
import QtQuick.Controls 2.15 as QC
import "../platform"

// A bottom sheet over the scrim, with a grab handle. Children go below the
// handle; a tap on the scrim closes it.
QC.Popup {
    id: root
    required property Theme theme
    property string title: ""
    default property alias content: body.data

    parent: QC.Overlay.overlay
    modal: true
    closePolicy: QC.Popup.CloseOnEscape | QC.Popup.CloseOnPressOutside
    x: 0
    width: parent ? parent.width : 0
    y: parent ? parent.height - height : 0
    padding: 0

    QC.Overlay.modal: Rectangle { color: root.theme.scrim }

    background: Rectangle {
        color: root.theme.ground
        radius: root.theme.airmail ? root.theme.gu(3.25) : root.theme.gu(2)
        // Square off the bottom corners: the sheet sits on the screen edge.
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: parent.radius
            color: parent.color
        }
    }

    contentItem: Column {
        spacing: root.theme.gu(1)
        topPadding: root.theme.gu(1.25)
        bottomPadding: root.theme.gu(2)
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            width: root.theme.gu(5)
            height: 4
            radius: 2
            color: root.theme.field
        }
        Text {
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
        Column { id: body; width: parent.width }
    }
}
