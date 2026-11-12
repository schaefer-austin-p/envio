import QtQuick 2.15
import "../../platform"
import "../../components"

// Writing a message (reply, forward, new, a draft, an Outbox item or a
// mailto: link). The core owns the draft: it fills the page with
// `composeReady`, and every edit goes back as `draftChanged` (the core
// throttles the saving). Nothing here builds mail; the page hands over the
// user's fields as JSON and the core builds the message when it is sent.
// The quote of a reply or forward stays folded and is appended to the body
// unless the user removes it.
Item {
    id: root
    objectName: "composePage"
    required property Theme theme
    required property var core
    required property var accounts
    required property var suggestions
    // The compose id `startCompose` returned.
    property real compose: -1
    // The platform's file picker; the tests replace it (the Content Hub only
    // exists on the phone).
    property var importFiles: function() { content.importFiles() }

    // The message was queued: the Outbox has it. The text is core's, for a
    // toast.
    signal sent(string text)
    // Back or Discard: the page is done.
    signal closed()

    property bool filled: false
    property bool filling: false
    property bool sending: false
    property bool showCc: false
    property string account: ""
    property string draftId: ""
    property string inReplyTo: ""
    property var references: []
    property string quoted: ""
    property bool keepQuoted: true
    // [{ path, name, size }]
    property var attachments: []
    property string error: ""

    function reset() {
        filled = false
        sending = false
        showCc = false
        account = ""
        draftId = ""
        inReplyTo = ""
        references = []
        quoted = ""
        keepQuoted = true
        attachments = []
        error = ""
        fold.reset()
        filling = true
        to.setEntries([]); cc.setEntries([]); bcc.setEntries([])
        to.input.text = cc.input.text = bcc.input.text = ""
        subject.text = ""
        body.text = ""
        filling = false
    }

    function nameOf(path) { return path.split("/").pop() }

    function fill(draft, quote) {
        filling = true
        account = draft.account
        draftId = draft.draftId || ""
        inReplyTo = draft.inReplyTo || ""
        references = draft.references || []
        to.setEntries(draft.to || [])
        cc.setEntries(draft.cc || [])
        bcc.setEntries(draft.bcc || [])
        showCc = cc.entries.length > 0 || bcc.entries.length > 0
        subject.text = draft.subject || ""
        body.text = draft.body || ""
        attachments = (draft.attachments || []).map(function(p) { return { path: p, name: nameOf(p), size: -1 } })
        quoted = quote
        keepQuoted = true
        fold.reset()
        error = ""
        sending = false
        filling = false
        filled = true
    }

    // What the user typed in the address fields and has not committed yet
    // becomes addresses now.
    function commitAll() {
        to.commit(true)
        cc.commit(true)
        bcc.commit(true)
    }
    function draftJson() {
        const tail = keepQuoted && quoted !== "" ? "\n\n" + quoted : ""
        return JSON.stringify({
            account: account,
            draftId: draftId,
            to: to.all(),
            cc: cc.all(),
            bcc: bcc.all(),
            subject: subject.text,
            body: body.text + tail,
            inReplyTo: inReplyTo,
            references: references,
            attachments: attachments.map(function(a) { return a.path })
        })
    }
    // Any edit: the core keeps the draft.
    function edited() {
        if (filling || !filled) return
        error = ""
        core.draftChanged(compose, draftJson())
    }
    function pick(paths) {
        for (const p of paths) core.attach(compose, p)
    }
    function leave() {
        commitAll()
        core.closeCompose(compose, false)
        closed()
    }
    function send() {
        commitAll()
        error = ""
        sending = true
        core.send(compose, draftJson())
    }

    TopBar {
        id: bar
        theme: root.theme
        width: parent.width
        showBack: true
        backIcon: "close"
        compact: true
        title: qsTr("Write")
        onBackClicked: root.leave()
        IconButton {
            objectName: "attachButton"
            theme: root.theme
            iconName: "clip"
            text: qsTr("Attach a file")
            enabled: root.filled
            onClicked: root.importFiles()
        }
        IconButton {
            objectName: "moreButton"
            theme: root.theme
            iconName: "more"
            text: qsTr("More actions")
            onClicked: more.open()
        }
        AppButton {
            objectName: "sendButton"
            theme: root.theme
            primary: true
            iconName: "send"
            visibleHeight: theme.compactControlHeight
            // The tap target stays inside the bar's button row.
            implicitHeight: Math.min(theme.controlHeight, theme.touch)
            text: qsTr("Send")
            enabled: root.filled && !root.sending
            onClicked: root.send()
        }
    }

    Banner {
        id: errorBanner
        objectName: "composeError"
        anchors.top: bar.bottom
        width: parent.width
        theme: root.theme
        kind: "warning"
        text: root.error
        visible: root.error !== ""
        height: visible ? implicitHeight : 0
    }

    Flickable {
        id: flick
        anchors { top: errorBanner.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        clip: true
        contentHeight: column.height
        boundsBehavior: Flickable.StopAtBounds

        KeyboardAvoider { objectName: "keyboardAvoider"; flickable: flick }

        Column {
            id: column
            width: flick.width
            FromRow {
                width: parent.width
                theme: root.theme
                accounts: root.accounts
                account: root.account
                onChosen: function(key) { root.account = key; root.edited() }
            }
            RecipientField {
                id: to
                objectName: "to"
                width: parent.width
                theme: root.theme
                core: root.core
                suggestions: root.suggestions
                label: qsTr("To")
                onEdited: root.edited()
                onAsked: { cc.asking = false; bcc.asking = false }
            }
            RecipientField {
                id: cc
                objectName: "ccField"
                width: parent.width
                visible: root.showCc
                height: visible ? implicitHeight : 0
                theme: root.theme
                core: root.core
                suggestions: root.suggestions
                label: qsTr("Cc")
                onEdited: root.edited()
                onAsked: { to.asking = false; bcc.asking = false }
            }
            RecipientField {
                id: bcc
                objectName: "bccField"
                width: parent.width
                visible: root.showCc
                height: visible ? implicitHeight : 0
                theme: root.theme
                core: root.core
                suggestions: root.suggestions
                label: qsTr("Bcc")
                onEdited: root.edited()
                onAsked: { to.asking = false; cc.asking = false }
            }
            AppButton {
                objectName: "showCc"
                x: root.theme.gu(2)
                visible: !root.showCc
                theme: root.theme
                text: qsTr("Cc and Bcc")
                onClicked: root.showCc = true
            }
            Field {
                id: subject
                objectName: "subjectField"
                x: root.theme.gu(2)
                width: parent.width - root.theme.gu(4)
                topPadding: root.theme.gu(1.5)
                theme: root.theme
                label: qsTr("Subject")
                onTextChanged: root.edited()
            }
            AttachmentChips {
                width: parent.width
                theme: root.theme
                files: root.attachments
                onRemoveRequested: function(index) {
                    const list = root.attachments.slice()
                    list.splice(index, 1)
                    root.attachments = list
                    root.edited()
                }
            }
            TextBlock {
                id: body
                objectName: "bodyField"
                width: parent.width
                theme: root.theme
                placeholder: qsTr("Write your message")
                Connections {
                    target: body.input
                    function onTextChanged() { root.edited() }
                }
            }
            QuoteFold {
                id: fold
                width: parent.width
                theme: root.theme
                quoted: root.quoted
                visible: root.quoted !== "" && root.keepQuoted
                height: visible ? implicitHeight : 0
                onRemoved: { root.keepQuoted = false; root.edited() }
            }
        }
    }

    Toast {
        id: toast
        objectName: "composeToast"
        theme: root.theme
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: root.theme.gu(2) }
    }

    DiscardActions {
        id: more
        theme: root.theme
        onDiscarded: {
            root.core.closeCompose(root.compose, true)
            root.closed()
        }
    }

    Content {
        id: content
        anchors.fill: parent
        theme: root.theme
        onFilesImported: function(paths) { root.pick(paths) }
    }

    Connections {
        target: root.core
        function onComposeReady(compose, draft, quote) {
            if (compose === root.compose) root.fill(draft, quote)
        }
        function onComposeRefused(compose, text) {
            if (compose !== root.compose) return
            root.sending = false
            root.error = text
        }
        function onQueued(compose, text) {
            if (compose === root.compose) root.sent(text)
        }
        function onAttached(compose, path, name, size) {
            if (compose !== root.compose) return
            if (root.attachments.some(function(a) { return a.path === path })) return
            root.attachments = root.attachments.concat([{ path: path, name: name, size: size }])
            root.edited()
        }
        function onAttachFailed(compose, text) {
            if (compose === root.compose) toast.show(text)
        }
    }
}
