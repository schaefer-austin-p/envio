import QtQuick 2.15
import "../../platform"
import "../../components"

// Accounts (board 12): name, address and status per row. A tap opens the
// account's actions; there is no swipe. Removing asks first when the core
// says unsent work would go (`confirmRemove`).
Item {
    id: root
    required property Theme theme
    required property var core
    // The accounts model (roles key, name, email, status, state, unread, unreadText).
    required property var accounts
    property string current: ""
    property string currentName: ""
    signal backRequested()
    signal addRequested()
    signal updatePasswordRequested(string account, string name)

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Accounts")
        onBackClicked: root.backRequested()
        IconButton {
            objectName: "addAccount"
            theme: root.theme
            iconName: "plus"
            text: qsTr("Add account")
            onClicked: root.addRequested()
        }
    }

    ListView {
        id: list
        objectName: "accountList"
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        clip: true
        model: root.accounts
        delegate: ListRow {
            id: row
            required property var model
            objectName: "account-" + model.key
            theme: root.theme
            width: list.width
            onClicked: {
                root.current = row.model.key
                root.currentName = row.model.name
                actions.open()
            }
            Column {
                x: root.theme.gu(2.5)
                y: root.theme.gu(1.75)
                width: parent.width - root.theme.gu(5)
                spacing: 2
                Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: row.model.name
                    elide: Text.ElideRight
                    color: root.theme.heading
                    font.family: root.theme.fontUi
                    font.pixelSize: 16
                    font.weight: Font.Medium
                }
                Text {
                    textFormat: Text.PlainText
                    width: parent.width
                    text: row.model.email
                    elide: Text.ElideRight
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 14
                }
                Text {
                    textFormat: Text.PlainText
                    objectName: "status"
                    width: parent.width
                    text: row.model.status
                    wrapMode: Text.WordWrap
                    color: row.model.state === "ok" ? root.theme.secondary : root.theme.accent
                    font.family: root.theme.fontUi
                    font.pixelSize: 13
                }
                // A certificate the server presented that does not match:
                // look at it, then trust it or not (the dialog's primary
                // button is Cancel).
                AppButton {
                    objectName: "checkCertificate"
                    visible: row.model.state === "pin" && row.model.pinHex !== ""
                    height: visible ? implicitHeight : 0
                    theme: root.theme
                    text: qsTr("Check certificate")
                    onClicked: {
                        root.current = row.model.key
                        trust.account = row.model.key
                        trust.host = row.model.pinHost
                        trust.service = row.model.pinService
                        trust.heading = row.model.pinTitle
                        trust.fingerprint = row.model.pinFingerprint
                        trust.hex = row.model.pinHex
                        trust.body = row.model.pinText
                        trust.open()
                    }
                }
            }
        }
    }

    Sheet {
        id: actions
        objectName: "actionSheet"
        theme: root.theme
        title: root.currentName
        ListRow {
            objectName: "updatePasswordRow"
            theme: root.theme
            width: parent.width
            onClicked: {
                actions.close()
                root.updatePasswordRequested(root.current, root.currentName)
            }
            Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2.5)
                y: root.theme.gu(1.75)
                text: qsTr("Update password")
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 16
            }
        }
        ListRow {
            objectName: "removeRow"
            theme: root.theme
            width: parent.width
            separator: false
            onClicked: {
                actions.close()
                root.core.removeAccount(root.current, false)
            }
            Text {
                textFormat: Text.PlainText
                x: root.theme.gu(2.5)
                y: root.theme.gu(1.75)
                text: qsTr("Remove")
                color: root.theme.accent
                font.family: root.theme.fontUi
                font.pixelSize: 16
            }
        }
    }

    AppDialog {
        id: removeAsk
        objectName: "removeAsk"
        theme: root.theme
        property string account: ""
        title: qsTr("Remove this account?")
        primaryText: qsTr("Keep account")
        secondaryText: qsTr("Remove")
        onSecondaryActivated: root.core.removeAccount(removeAsk.account, true)
    }
    TrustDialog {
        id: trust
        objectName: "trustDialog"
        theme: root.theme
        property string account: ""
        onTrusted: root.core.acceptCertificate(trust.account, trust.service, trust.hex)
    }
    Connections {
        target: root.core
        function onConfirmRemove(account, text) {
            removeAsk.account = account
            removeAsk.message = text
            removeAsk.open()
        }
    }
}
