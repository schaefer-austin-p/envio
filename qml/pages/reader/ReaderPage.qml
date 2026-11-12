import QtQuick 2.15
import "../../platform"
import "../../components"
import "../inbox"

// Reading one message (boards 2, 3, 4): header, the body as rich text with
// remote images blocked until the user allows them, attachments, and the
// actions. It asks the core to `open` the message when it is created and
// shows what `messageOpened` brings. A link only ever opens from the link
// sheet, after the user has seen its real address.
Item {
    id: root
    objectName: "readerPage"
    required property Theme theme
    required property var core
    required property var accounts
    required property var folders
    property real messageId: -1

    // Set by messageOpened; null until then.
    property var view: null
    property string unavailable: ""
    property string accountName: ""
    // "open", "save" or "share" while an attachment is being saved.
    property string pendingMode: ""
    property bool started: false
    property bool moved: false
    readonly property var noView: ({ from: "", to: [], cc: [], subject: "", date: 0, account: "" })
    // The platform's way to open a link and to export a file; the tests
    // replace them (neither has a visible effect on the desktop).
    property var openUrl: function(url) { Qt.openUrlExternally(url) }
    property var exportFile: function(path, mode) { content.exportFile(path, mode) }

    // The body's width in pixels, which the core scales images to.
    readonly property int bodyWidth: Math.max(0, Math.round(width - theme.gu(7)))

    // Leaving the page: Back, or a message that is no longer in this list.
    signal backRequested()
    // The compose id `startCompose` returned; the compose page follows it.
    signal composeOpened(var compose)

    function load(images) {
        unavailable = ""
        core.open(messageId, images, bodyWidth)
    }
    function start() {
        if (started || messageId < 0 || width <= 0) return
        started = true
        load(false)
    }
    onWidthChanged: start()
    onMessageIdChanged: { started = false; view = null; start() }
    Component.onCompleted: start()

    // Archive and Delete leave the page (the inbox offers the Undo); the
    // other actions go to the core the way the inbox's selection bar does:
    // this message is the selection. When sync has pruned the row the
    // action does not run: the page asks again, and the core answers with
    // its unavailable notice if the message is gone.
    function act(action, leave) {
        core.select(messageId, true)
        if (!core.act(action, 0)) {
            load(false)
            return
        }
        if (leave) backRequested()
    }
    function compose(kind) {
        const id = core.startCompose(kind, messageId, view.account, "")
        if (id > 0) composeOpened(id)
    }
    // A forward carries the message's attachments; the compose page shows
    // them as chips the user can remove.
    function forward() { compose("forward") }
    function save(mode) {
        pendingMode = mode
        core.saveAttachment(messageId, attachmentSheet.attachment.index, content.attachmentsDir)
    }

    // The account's name for the header, from the accounts model.
    Repeater {
        model: root.accounts
        delegate: Item {
            id: account
            required property var model
            visible: false
            Binding {
                target: root
                property: "accountName"
                value: account.model.name
                when: root.view !== null && account.model.key === root.view.account
            }
        }
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        onBackClicked: root.backRequested()
        IconButton {
            objectName: "act-archive"
            theme: root.theme
            iconName: "archive"
            text: qsTr("Archive")
            enabled: root.view !== null
            onClicked: root.act("archive", true)
        }
        IconButton {
            objectName: "act-delete"
            theme: root.theme
            iconName: "trash"
            text: qsTr("Delete")
            enabled: root.view !== null
            onClicked: root.act("delete", true)
        }
        IconButton {
            objectName: "act-more"
            theme: root.theme
            iconName: "more"
            text: qsTr("More actions")
            enabled: root.view !== null
            onClicked: more.open()
        }
    }

    ImagesBanner {
        id: banner
        anchors.top: bar.bottom
        width: parent.width
        theme: root.theme
        objectName: "imagesBanner"
        visible: root.view !== null && root.view.blockedImages > 0
        height: visible ? implicitHeight : 0
        onLoadRequested: root.load(true)
        onAlwaysRequested: {
            root.core.allowImagesFrom(root.view.fromAddress)
            // The sender is allowed now, so a plain open shows the images.
            root.load(false)
        }
    }

    Flickable {
        id: flick
        anchors { top: banner.bottom; bottom: footer.top; left: parent.left; right: parent.right }
        visible: root.view !== null
        clip: true
        contentHeight: page.height + root.theme.gu(3)
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: page
            x: root.theme.gu(2)
            width: flick.width - root.theme.gu(4)
            topPadding: root.theme.gu(1.5)
            spacing: root.theme.gu(2)

            HeaderBlock {
                width: parent.width
                theme: root.theme
                view: root.view !== null ? root.view : root.noView
                accountName: root.accountName
            }
            Rectangle {
                width: parent.width
                height: body.height + root.theme.gu(3)
                radius: root.theme.gu(1)
                color: root.theme.surface
                Text {
                    id: body
                    objectName: "bodyText"
                    x: root.theme.gu(1.5)
                    y: root.theme.gu(1.5)
                    width: parent.width - root.theme.gu(3)
                    // The core's HTML has no remote image unless the user
                    // allowed it; a link is only ever shown, never followed.
                    text: root.view !== null ? root.view.html : ""
                    textFormat: Text.RichText
                    wrapMode: Text.Wrap
                    linkColor: root.theme.accent
                    color: root.theme.text
                    font.family: root.theme.fontUi
                    font.pixelSize: 16
                    onLinkActivated: linkSheet.show(link)
                }
            }
            AttachmentList {
                width: parent.width
                theme: root.theme
                files: root.view !== null ? root.view.attachments : []
                onFileClicked: attachmentSheet.showFor(file)
            }
        }
    }

    Text {
        textFormat: Text.PlainText
        anchors.centerIn: parent
        visible: root.view === null && root.unavailable === ""
        text: qsTr("Loading…")
        color: root.theme.secondary
        font.family: root.theme.fontUi
        font.pixelSize: 15
    }
    Column {
        anchors.centerIn: parent
        width: parent.width - root.theme.gu(6)
        visible: root.unavailable !== ""
        spacing: root.theme.gu(2)
        Text {
            textFormat: Text.PlainText
            objectName: "unavailableText"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.unavailable
            wrapMode: Text.WordWrap
            color: root.theme.text
            font.family: root.theme.fontUi
            font.pixelSize: 16
        }
        AppButton {
            objectName: "retry"
            anchors.horizontalCenter: parent.horizontalCenter
            theme: root.theme
            text: qsTr("Try again")
            onClicked: root.load(false)
        }
    }

    ReplyBar {
        id: footer
        anchors { bottom: parent.bottom; left: parent.left; right: parent.right }
        theme: root.theme
        visible: root.view !== null
        onReplyRequested: root.compose("reply")
        onForwardRequested: root.forward()
    }

    ReaderMore {
        id: more
        theme: root.theme
        onReplyAllRequested: root.compose("replyall")
        onMoveRequested: {
            root.moved = false
            root.core.select(root.messageId, true)
            moveSheet.openFor(root.view.account)
        }
        onFlagRequested: root.act(flagged ? "flag" : "unflag", false)
        onContactRequested: root.core.addContact(root.view.from, root.view.fromAddress)
    }
    MoveSheet {
        id: moveSheet
        theme: root.theme
        core: root.core
        folders: root.folders
        // The sheet acts on the selection itself. Closed without a choice,
        // nothing may stay selected in the inbox below.
        onChosen: { root.moved = true; root.backRequested() }
        onClosed: Qt.callLater(function() { if (!root.moved) root.core.clearSelection() })
    }
    LinkSheet {
        id: linkSheet
        theme: root.theme
        core: root.core
        onOpenRequested: root.openUrl(url)
    }
    AttachmentsSheet {
        id: attachmentSheet
        theme: root.theme
        onChosen: root.save(mode)
    }
    Content {
        id: content
        anchors.fill: parent
        theme: root.theme
    }

    Connections {
        target: root.core
        // A failed save says so with attachmentFailed (and a notice): forget
        // the pending export, or the next save would be exported in its
        // mode. Other notices are not the save's business.
        function onAttachmentFailed(id) {
            // A late failure from an earlier page's save is not ours.
            if (id === root.messageId) root.pendingMode = ""
        }
        function onMessageOpened(view) {
            if (view.id !== root.messageId) return
            root.unavailable = ""
            root.view = view
        }
        function onMessageUnavailable(id, text) {
            if (id !== root.messageId) return
            root.view = null
            root.unavailable = text
        }
        function onAttachmentSaved(path) {
            if (root.pendingMode === "") return
            const mode = root.pendingMode
            root.pendingMode = ""
            root.exportFile(path, mode)
        }
    }
}
