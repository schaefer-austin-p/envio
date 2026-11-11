import QtQuick 2.15
import "../../platform"
import "../../components"

// Forward asks about attachments only when the message has some (board 15):
// forward them too, or the text alone. `continued` carries the choice.
Sheet {
    id: root
    objectName: "forwardPrompt"
    property bool withAttachments: true
    signal continued(bool withAttachments)

    title: qsTr("Forward message")
    onAboutToShow: withAttachments = true

    RadioRow {
        objectName: "forwardWith"
        width: parent.width
        theme: root.theme
        text: qsTr("With attachments")
        caption: qsTr("The files go along with the text")
        checked: root.withAttachments
        onActivated: root.withAttachments = true
    }
    RadioRow {
        objectName: "forwardWithout"
        width: parent.width
        theme: root.theme
        text: qsTr("Without attachments")
        caption: qsTr("Only the text of the message")
        checked: !root.withAttachments
        onActivated: root.withAttachments = false
    }
    Row {
        x: root.theme.gu(2)
        width: parent.width - root.theme.gu(4)
        topPadding: root.theme.gu(1)
        spacing: root.theme.gu(1.25)
        AppButton {
            objectName: "forwardCancel"
            width: (parent.width - parent.spacing) / 2
            theme: root.theme
            text: qsTr("Cancel")
            onClicked: root.close()
        }
        AppButton {
            objectName: "forwardContinue"
            width: (parent.width - parent.spacing) / 2
            theme: root.theme
            primary: true
            text: qsTr("Continue")
            onClicked: { root.close(); root.continued(root.withAttachments) }
        }
    }
}
