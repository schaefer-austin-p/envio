import QtQuick 2.15
import "../../platform"
import "../../components"

// A new password for an account that failed to sign in. The password goes
// straight to `updatePassword`; the field is cleared when it is sent and
// when the page closes.
Item {
    id: root
    required property Theme theme
    required property var core
    property string account: ""
    property string accountName: ""
    signal closed()

    function close() {
        field.text = ""
        root.closed()
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Update password")
        onBackClicked: root.close()
    }
    Column {
        anchors { top: bar.bottom; left: parent.left; right: parent.right; margins: root.theme.gu(2.5) }
        anchors.topMargin: root.theme.gu(2)
        spacing: root.theme.gu(2)

        Text {
            textFormat: Text.PlainText
            width: parent.width
            text: root.accountName
            elide: Text.ElideRight
            color: root.theme.heading
            font.family: root.theme.fontUi
            font.pixelSize: 16
            font.weight: Font.Medium
        }
        Field {
            id: field
            objectName: "newPassword"
            width: parent.width
            theme: root.theme
            password: true
            label: qsTr("New password")
            onAccepted: if (send.enabled) send.clicked()
        }
        AppButton {
            id: send
            objectName: "savePassword"
            width: parent.width
            theme: root.theme
            primary: true
            text: qsTr("Save password")
            enabled: field.text !== ""
            onClicked: {
                root.core.updatePassword(root.account, field.text)
                root.close()
            }
        }
    }
}
