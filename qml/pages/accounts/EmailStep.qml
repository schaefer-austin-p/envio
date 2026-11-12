import QtQuick 2.15
import "../../platform"
import "../../components"

// Step 1: the address. The ISPDB note is always on screen here, so the
// domain is never sent (Continue) before the user could read it.
Item {
    id: root
    required property Theme theme
    required property var core
    property string error: ""
    property bool busy: false
    property alias email: emailField.text
    signal continued()
    signal manualRequested()

    KeyboardAvoider { objectName: "keyboardAvoider"; flickable: flick }

    Flickable {
        id: flick
        objectName: "stepFlick"
        anchors { top: parent.top; bottom: footer.top; left: parent.left; right: parent.right }
        contentHeight: col.height + root.theme.gu(4)
        clip: true

        Column {
            id: col
            x: root.theme.gu(2.5)
            y: root.theme.gu(2)
            width: root.width - root.theme.gu(5)
            spacing: theme.gu(2)

            Text {
                textFormat: Text.PlainText
                width: parent.width
                text: qsTr("Enter your email address. Envio finds the mail servers for it.")
                wrapMode: Text.WordWrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 15
            }
            Field {
                id: emailField
                objectName: "emailField"
                width: parent.width
                theme: root.theme
                label: qsTr("Email address")
                inputMethodHints: Qt.ImhEmailCharactersOnly | Qt.ImhNoAutoUppercase
                onAccepted: if (continueButton.enabled) root.continued()
            }
            Text {
                textFormat: Text.PlainText
                objectName: "ispdbNote"
                width: parent.width
                text: root.core.ispdbNote()
                wrapMode: Text.WordWrap
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 13
                lineHeight: 1.3
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
            AppButton {
                objectName: "manualButton"
                width: parent.width
                theme: root.theme
                text: qsTr("Set up manually")
                onClicked: root.manualRequested()
            }
        }
    }

    PageFooter {
        id: footer
        theme: root.theme
    AppButton {
        id: continueButton
        objectName: "continueButton"
        width: parent.width
        theme: root.theme
        primary: true
        text: qsTr("Continue")
        enabled: !root.busy && emailField.text.indexOf("@") > 0
        onClicked: root.continued()
    }
    }
}
