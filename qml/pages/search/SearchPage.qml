import QtQuick 2.15
import QtQuick.Controls 2.15 as QC
import "../../platform"
import "../../components"
import "../inbox"

// Search (board 11): a field with scope chips, the answer from this
// phone first, then "Search the servers too" for older mail. The rows are
// `results` (the search model); Rust drops answers to any question but the
// last, so Cancel just asks the same question locally again. Failures come
// from the core, one line per account that could not be asked.
Item {
    id: root
    objectName: "searchPage"
    required property Theme theme
    required property var core
    required property var accounts
    // The search model (the message roles).
    required property var results
    // The folder the inbox was showing, -1 for the unified inbox.
    property int folderId: -1
    // "all", "account:<key>" or "folder:<id>".
    property string scope: "all"
    property bool serverBusy: false
    property var failures: []
    readonly property string query: field.text.trim()

    signal backRequested()
    signal messageRequested(var message)

    // Asks the core; a new question forgets the last one's failures and
    // stops waiting for the servers.
    function ask(server) {
        failures = []
        serverBusy = server
        if (query !== "") core.search(scope, query, server)
    }
    function choose(value) {
        scope = value
        ask(false)
    }
    function cancelServer() {
        serverBusy = false
        core.search(scope, query, false)
    }

    Timer {
        id: typing
        interval: 250
        onTriggered: root.ask(false)
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        title: ""
        onBackClicked: root.backRequested()
    }
    // The field sits in the top bar, to the right of the back button.
    Field {
        id: field
        objectName: "searchField"
        theme: root.theme
        anchors { left: parent.left; right: parent.right; top: parent.top }
        anchors.leftMargin: root.theme.touch + root.theme.gu(1)
        anchors.rightMargin: root.theme.gu(2)
        anchors.topMargin: root.theme.gu(0.5)
        placeholder: qsTr("Search mail")
        inputMethodHints: Qt.ImhNoPredictiveText
        Connections {
            target: field.input
            function onTextChanged() { typing.restart() }
        }
        onAccepted: { typing.stop(); root.ask(false) }
    }

    Flow {
        id: chips
        anchors { top: bar.bottom; left: parent.left; right: parent.right; margins: root.theme.gu(2) }
        anchors.topMargin: root.theme.gu(1)
        spacing: root.theme.gu(0.75)
        Chip {
            objectName: "chip-all"
            theme: root.theme
            text: qsTr("All accounts")
            selected: root.scope === "all"
            onClicked: root.choose("all")
        }
        Repeater {
            model: root.accounts
            delegate: Chip {
                required property var model
                objectName: "chip-account-" + model.key
                theme: root.theme
                text: model.name
                selected: root.scope === "account:" + model.key
                onClicked: root.choose("account:" + model.key)
            }
        }
        Chip {
            objectName: "chip-folder"
            theme: root.theme
            visible: root.folderId >= 0
            text: qsTr("This folder")
            selected: root.scope === "folder:" + root.folderId
            onClicked: root.choose("folder:" + root.folderId)
        }
    }

    Text {
        id: label
        objectName: "resultCount"
        anchors { top: chips.bottom; left: parent.left; right: parent.right; margins: root.theme.gu(2) }
        anchors.topMargin: root.theme.gu(1)
        visible: root.query !== ""
        height: visible ? implicitHeight : 0
        textFormat: Text.PlainText
        text: root.core.countText("results", list.count)
        color: root.theme.secondary
        font.family: root.theme.fontMono
        font.pixelSize: 11
        font.letterSpacing: 0.6
        font.capitalization: Font.AllUppercase
    }

    Column {
        id: failed
        anchors { top: label.bottom; left: parent.left; right: parent.right }
        anchors.topMargin: root.theme.gu(0.5)
        Repeater {
            model: root.failures
            delegate: Banner {
                required property string modelData
                objectName: "failedLine"
                theme: root.theme
                width: failed.width
                kind: "warning"
                text: modelData
            }
        }
    }

    ListView {
        id: list
        objectName: "searchResults"
        anchors { top: failed.bottom; bottom: footer.top; left: parent.left; right: parent.right }
        clip: true
        visible: root.query !== ""
        model: root.results
        delegate: MessageRow {
            id: row
            theme: root.theme
            width: list.width
            showAccount: true
            onClicked: root.messageRequested(row.model.id)
        }
    }

    Column {
        id: footer
        anchors { bottom: parent.bottom; left: parent.left; right: parent.right; margins: root.theme.gu(2) }
        anchors.bottomMargin: root.theme.gu(2)
        spacing: root.theme.gu(1)
        visible: root.query !== ""
        Row {
            visible: root.serverBusy
            width: parent.width
            spacing: root.theme.gu(1.5)
            QC.BusyIndicator {
                objectName: "searchBusy"
                running: root.serverBusy
                width: root.theme.touch
                height: root.theme.touch
            }
            Text {
                objectName: "searchingText"
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - root.theme.touch - parent.spacing
                textFormat: Text.PlainText
                text: qsTr("Searching the servers…")
                wrapMode: Text.WordWrap
                color: root.theme.text
                font.family: root.theme.fontUi
                font.pixelSize: 15
            }
        }
        AppButton {
            objectName: "cancelServerSearch"
            visible: root.serverBusy
            width: parent.width
            theme: root.theme
            text: qsTr("Cancel")
            onClicked: root.cancelServer()
        }
        AppButton {
            objectName: "searchServers"
            visible: !root.serverBusy
            width: parent.width
            theme: root.theme
            iconName: "search"
            text: qsTr("Search the servers too")
            onClicked: root.ask(true)
        }
        Text {
            visible: !root.serverBusy
            width: parent.width
            textFormat: Text.PlainText
            horizontalAlignment: Text.AlignHCenter
            text: qsTr("Finds mail older than the 30 days kept here")
            wrapMode: Text.WordWrap
            color: root.theme.secondary
            font.family: root.theme.fontUi
            font.pixelSize: 13
        }
    }

    Connections {
        target: root.core
        function onSearchFailed(text) { root.failures = root.failures.concat([text]) }
        function onSearchDone() { root.serverBusy = false }
    }
}
