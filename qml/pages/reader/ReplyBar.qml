import QtQuick 2.15
import "../../platform"
import "../../components"

// The reader's bottom bar (boards 2 to 4): Reply, filled, and Forward,
// outlined, side by side, in the pinned PageFooter.
PageFooter {
    id: root
    signal replyRequested()
    signal forwardRequested()

    Row {
        width: parent.width
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
