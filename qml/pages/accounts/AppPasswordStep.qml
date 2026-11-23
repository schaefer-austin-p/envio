import QtQuick 2.15
import "../../platform"
import "../../components"
import "../../bindle/Platform" as Platform

// Step 2 for iCloud, Gmail and Yahoo (#221): they take an app password, not
// the usual one. The steps to make one, a button to the provider's page, then
// the address and the password. The servers are known (the wizard asks the
// core for them), so nothing is looked up. The password lives only in the
// field; the wizard reads it for `connect` and clears it.
Item {
    id: root
    required property Theme theme
    // "icloud", "gmail" or "yahoo".
    property string provider: "icloud"
    // The provider's app-password page; the core owns it (#221).
    property string helpUrl: ""
    property string error: ""
    property bool busy: false
    property alias email: emailField.text
    property alias password: passwordField.text
    // For tests: where the provider's page is opened.
    readonly property alias uri: uri
    signal continued()

    readonly property var info: ({
        "icloud": {
            "intro": qsTr("iCloud needs an app password for Envio, not your Apple Account password. Turn on two-factor authentication first."),
            "steps": qsTr("1. Open your Apple Account settings.\n2. Under Sign-In and Security, choose App-Specific Passwords.\n3. Create one named “Envio” and copy it."),
            "button": qsTr("Open Apple Account settings")
        },
        "gmail": {
            "intro": qsTr("Gmail needs an app password for Envio, not your Google password. Turn on 2-Step Verification first."),
            "steps": qsTr("1. Open your Google account settings.\n2. Under Security, choose App passwords.\n3. Create one named “Envio” and copy it."),
            "button": qsTr("Open Google account settings")
        },
        "yahoo": {
            "intro": qsTr("Yahoo needs an app password for Envio, not your Yahoo password. Turn on two-step verification first."),
            "steps": qsTr("1. Open your Yahoo account security page.\n2. Choose Generate app password.\n3. Create one named “Envio” and copy it."),
            "button": qsTr("Open Yahoo account security")
        }
    })
    readonly property var current: info[provider] !== undefined ? info[provider] : info["icloud"]
    readonly property bool hasAddress: {
        const t = emailField.text.trim()
        const at = t.lastIndexOf("@")
        return at > 0 && at < t.length - 1 && t.indexOf("@", at + 1) < 0
    }

    Platform.Uri {
        id: uri
        objectName: "appPasswordUri"
        sourceFile: ""
    }
    Platform.KeyboardAvoider { objectName: "keyboardAvoider"; flickable: flick }

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
            spacing: root.theme.gu(2)

            Text {
                textFormat: Text.PlainText
                objectName: "appPasswordIntro"
                width: parent.width
                text: root.current.intro
                wrapMode: Text.WordWrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 15
                lineHeight: 1.3
            }
            Text {
                textFormat: Text.PlainText
                objectName: "appPasswordSteps"
                width: parent.width
                text: root.current.steps
                wrapMode: Text.WordWrap
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 14
                lineHeight: 1.3
            }
            AppButton {
                objectName: "appPasswordLink"
                width: parent.width
                theme: root.theme
                iconName: "external"
                text: root.current.button
                onClicked: uri.open(root.helpUrl)
            }
            Field {
                id: emailField
                objectName: "appPasswordEmail"
                width: parent.width
                theme: root.theme
                label: qsTr("Email address")
            }
            Field {
                id: passwordField
                objectName: "appPasswordField"
                width: parent.width
                theme: root.theme
                password: true
                label: qsTr("App password")
                onAccepted: if (root.hasAddress && text !== "" && !root.busy) root.continued()
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

    PageFooter {
        id: footer
        theme: root.theme
        busy: root.busy
        busyText: qsTr("Signing in…")
        AppButton {
            objectName: "appPasswordContinue"
            width: parent.width
            theme: root.theme
            primary: true
            text: qsTr("Continue")
            enabled: !root.busy && root.hasAddress && passwordField.text !== ""
            onClicked: root.continued()
        }
    }
}
