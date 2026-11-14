import QtQuick 2.15
import QtQuick.Controls 2.15
import "../../platform"
import "../../components"
import "../inbox"
import "../../components/richText.js" as RichText

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
    // The unavailable text is the size-cap notice: offer "Load anyway".
    property bool tooLarge: false
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

    // The body is laid out once, after the push transition ends (a page
    // that is not in a StackView, as in the tests, is settled at once): a
    // rich-text relayout costs hundreds of ms on a big message.
    readonly property bool settled: StackView.view === null || StackView.status === StackView.Active
    // Once settled it stays so: a page pushed over the reader (reply,
    // forward) makes the status Inactive, and the body must not go away
    // and lay out again on Back.
    property bool everSettled: false
    onSettledChanged: if (settled) everSettled = true
    readonly property bool ready: view !== null && everSettled
    // What the body shows (a rich TextEdit's own `text` is Qt's re-serialised
    // HTML, so this is the value to read; an image redraw adds only an empty
    // comment).
    // Mail that sets its own colours (the core says so) sits on a light
    // paper card in both themes; nothing here reads the HTML.
    readonly property bool paper: view !== null && view.ownColours === true
    readonly property string bodyHtml: ready ? RichText.withLinkColor(view.html, String(paper ? theme.paperLink : theme.link), theme.bodyLineHeight) : ""

    // Qt 5's rich TextEdit reserves a remote image's space but never draws
    // it once it arrives (#163). Hidden Images load the same URLs into Qt's
    // pixmap cache; each time some are in, the body is set again (a
    // different string, so the TextEdit lays out) and draws them from there.
    property int imageRedraws: 0
    readonly property var remoteImages: view !== null && view.remoteImages !== undefined ? view.remoteImages : []
    // A redraw due for the last message must not lay out the next one.
    onViewChanged: imageRedraw.stop()

    // The body's width in pixels, which the core scales images to.
    readonly property int bodyWidth: Math.max(0, Math.round(width - theme.gu(7)))

    // Leaving the page: Back, or a message that is no longer in this list.
    signal backRequested()
    // The compose id `startCompose` returned; the compose page follows it.
    signal composeOpened(var compose)

    // The user chose to load this message past the size cap; later loads
    // (images, retries) keep that choice.
    property bool largeAllowed: false
    function load(images, anyway) {
        if (anyway) largeAllowed = true
        unavailable = ""
        tooLarge = false
        if (largeAllowed) core.openAnyway(messageId, images, bodyWidth)
        else core.open(messageId, images, bodyWidth)
    }
    function start() {
        if (started || messageId < 0 || width <= 0) return
        started = true
        load(false)
    }
    onWidthChanged: start()
    onMessageIdChanged: { started = false; view = null; largeAllowed = false; start() }
    Component.onCompleted: { if (settled) everSettled = true; start() }

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

    Repeater {
        model: root.remoteImages
        delegate: Image {
            required property string modelData
            objectName: "preloadedImage"
            visible: false
            source: modelData
            onStatusChanged: if (status === Image.Ready) imageRedraw.restart()
        }
    }
    // Several images arriving close together cost one layout.
    Timer {
        id: imageRedraw
        interval: 300
        onTriggered: root.imageRedraws += 1
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
        visible: root.ready
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
                objectName: "bodyCard"
                width: parent.width
                height: body.height + root.theme.gu(3)
                radius: root.theme.cardRadius
                color: root.paper ? root.theme.paper : root.theme.surface
                // A read-only TextEdit: Qt 5 lays rich text out about 6x
                // faster there than in a Text. No cursor, focus or editing.
                TextEdit {
                    id: body
                    objectName: "bodyText"
                    x: root.theme.gu(1.5)
                    y: root.theme.gu(1.5)
                    width: parent.width - root.theme.gu(3)
                    readOnly: true
                    activeFocusOnPress: false
                    selectByMouse: false
                    cursorVisible: false
                    // The core's HTML has no remote image unless the user
                    // allowed it; a link is only ever shown, never followed.
                    // An empty comment after an image redraw: same layout,
                    // but a new string, so the TextEdit takes the images
                    // from the cache.
                    text: root.bodyHtml + (root.imageRedraws % 2 === 1 ? "<!---->" : "")
                    textFormat: TextEdit.RichText
                    wrapMode: TextEdit.Wrap
                    color: root.paper ? root.theme.paperText : root.theme.text
                    font.family: root.theme.fontBody
                    font.pixelSize: root.theme.bodySize
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

    BusyLine {
        objectName: "loadingText"
        theme: root.theme
        anchors.centerIn: parent
        width: Math.min(implicitWidth, parent.width)
        running: !root.ready && root.unavailable === ""
        text: qsTr("Loading…")
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
            visible: !root.tooLarge
            onClicked: root.load(false)
        }
        AppButton {
            objectName: "loadAnyway"
            anchors.horizontalCenter: parent.horizontalCenter
            theme: root.theme
            text: qsTr("Load anyway")
            visible: root.tooLarge
            onClicked: root.load(false, true)
        }
    }

    ReplyBar {
        id: footer
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
            root.tooLarge = false
            root.unavailable = text
        }
        function onMessageTooLarge(id, text) {
            if (id !== root.messageId) return
            root.view = null
            root.tooLarge = true
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
