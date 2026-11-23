import QtQuick 2.15
import "../../platform"
import "../../components"
import "../../bindle/Platform" as Platform

// Step 2: the settings that were found, and the password. The password
// lives only in the field; the wizard reads it for `connect` and clears it.
Item {
    id: root
    required property Theme theme
    property string foundText: ""
    // The two guessed servers, one line each (chosen and translated in Rust).
    property string serversText: ""
    // The provider wants an app password (#221): one translated sentence
    // and the provider's help page, both from the core. Empty for no hint.
    property string appHint: ""
    property string appHelpUrl: ""
    readonly property bool appPassword: appHint !== ""
    // For tests: where the help page is opened.
    readonly property alias uri: uri
    property string error: ""
    property bool busy: false
    property alias password: passwordField.text
    // Set when the address can sign in with its provider ("Microsoft"): the
    // step then has no password. `oauthEmail` is the address that goes to the
    // provider as a hint; the notice is the session's (cancelled, failed, or
    // the question after a sign-in as another address).
    property string oauthProvider: ""
    readonly property bool oauth: oauthProvider !== ""
    // Typed on the page after a provider pick (the address goes to the
    // provider as a hint); read-only for an address the wizard found.
    property alias oauthEmail: oauthEmailField.text
    property bool oauthEditable: false
    property string noticeKind: ""
    property string noticeTitle: ""
    property string noticeText: ""
    // The user is asked whether this account is the right one.
    property bool confirming: false
    // The account is of the other kind than the pick: the notice says so and
    // the page offers the one more sign-in.
    property bool mismatch: false
    signal continued()
    signal editServersRequested()
    signal oauthRequested()
    signal oauthAnswered(bool accept)

    Platform.Uri {
        id: uri
        objectName: "signInUri"
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
            spacing: theme.gu(2)

            Rectangle {
                visible: !root.oauth
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
                objectName: "serversText"
                width: parent.width
                visible: !root.oauth && root.serversText !== ""
                text: root.serversText
                wrapMode: Text.WordWrap
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 14
                lineHeight: 1.3
            }
            AppButton {
                objectName: "editServers"
                visible: !root.oauth
                width: parent.width
                theme: root.theme
                text: qsTr("Edit servers")
                enabled: !root.busy
                onClicked: root.editServersRequested()
            }
            // The hint and its link are one block (the boards: about 2 px
            // between them), not two steps of the column's spacing.
            Column {
                objectName: "appPasswordBlock"
                visible: !root.oauth && root.appPassword
                width: parent.width
                spacing: root.theme.gu(0.25)
                Text {
                    textFormat: Text.PlainText
                    objectName: "appPasswordHint"
                    width: parent.width
                    text: root.appHint
                    wrapMode: Text.WordWrap
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 14
                    lineHeight: 1.4
                }
                // A text link on purpose: the hint is one contextual line, not a
                // card or a second button beside "Continue".
                Item {
                    objectName: "appPasswordHelpLink"
                    width: parent.width
                    height: root.theme.gu(5.5)
                    readonly property alias text: linkText.text
                    Row {
                        spacing: root.theme.gu(0.75)
                        anchors.verticalCenter: parent.verticalCenter
                        Text {
                            id: linkText
                            textFormat: Text.PlainText
                            text: qsTr("Create an app password")
                            color: root.theme.primary
                            font.family: root.theme.fontUi
                            font.pixelSize: 15
                            font.weight: Font.DemiBold
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        Icon {
                            theme: root.theme
                            name: "external"
                            size: 18
                            color: root.theme.primary
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        onClicked: uri.open(root.appHelpUrl)
                    }
                    Accessible.role: Accessible.Link
                    Accessible.name: linkText.text
                }
            }
            Field {
                id: passwordField
                objectName: "passwordField"
                visible: !root.oauth
                width: parent.width
                theme: root.theme
                password: true
                label: root.appPassword ? qsTr("App password") : qsTr("Password")
                onAccepted: if (continueButton.enabled) root.continued()
            }
            // The Microsoft variant: the address, why no password is typed,
            // and what Envio keeps.
            Field {
                id: oauthEmailField
                objectName: "oauthEmail"
                visible: root.oauth && !root.mismatch
                width: parent.width
                theme: root.theme
                label: qsTr("Email address")
                readOnly: !root.oauthEditable
                inputMethodHints: Qt.ImhEmailCharactersOnly
            }
            NoticeCard {
                objectName: "oauthNotice"
                visible: root.oauth && root.noticeKind !== ""
                width: parent.width
                theme: root.theme
                kind: root.noticeKind
                title: root.noticeTitle
                text: root.noticeText
            }
            Text {
                textFormat: Text.PlainText
                objectName: "oauthExplain"
                visible: root.oauth
                width: parent.width
                text: root.mismatch
                    ? qsTr("Microsoft opens in the browser again. Sign in with the same address.")
                    : root.noticeKind === ""
                    ? qsTr("Microsoft signs you in on its own page in the browser. Your password never passes through Envio, and you come back here when it is done.")
                    : qsTr("Microsoft signs you in on its own page in the browser. Your password never passes through Envio.")
                wrapMode: Text.WordWrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: root.noticeKind === "" ? 15 : 14
                lineHeight: 1.3
            }
            Rectangle {
                objectName: "oauthPrivacy"
                visible: root.oauth && root.noticeKind === ""
                width: parent.width
                height: privacy.implicitHeight + root.theme.gu(3)
                radius: root.theme.cardRadius
                color: root.theme.surface
                border.width: 1
                border.color: root.theme.hairline
                Icon {
                    id: lock
                    theme: root.theme
                    name: "lock"
                    size: 20
                    color: root.theme.text
                    x: root.theme.gu(1.75)
                    y: root.theme.gu(1.5)
                }
                Text {
                    textFormat: Text.PlainText
                    id: privacy
                    x: lock.x + lock.width + root.theme.gu(1.25)
                    y: root.theme.gu(1.5)
                    width: parent.width - x - root.theme.gu(1.75)
                    text: qsTr("Envio keeps only a sign-in token, on this phone. Remove the account and the token goes with it.")
                    wrapMode: Text.WordWrap
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 14
                    lineHeight: 1.3
                }
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
        id: continueButton
        objectName: "signInContinue"
        visible: !root.oauth
        width: parent.width
        theme: root.theme
        primary: true
        text: qsTr("Continue")
        enabled: !root.busy && passwordField.text !== ""
        onClicked: root.continued()
    }
    AppButton {
        objectName: "oauthButton"
        visible: root.oauth && !root.confirming && !root.mismatch
        enabled: !root.oauthEditable || oauthEmailField.text.trim() !== ""
        width: parent.width
        theme: root.theme
        primary: true
        iconName: "external"
        text: qsTr("Sign in with Microsoft")
        onClicked: root.oauthRequested()
    }
    AppButton {
        objectName: "mismatchContinue"
        visible: root.oauth && root.mismatch
        width: parent.width
        theme: root.theme
        primary: true
        iconName: "external"
        text: qsTr("Sign in with Microsoft")
        onClicked: root.oauthAnswered(true)
    }
    AppButton {
        objectName: "useAccount"
        visible: root.oauth && root.confirming
        width: parent.width
        theme: root.theme
        primary: true
        iconName: "external"
        text: qsTr("Use this account")
        onClicked: root.oauthAnswered(true)
    }
    }
}
