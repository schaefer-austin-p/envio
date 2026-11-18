import QtQuick 2.15
import "../../platform"
import "../../components"

// Rename an account (#217): one Field with the current label, a hint that an
// empty one falls back to the default, and Cancel and Save. The label shows
// on the inbox tabs, the drawer heading and the Accounts page; the sender
// name on mail is not this.
Sheet {
    id: root
    objectName: "renameSheet"
    property var core: null
    property string account: ""
    property string email: ""
    property string label: ""

    title: qsTr("Rename account")

    // Opens with the account's current label in the field.
    function show(key, address, current) {
        account = key
        email = address
        label = current
        field.text = current
        open()
    }

    Column {
        x: root.theme.gu(2.5)
        width: parent.width - root.theme.gu(5)
        spacing: root.theme.gu(1.5)
        Text {
            textFormat: Text.PlainText
            width: parent.width
            text: root.email
            elide: Text.ElideRight
            color: root.theme.secondary
            font.family: root.theme.fontAddress
            font.pixelSize: 12
        }
        Field {
            id: field
            objectName: "labelField"
            theme: root.theme
            width: parent.width
            label: qsTr("Label")
            onAccepted: save.clicked()
        }
        Text {
            objectName: "labelHint"
            textFormat: Text.PlainText
            width: parent.width
            text: qsTr("Leave empty to show the default.")
            wrapMode: Text.Wrap
            color: root.theme.secondary
            font.family: root.theme.fontUi
            font.pixelSize: 13
        }
        Row {
            width: parent.width
            spacing: root.theme.gu(1.25)
            AppButton {
                objectName: "renameCancel"
                theme: root.theme
                width: (parent.width - parent.spacing) / 2
                text: qsTr("Cancel")
                onClicked: root.close()
            }
            AppButton {
                id: save
                objectName: "renameSave"
                theme: root.theme
                primary: true
                width: (parent.width - parent.spacing) / 2
                text: qsTr("Save")
                onClicked: {
                    root.core.setAccountLabel(root.account, field.text)
                    root.close()
                }
            }
        }
    }
}
