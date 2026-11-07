import QtQuick 2.15
import QtQuick.Controls 2.15
import "../platform"

// The unified inbox. Empty until accounts exist (Plan 7 fills it).
Page {
    id: page
    required property Theme theme

    background: Rectangle { color: page.theme.ground }

    header: ToolBar {
        height: 56
        background: Rectangle {
            color: page.theme.ground
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: 1
                color: page.theme.hairline
            }
        }
        Label {
            x: 16
            anchors.verticalCenter: parent.verticalCenter
            text: qsTr("Inbox")
            color: page.theme.heading
            font.pixelSize: 20
            font.weight: Font.DemiBold
        }
    }

    Label {
        anchors.centerIn: parent
        width: parent.width - 32
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        text: qsTr("No accounts yet.")
        color: page.theme.secondary
    }
}
