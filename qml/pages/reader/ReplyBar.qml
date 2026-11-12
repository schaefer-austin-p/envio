import QtQuick 2.15
import "../../platform"
import "../../components"

// The reader's bottom bar (boards 2 to 4): Reply, filled, and Forward,
// outlined, side by side.
Rectangle {
    id: root
    required property Theme theme
    signal replyRequested()
    signal forwardRequested()

    height: reply.height + theme.gu(3)
    color: theme.ground

    Rectangle { width: parent.width; height: 1; color: root.theme.hairline }
    Row {
        x: root.theme.gu(2)
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - root.theme.gu(4)
        spacing: root.theme.gu(1.25)
        AppButton {
            id: reply
            objectName: "reply"
            width: (parent.width - parent.spacing) / 2
            theme: root.theme
            primary: true
            text: qsTr("Reply")
            onClicked: root.replyRequested()
        }
        AppButton {
            objectName: "forward"
            width: (parent.width - parent.spacing) / 2
            theme: root.theme
            text: qsTr("Forward")
            onClicked: root.forwardRequested()
        }
    }
}
