import QtQuick 2.15
import "../../platform"
import "../../components"

// The folders menu (board 10), a page over the inbox: the unified inbox and
// the Outbox, then each account with its folders (the open account's list
// comes from `core.listFolders`). Taps leave through signals.
Rectangle {
    id: root
    required property Theme theme
    required property var core
    required property var accounts
    required property var folders
    property int outboxCount: 0
    // The account whose folders are shown.
    property string expanded: ""
    readonly property int unreadTotal: probe.total
    signal closeRequested()
    signal unifiedRequested()
    signal folderRequested(var folder, string label)
    signal outboxRequested()
    signal accountsRequested()
    signal settingsRequested()

    color: theme.ground

    function expand(key) {
        expanded = key
        core.drawerAccount(key)
        core.listFolders(key)
    }
    // Every opening lists the open account again, so its counts are current;
    // the core re-lists them while the drawer is open on an account.
    onVisibleChanged: {
        // Visibility can change while the page is still being built.
        if (!core) return
        if (!visible) {
            core.drawerAccount("")
            return
        }
        const key = expanded !== "" ? expanded : probe.firstKey
        if (key !== "") expand(key)
    }

    // The accounts' keys and unread counts, for the Unified inbox row.
    Repeater {
        id: probe
        property int total: 0
        property string firstKey: ""
        function recount() {
            let n = 0
            for (let i = 0; i < count; i++) {
                const it = itemAt(i)
                if (it !== null) n += it.unread
            }
            total = n
            firstKey = count > 0 && itemAt(0) !== null ? itemAt(0).key : ""
            // The open account is gone: fall back to the first one.
            if (root.expanded !== "") {
                let found = false
                for (let i = 0; i < count; i++)
                    if (itemAt(i) !== null && itemAt(i).key === root.expanded) found = true
                if (!found) {
                    root.expanded = ""
                    if (root.visible && firstKey !== "") root.expand(firstKey)
                    else if (root.visible) core.drawerAccount("")
                }
            }
        }
        model: root.accounts
        delegate: Item {
            visible: false
            required property var model
            readonly property int unread: model.unread
            readonly property string key: model.key
            onUnreadChanged: probe.recount()
        }
        onItemAdded: recount()
        onItemRemoved: recount()
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        compact: true
        title: qsTr("Mail")
        onBackClicked: root.closeRequested()
    }

    Flickable {
        anchors { top: bar.bottom; bottom: footer.top; left: parent.left; right: parent.right }
        contentHeight: column.height
        clip: true
        Column {
            id: column
            width: parent.width

            component Section: Text {
                textFormat: Text.PlainText
                leftPadding: root.theme.gu(2.5)
                topPadding: root.theme.gu(1.5)
                bottomPadding: root.theme.gu(0.75)
                color: root.theme.secondary
                font.family: root.theme.eyebrowFamily
                font.pixelSize: root.theme.eyebrowSize
                font.weight: root.theme.eyebrowWeight
                font.letterSpacing: root.theme.eyebrowSpacing
                font.capitalization: root.theme.eyebrowCaps ? Font.AllUppercase : Font.MixedCase
            }
            component CountText: Text {
                textFormat: Text.PlainText
                property int n: 0
                visible: n > 0
                text: n
                color: root.theme.accent
                font.family: root.theme.fontLabel
                font.pixelSize: 13
                font.weight: Font.DemiBold
            }

            Section { text: qsTr("All accounts") }
            ListRow {
                objectName: "unifiedRow"
                theme: root.theme
                width: column.width
                separator: false
                onClicked: root.unifiedRequested()
                Text {
                    textFormat: Text.PlainText
                    x: root.theme.gu(2.5)
                    y: root.theme.gu(1.75)
                    text: qsTr("Unified inbox")
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 16
                }
                CountText {
                    n: root.unreadTotal
                    anchors.right: parent.right
                    anchors.rightMargin: root.theme.gu(2.5)
                    y: root.theme.gu(1.75)
                }
            }
            ListRow {
                objectName: "outboxRow"
                theme: root.theme
                width: column.width
                separator: false
                onClicked: root.outboxRequested()
                Row {
                    x: root.theme.gu(2.5)
                    y: root.theme.gu(1.75)
                    spacing: root.theme.gu(0.75)
                    Text {
                        textFormat: Text.PlainText
                        text: qsTr("Outbox")
                        color: root.theme.text
                        font.family: root.theme.fontUi
                        font.pixelSize: 16
                    }
                    Text {
                        textFormat: Text.PlainText
                        visible: root.outboxCount > 0
                        text: qsTr("waiting to send")
                        color: root.theme.secondary
                        font.family: root.theme.fontUi
                        font.pixelSize: 12
                        topPadding: 3
                    }
                }
                CountText {
                    n: root.outboxCount
                    anchors.right: parent.right
                    anchors.rightMargin: root.theme.gu(2.5)
                    y: root.theme.gu(1.75)
                }
            }

            Repeater {
                model: root.accounts
                delegate: Column {
                    id: acct
                    required property var model
                    width: column.width
                    Rectangle { width: parent.width; height: 1; color: root.theme.hairline }
                    ListRow {
                        id: group
                        objectName: "account-" + acct.model.key
                        theme: root.theme
                        width: parent.width
                        separator: false
                        chevron: true
                        chevronDirection: root.expanded === acct.model.key ? "down" : "forward"
                        onClicked: root.expand(acct.model.key)
                        readonly property bool stopped: acct.model.attention === true
                        Accessible.name: stopped ? qsTr("%1, sync stopped").arg(accountName.text)
                                                 : (offlineIcon.visible ? qsTr("%1, offline").arg(accountName.text) : accountName.text)
                        // The state icon: cloud-off while offline, the alert sign
                        // while sync is stopped (a tap on it opens Accounts).
                        Icon {
                            id: offlineIcon
                            objectName: "offlineIcon"
                            visible: group.stopped || acct.model.state === "offline"
                            theme: root.theme
                            name: group.stopped ? "alert" : "offline"
                            color: root.theme.accent
                            size: root.theme.gu(2.5)
                            x: root.theme.gu(2.5)
                            y: root.theme.gu(1.75) + (accountName.height - height) / 2
                            MouseArea {
                                objectName: "stoppedLink"
                                enabled: group.stopped
                                // The icon's own cell: from the row's edge to the
                                // account name, so a tap on the name still expands.
                                x: -offlineIcon.x
                                width: offlineIcon.x + offlineIcon.width + root.theme.gu(1)
                                height: root.theme.touch
                                anchors.verticalCenter: parent.verticalCenter
                                Accessible.role: Accessible.Button
                                Accessible.name: qsTr("Open accounts")
                                onClicked: root.accountsRequested()
                            }
                        }
                        Text {
                            id: accountName
                            objectName: "accountName"
                            textFormat: Text.PlainText
                            x: root.theme.gu(2.5) + (offlineIcon.visible ? offlineIcon.width + root.theme.gu(1) : 0)
                            y: root.theme.gu(1.75)
                            width: parent.width - x - group.chevronSpace
                            text: acct.model.heading
                            elide: Text.ElideRight
                            color: root.theme.heading
                            font.family: root.theme.fontUi
                            font.pixelSize: 16
                            font.weight: Font.Medium
                        }
                    }
                    Repeater {
                        model: root.folders
                        delegate: ListRow {
                            id: folder
                            required property var model
                            objectName: "folder-" + model.id
                            theme: root.theme
                            width: acct.width
                            separator: false
                            visible: model.account === acct.model.key && root.expanded === acct.model.key
                            height: visible ? implicitHeight : 0
                            onClicked: root.folderRequested(folder.model.id, label.text)
                            FolderLabel {
                                id: label
                                x: root.theme.gu(2.5) + folder.model.depth * root.theme.gu(2)
                                y: root.theme.gu(1.75)
                                width: parent.width - x - root.theme.gu(8)
                                theme: root.theme
                                role: folder.model.role
                                folderName: folder.model.name
                            }
                            CountText {
                                n: folder.model.unread
                                anchors.right: parent.right
                                anchors.rightMargin: root.theme.gu(2.5)
                                y: root.theme.gu(1.75)
                            }
                        }
                    }
                }
            }
        }
    }

    Row {
        id: footer
        anchors { bottom: parent.bottom; left: parent.left; right: parent.right; margins: root.theme.gu(1) }
        spacing: root.theme.gu(1)
        AppButton {
            objectName: "accountsButton"
            theme: root.theme
            width: (parent.width - parent.spacing) / 2
            text: qsTr("Accounts")
            onClicked: root.accountsRequested()
        }
        AppButton {
            objectName: "settingsButton"
            theme: root.theme
            width: (parent.width - parent.spacing) / 2
            text: qsTr("Settings")
            onClicked: root.settingsRequested()
        }
    }
}
