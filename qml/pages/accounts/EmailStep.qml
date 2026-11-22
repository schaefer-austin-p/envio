import QtQuick 2.15
import "../../platform"
import "../../components"
import "../../bindle/Platform" as Platform

// Step 1: the address. The ISPDB note is always on screen here, so the
// domain is never sent (Continue) before the user could read it.
Item {
    id: root
    required property Theme theme
    required property var core
    property string error: ""
    // The address was refused (Microsoft, #381): a card, "Change address" in
    // place of Continue, and no manual setup or ISPDB note (nothing is sent).
    property string refusedTitle: ""
    property string refusedText: ""
    // The refusal still offers manual setup (a Microsoft 365 MX hit, #384).
    property bool manualOk: false
    readonly property bool refused: refusedText !== ""
    // The mail is on Microsoft 365 (#384) and sign-in is set up: an info card,
    // "Sign in with Microsoft" in the footer, and "Change address" and
    // "Set up manually" under the card. The ISPDB note is hidden, as for a
    // refusal.
    property string hostedTitle: ""
    property string hostedText: ""
    readonly property bool hosted: hostedText !== ""
    // How the last hosted sign-in ended, when nothing was saved (a second card).
    property string hostedNoticeKind: ""
    property string hostedNoticeTitle: ""
    property string hostedNoticeText: ""
    property bool busy: false
    property alias email: emailField.text
    signal continued()
    signal manualRequested()
    signal signInRequested()
    // "Change address" on the hosted card: the card goes, the cursor is in the field.
    signal addressChangeRequested()
    // The manual path has no address field of its own, so both buttons wait.
    // Same rule as the core: text before the last "@", a domain after it.
    readonly property bool hasAddress: {
        const t = emailField.text.trim()
        const at = t.lastIndexOf("@")
        return at > 0 && at < t.length - 1 && t.indexOf("@", at + 1) < 0
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
                onAccepted: {
                    if (!continueButton.enabled) return
                    if (root.hosted) root.signInRequested()
                    else if (!root.refused) root.continued()
                }
            }
            Text {
                textFormat: Text.PlainText
                objectName: "ispdbNote"
                visible: !root.refused && !root.hosted
                width: parent.width
                text: root.core.ispdbNote()
                wrapMode: Text.WordWrap
                color: root.theme.secondary
                font.family: root.theme.fontUi
                font.pixelSize: 13
                lineHeight: 1.3
            }
            NoticeCard {
                objectName: "refusalCard"
                visible: root.refused
                width: parent.width
                theme: root.theme
                kind: "info"
                title: root.refusedTitle
                text: root.refusedText
            }
            NoticeCard {
                objectName: "hostedCard"
                visible: root.hosted
                width: parent.width
                theme: root.theme
                kind: "info"
                title: root.hostedTitle
                text: root.hostedText
            }
            NoticeCard {
                objectName: "hostedNotice"
                visible: root.hosted && root.hostedNoticeText !== ""
                width: parent.width
                theme: root.theme
                kind: root.hostedNoticeKind === "warning" ? "warning" : "info"
                title: root.hostedNoticeTitle
                text: root.hostedNoticeText
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
                objectName: "changeButton"
                width: parent.width
                theme: root.theme
                text: qsTr("Change address")
                visible: root.hosted
                onClicked: { root.addressChangeRequested(); emailField.input.forceActiveFocus() }
            }
            AppButton {
                objectName: "manualButton"
                width: parent.width
                theme: root.theme
                text: qsTr("Set up manually")
                visible: !root.refused || root.manualOk
                enabled: root.hasAddress
                onClicked: root.manualRequested()
            }
        }
    }

    PageFooter {
        id: footer
        theme: root.theme
        busy: root.busy
        busyText: qsTr("Looking up settings…")
    AppButton {
        id: continueButton
        objectName: "continueButton"
        width: parent.width
        theme: root.theme
        primary: true
        text: root.hosted ? qsTr("Sign in with Microsoft")
            : root.refused ? qsTr("Change address") : qsTr("Continue")
        enabled: root.hosted ? !root.busy : (root.refused || (!root.busy && root.hasAddress))
        onClicked: root.hosted ? root.signInRequested()
            : root.refused ? emailField.input.forceActiveFocus() : root.continued()
    }
    }
}
