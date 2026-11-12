import QtQuick 2.15
import "../../platform"
import "../../components"

// Board 14d: an unverified certificate. The filled, listed-first button is
// Cancel; trusting is the outlined one. The fingerprint is `hex` as the
// glue sends it, shown in the mono face for comparison with the server's.
AppDialog {
    id: root
    property string host: ""
    // The core's title, as given: it names the server (IMAP or SMTP).
    property string heading: ""
    property string service: ""
    property string fingerprint: ""
    property string hex: ""
    property string body: ""
    signal trusted()

    title: heading
    message: body
    primaryText: qsTr("Cancel")
    secondaryText: qsTr("Trust this certificate")
    onSecondaryActivated: trusted()

    Text {
        textFormat: Text.PlainText
        width: parent.width
        text: qsTr("SHA-256")
        color: root.theme.secondary
        font.family: root.theme.fontUi
        font.pixelSize: 13
    }
    Text {
        textFormat: Text.PlainText
        objectName: "fingerprint"
        width: parent.width
        text: root.fingerprint
        wrapMode: Text.WrapAnywhere
        color: root.theme.heading
        font.family: root.theme.fontAddress
        font.pixelSize: 13
        lineHeight: 1.3
    }
}
