import QtQuick 2.15
import "../../platform"
import "../../components"

// The Outbox: unsent messages with what happened to each. Deleting asks
// first, and Cancel is the default button, since an unsent message is gone
// for good.
Item {
    id: root
    required property Theme theme
    required property var core
    required property var outbox
    // The compose id `editOutbox` started; the compose page follows it.
    signal composeOpened(var compose)
    signal backRequested()
    // The way to the Accounts page, where the certificate is checked.
    signal accountsRequested()
    // A message waits for a Microsoft sign-in: the way to Sign in again.
    signal signInAgainRequested(string account)

    property string asking: ""
    // The accounts model (roles key, display), for the name of the account
    // a message is held for.
    property var accounts: null
    property var names: ({})
    function setName(key, name) {
        if (names[key] === name) return
        const copy = Object.assign({}, names)
        copy[key] = name
        names = copy
    }
    Repeater {
        model: root.accounts
        delegate: Item {
            id: acc
            required property var model
            visible: false
            readonly property string key: acc.model.key
            readonly property string label: acc.model.display
            onKeyChanged: root.setName(key, label)
            onLabelChanged: root.setName(key, label)
            Component.onCompleted: root.setName(key, label)
        }
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: qsTr("Outbox")
        onBackClicked: root.backRequested()
    }

    ListView {
        id: list
        objectName: "outboxList"
        anchors { top: bar.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        clip: true
        model: root.outbox
        delegate: OutboxRow {
            theme: root.theme
            width: list.width
            accountName: root.names[model.account] || ""
            onAccountsRequested: root.accountsRequested()
            onSignInAgainRequested: function(account) { root.signInAgainRequested(account) }
            onDecided: root.core.decideOutbox(item, decision)
            onEditRequested: root.composeOpened(root.core.editOutbox(item))
            onDeleteRequested: { root.asking = item; deleteAsk.open() }
        }
    }
    Text {
        textFormat: Text.PlainText
        anchors.centerIn: parent
        visible: list.count === 0
        text: qsTr("Nothing waiting to send.")
        color: root.theme.secondary
        font.family: root.theme.fontUi
        font.pixelSize: 15
    }

    AppDialog {
        id: deleteAsk
        objectName: "deleteAsk"
        theme: root.theme
        title: qsTr("Delete this message?")
        message: qsTr("It has not been sent, and it cannot be brought back.")
        primaryText: qsTr("Cancel")
        secondaryText: qsTr("Delete")
        onSecondaryActivated: root.core.decideOutbox(root.asking, "delete")
    }
}
