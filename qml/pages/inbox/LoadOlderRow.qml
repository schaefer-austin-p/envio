import QtQuick 2.15
import QtQuick.Controls 2.15
import "../../platform"
import "../../components"

// The foot of a message list. `state` is envio_ui::OlderRow as the core
// exposes it: 0 hidden, 1 ready (tap to load a batch), 2 loading, 3 done.
// A plain tap row: no swipe.
Item {
    id: root
    required property Theme theme
    property int state: 0
    signal loadRequested()

    width: parent ? parent.width : 0
    height: state === 0 ? 0 : theme.touch
    visible: state !== 0

    AbstractButton {
        objectName: "olderTap"
        anchors.fill: parent
        visible: root.state === 1
        text: qsTr("Load older messages")
        Accessible.role: Accessible.Button
        onClicked: root.loadRequested()
        background: Rectangle { color: parent.down ? root.theme.selected : "transparent" }
        contentItem: Text {
            textFormat: Text.PlainText
            text: parent.text
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            color: root.theme.primary
            font.family: root.theme.fontUi
            font.pixelSize: 16
            font.weight: Font.Medium
        }
    }
    BusyLine {
        objectName: "olderLoading"
        theme: root.theme
        anchors.centerIn: parent
        width: Math.min(implicitWidth, parent.width)
        running: root.state === 2
        text: qsTr("Loading older messages…")
    }
    Text {
        objectName: "olderDone"
        anchors.centerIn: parent
        visible: root.state === 3
        textFormat: Text.PlainText
        text: qsTr("No older messages")
        color: root.theme.secondary
        font.family: root.theme.fontUi
        font.pixelSize: 15
    }
}
