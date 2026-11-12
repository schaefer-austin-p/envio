import QtQuick 2.15
import "../../platform"
import "../../components"

// The settings came from DNS records of a host outside the address's
// domain (#33): name the servers and ask. Cancel is the filled, listed-first
// action; only "Use these servers" emits `confirmed`.
AppDialog {
    id: root
    property string imapHost: ""
    property int imapPort: 0
    property string smtpHost: ""
    property int smtpPort: 0
    signal confirmed()
    signal cancelled()

    title: qsTr("Use these servers?")
    message: qsTr("These servers are not on your address's domain. They came from the domain's DNS records. Use them only if you recognise them.")
    primaryText: qsTr("Cancel")
    secondaryText: qsTr("Use these servers")
    onPrimaryActivated: cancelled()
    onSecondaryActivated: confirmed()

    Text {
        textFormat: Text.PlainText
        objectName: "imapLine"
        width: parent.width
        text: qsTr("IMAP: %1, port %2").arg(root.imapHost).arg(root.imapPort)
        color: root.theme.heading
        font.family: root.theme.fontAddress
        font.pixelSize: 14
        wrapMode: Text.WrapAnywhere
    }
    Text {
        textFormat: Text.PlainText
        objectName: "smtpLine"
        width: parent.width
        text: qsTr("SMTP: %1, port %2").arg(root.smtpHost).arg(root.smtpPort)
        color: root.theme.heading
        font.family: root.theme.fontAddress
        font.pixelSize: 14
        wrapMode: Text.WrapAnywhere
    }
}
