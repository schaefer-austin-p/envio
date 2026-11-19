import QtQuick 2.15
import "../../platform"
import "../../components"

// The reader's bottom bar (boards 2 to 4): Reply, filled, and Forward,
// outlined, side by side, in the pinned PageFooter.
PageFooter {
    id: root
    // The wide layout: fixed 20 gu buttons, not stretched across the pane.
    property bool wide: false
    signal replyRequested()
    signal forwardRequested()

    Row {
        // Under the text column when the pane is wider than the measure.
        width: root.wide ? root.theme.gu(40) + spacing : parent.width
        x: root.wide ? Math.max(0, (parent.width - Math.min(parent.width, root.theme.gu(80))) / 2) : 0
        spacing: root.theme.gu(1.25)
        AppButton {
            id: reply
            objectName: "reply"
            width: root.wide ? root.theme.gu(20) : (parent.width - parent.spacing) / 2
            theme: root.theme
            primary: true
            text: qsTr("Reply")
            onClicked: root.replyRequested()
        }
        AppButton {
            objectName: "forward"
            width: root.wide ? root.theme.gu(20) : (parent.width - parent.spacing) / 2
            theme: root.theme
            text: qsTr("Forward")
            onClicked: root.forwardRequested()
        }
    }
}
