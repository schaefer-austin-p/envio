import QtQuick 2.15
import "../../platform"
import "../../components"

// One unsent message: subject, recipients, its status in words, and the
// actions its status allows. Envio never resends on its own, so "Send again"
// is only ever a tap here.
//   waiting, sending: Delete
//   failed:           Retry, Edit, Delete
//   maybesent:        Send again, Move to Sent, Delete (the server may
//                     already have it, so the user decides)
ListRow {
    id: root
    required property var model
    signal decided(string item, string decision)
    signal editRequested(string item)
    signal deleteRequested(string item)

    objectName: "outbox-" + model.id

    Column {
        x: root.theme.gu(2.5)
        y: root.theme.gu(1.75)
        width: parent.width - root.theme.gu(5)
        spacing: root.theme.gu(0.5)
        Text {
            textFormat: Text.PlainText
            width: parent.width
            text: root.model.subject
            elide: Text.ElideRight
            color: root.theme.heading
            font.family: root.theme.fontUi
            font.pixelSize: 16
            font.weight: Font.Medium
        }
        Text {
            textFormat: Text.PlainText
            width: parent.width
            text: root.model.to
            elide: Text.ElideRight
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 14
        }
        Text {
            textFormat: Text.PlainText
            objectName: "status-" + root.model.id
            width: parent.width
            text: root.model.text
            wrapMode: Text.WordWrap
            color: root.model.status === "failed" ? root.theme.accent : root.theme.secondary
            font.family: root.theme.fontUi
            font.pixelSize: 13
        }
        Flow {
            width: parent.width
            spacing: root.theme.gu(1)
            topPadding: root.theme.gu(0.5)
            AppButton {
                objectName: "sendAgain-" + root.model.id
                visible: root.model.status === "maybesent"
                theme: root.theme
                text: qsTr("Send again")
                onClicked: root.decided(root.model.id, "sendagain")
            }
            AppButton {
                objectName: "moveToSent-" + root.model.id
                visible: root.model.status === "maybesent"
                theme: root.theme
                text: qsTr("Move to Sent")
                onClicked: root.decided(root.model.id, "movetosent")
            }
            AppButton {
                objectName: "retry-" + root.model.id
                visible: root.model.status === "failed"
                theme: root.theme
                text: qsTr("Retry")
                onClicked: root.decided(root.model.id, "retry")
            }
            AppButton {
                objectName: "edit-" + root.model.id
                visible: root.model.status === "failed"
                theme: root.theme
                text: qsTr("Edit")
                onClicked: root.editRequested(root.model.id)
            }
            AppButton {
                objectName: "delete-" + root.model.id
                theme: root.theme
                text: qsTr("Delete")
                onClicked: root.deleteRequested(root.model.id)
            }
        }
    }
}
