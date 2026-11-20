import QtQuick 2.15
import "../../platform"
import "../../components"

// The reader's bottom bar (boards 2 to 4): Reply, filled, and Forward,
// outlined, side by side, in the pinned PageFooter. A draft (`isDraft`) has
// one filled Edit button with a pencil instead (carbon-draft-edit).
PageFooter {
    id: root
    // The wide layout: fixed 20 gu buttons, not stretched across the pane.
    property bool wide: false
    signal replyRequested()
    signal forwardRequested()
    signal editRequested()
    property bool isDraft: false

    Row {
        // Under the text column, which starts 5 gu in (the footer body starts at 2).
        width: root.wide ? root.theme.gu(40) + spacing : parent.width
        x: root.wide ? root.theme.gu(3) : 0
        spacing: root.theme.gu(1.25)
        AppButton {
            id: reply
            visible: !root.isDraft
            objectName: "reply"
            width: root.wide ? root.theme.gu(20) : (parent.width - parent.spacing) / 2
            theme: root.theme
            primary: true
            text: qsTr("Reply")
            onClicked: root.replyRequested()
        }
        AppButton {
            objectName: "forward"
            visible: !root.isDraft
            width: root.wide ? root.theme.gu(20) : (parent.width - parent.spacing) / 2
            theme: root.theme
            text: qsTr("Forward")
            onClicked: root.forwardRequested()
        }
        AppButton {
            objectName: "edit"
            visible: root.isDraft
            width: root.wide ? root.theme.gu(20) : parent.width
            theme: root.theme
            primary: true
            iconName: "pencil"
            text: qsTr("Edit")
            onClicked: root.editRequested()
        }
    }
}
