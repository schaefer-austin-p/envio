import QtQuick 2.15
import "../../platform"
import "../../components"

// Step 2: the settings that were found, and the password. The password
// lives only in the field; the wizard reads it for `connect` and clears it.
Flickable {
    id: root
    required property Theme theme
    property string foundText: ""
    // Detected.source is "provider": that provider wants an app password.
    property bool appPassword: false
    property string error: ""
    property bool busy: false
    property alias password: passwordField.text
    signal continued()

    contentHeight: col.height + theme.gu(4)
    clip: true

    KeyboardAvoider { objectName: "keyboardAvoider"; flickable: root }

    Column {
        id: col
        x: theme.gu(2.5)
        y: theme.gu(2)
        width: root.width - theme.gu(5)
        spacing: theme.gu(2)

        Rectangle {
            width: parent.width
            height: found.implicitHeight + root.theme.gu(3)
            radius: root.theme.cardRadius
            color: root.theme.surface
            border.width: 1
            border.color: root.theme.hairline
            Text {
                textFormat: Text.PlainText
                id: found
                objectName: "foundText"
                anchors.fill: parent
                anchors.margins: root.theme.gu(1.5)
                text: root.foundText
                wrapMode: Text.WordWrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 14
                lineHeight: 1.3
            }
        }
        Text {
            textFormat: Text.PlainText
            objectName: "appPasswordHint"
            width: parent.width
            visible: root.appPassword
            text: qsTr("This provider needs an app password for Envio, not your usual one. Create one in your account settings.")
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 14
            lineHeight: 1.3
        }
        Field {
            id: passwordField
            objectName: "passwordField"
            width: parent.width
            theme: root.theme
            password: true
            label: root.appPassword ? qsTr("App password") : qsTr("Password")
            onAccepted: if (continueButton.enabled) root.continued()
        }
        AppButton {
            id: continueButton
            objectName: "signInContinue"
            width: parent.width
            theme: root.theme
            primary: true
            text: qsTr("Continue")
            enabled: !root.busy && passwordField.text !== ""
            onClicked: root.continued()
        }
        Text {
            textFormat: Text.PlainText
            objectName: "stepError"
            width: parent.width
            visible: root.error !== ""
            text: root.error
            wrapMode: Text.WordWrap
            color: root.theme.accent
            font.family: root.theme.fontUi
            font.pixelSize: 14
        }
    }
}
