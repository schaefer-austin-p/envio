import QtQuick 2.15
import "../../platform"
import "../../components"

// Manual setup: typed IMAP and SMTP servers. `setupJson(email)` is the
// setup record `connect` takes (same keys as `detected` carries), with no
// source and nothing to confirm: the user typed these hosts.
Flickable {
    id: root
    required property Theme theme
    property string email: ""
    property string error: ""
    property bool busy: false
    property alias password: passwordField.text
    readonly property bool valid: imapHost.text.trim() !== "" && smtpHost.text.trim() !== ""
        && portOk(imapPort.text) && portOk(smtpPort.text)
        && username.text.trim() !== "" && passwordField.text !== ""
    signal continued()

    function portOk(text) {
        const n = Number(text)
        return text !== "" && Number.isInteger(n) && n >= 1 && n <= 65535
    }
    function setupJson() {
        return JSON.stringify({
            name: root.email,
            email: root.email,
            username: username.text.trim(),
            imapHost: imapHost.text.trim(),
            imapPort: Number(imapPort.text),
            imapSecurity: imapStarttls.checked ? "starttls" : "tls",
            smtpHost: smtpHost.text.trim(),
            smtpPort: Number(smtpPort.text),
            smtpSecurity: smtpStarttls.checked ? "starttls" : "tls",
            source: null,
            needsConfirm: false
        })
    }

    contentHeight: col.height + theme.gu(4)
    clip: true

    Column {
        id: col
        x: theme.gu(2.5)
        y: theme.gu(2)
        width: root.width - theme.gu(5)
        spacing: theme.gu(2)

        Field { id: imapHost; objectName: "imapHost"; width: parent.width; theme: root.theme
            label: qsTr("IMAP server"); inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase }
        Field { id: imapPort; objectName: "imapPort"; width: parent.width; theme: root.theme
            label: qsTr("IMAP port"); text: "993"; inputMethodHints: Qt.ImhDigitsOnly }
        Row {
            width: parent.width
            spacing: root.theme.gu(1)
            Text {
                width: parent.width - imapStarttls.width - root.theme.gu(1)
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Upgrade with STARTTLS (otherwise TLS from the start)")
                wrapMode: Text.WordWrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 14
            }
            Toggle { id: imapStarttls; objectName: "imapStarttls"; theme: root.theme; text: qsTr("IMAP STARTTLS") }
        }
        Field { id: smtpHost; objectName: "smtpHost"; width: parent.width; theme: root.theme
            label: qsTr("SMTP server"); inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoAutoUppercase }
        Field { id: smtpPort; objectName: "smtpPort"; width: parent.width; theme: root.theme
            label: qsTr("SMTP port"); text: "587"; inputMethodHints: Qt.ImhDigitsOnly }
        Row {
            width: parent.width
            spacing: root.theme.gu(1)
            Text {
                width: parent.width - smtpStarttls.width - root.theme.gu(1)
                anchors.verticalCenter: parent.verticalCenter
                text: qsTr("Upgrade with STARTTLS (otherwise TLS from the start)")
                wrapMode: Text.WordWrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 14
            }
            Toggle { id: smtpStarttls; objectName: "smtpStarttls"; theme: root.theme; checked: true; text: qsTr("SMTP STARTTLS") }
        }
        Field { id: username; objectName: "username"; width: parent.width; theme: root.theme
            label: qsTr("User name"); text: root.email; inputMethodHints: Qt.ImhNoAutoUppercase }
        Field { id: passwordField; objectName: "serversPassword"; width: parent.width; theme: root.theme
            label: qsTr("Password"); password: true }
        AppButton {
            objectName: "connectButton"
            width: parent.width
            theme: root.theme
            primary: true
            text: qsTr("Connect")
            enabled: !root.busy && root.valid
            onClicked: root.continued()
        }
        Text {
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
