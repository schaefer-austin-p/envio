import QtQuick 2.15
import "../../platform"
import "../../components"
import "../../bindle/Platform" as Platform

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
    // In the right pane of the wide layout: fields 5 gu in from the edge, 90 gu at most.
    property bool wide: false
    // The platform's file picker; the tests replace it (the Content Hub only
    // exists on the phone).
    property var importFiles: function() { content.importFiles() }
    // Where the From, To, Cc, Bcc and Subject values start: the page margin,
    // the widest label (at least the design's 52 px) and a gap, so a long
    // translated label never runs into its value.
    readonly property real labelColumn: {
        // Reading the font makes the column follow it once the UI font loads.
        void labelFont.font
        let widest = theme.gu(6.5)
        for (const t of [qsTr("From"), qsTr("To"), qsTr("Cc"), qsTr("Bcc"), qsTr("Subject")])
            widest = Math.max(widest, labelFont.advanceWidth(t))
        return theme.gu(2) + Math.ceil(widest) + theme.gu(1)
    }
    FontMetrics { id: labelFont; font.family: root.theme.fontUi; font.pixelSize: 14 }

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
    // Everything the page holds, for another page of the same compose (the
    // pane's page goes when the window narrows). Nothing is sent to the core.
    function snapshot() {
        commitAll()
        return {
            filled: filled, sending: sending, error: error, showCc: showCc, quoted: quoted, keepQuoted: keepQuoted, expanded: fold.expanded,
            attachments: attachments.slice(),
            draft: {
                account: account, draftId: draftId, to: to.all(), cc: cc.all(), bcc: bcc.all(),
                subject: subject.text, body: body.text, inReplyTo: inReplyTo, references: references,
                attachments: []
            }
        }
    }
    // A page that has not been filled yet gets its composeReady as usual.
    function restore(state) {
        if (!state.filled) return
        fill(state.draft, state.quoted)
        showCc = state.showCc
        keepQuoted = state.keepQuoted
        fold.expanded = state.expanded
        attachments = state.attachments
        // Between Send and the answer the new page waits too (no second send).
        sending = state.sending
        error = state.error
    }
    // Hands the core the fields as they are now, at once, without waiting for
    // an edit (the shell calls it before it closes this compose for a row
    // click). The core's SaveThrottle does the throttling; this is a plain
    // draftChanged.
    function flush() {
        if (!filled) return
        commitAll()
        core.draftChanged(compose, draftJson())
    }
    // The focused field, its cursor and the scroll position, for a move of
    // this page to another parent (the window narrowed or widened). The page
    // itself moves, so the text and the chips stay; focus is lost on the
    // move and has to be given back.
    function viewState() {
        const fields = { body: body.input, subject: subject.input, to: to.input, cc: cc.input, bcc: bcc.input }
        let focus = ""
        for (const k in fields) if (fields[k].activeFocus) focus = k
        return { focus: focus, cursor: focus !== "" ? fields[focus].cursorPosition : 0, contentY: resizing ? settledY : flick.contentY }
    }
    // After the move and the next layout. The layout lands in steps, so the
    // scroll is applied again whenever the content grows until the page has
    // been still for a moment.
    property var viewWanted: null
    // A width change makes the text re-wrap, and the field scrolls its cursor
    // into view on its own; that scroll is not the user's. Until the layout
    // has settled the last scroll before the change is the one to keep.
    property real settledY: 0
    readonly property bool resizing: resizeTimer.running
    onWidthChanged: resizeTimer.restart()
    Timer { id: resizeTimer; interval: 200; onTriggered: root.settledY = flick.contentY }
    function restoreView(state) {
        viewWanted = state
        viewTimer.restart()
        Qt.callLater(applyView)
    }
    function applyView() {
        const state = viewWanted
        if (state === null) return
        const fields = { body: body.input, subject: subject.input, to: to.input, cc: cc.input, bcc: bcc.input }
        if (state.focus !== "" && !fields[state.focus].activeFocus) {
            fields[state.focus].forceActiveFocus()
            fields[state.focus].cursorPosition = state.cursor
        }
        flick.contentY = Math.min(state.contentY, Math.max(0, flick.contentHeight - flick.height))
    }
    Timer { id: viewTimer; interval: 300; onTriggered: root.viewWanted = null }
    // Any edit: the core keeps the draft.
    function edited() {
        if (filling || !filled) return
        error = ""
        core.draftChanged(compose, draftJson())
    }
    // Files were handed to the core to attach: each answers with Attached
    // or AttachFailed. (The shell counts them with the shared files'.)
    // `transfer` is the picker's Content Hub transfer, if any: finalized once
    // its files settled.
    signal attachStarted(int count, var transfer)
    function pick(paths, transfer) {
        attachStarted(paths.length, transfer || null)
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
        // In the pane, Close asks first: the page is not a whole screen the
        // user can back out of, and the draft may be worth keeping or not.
        onBackClicked: if (root.wide) more.openClosing(); else root.leave()
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
        objectName: "composeFlick"
        anchors { top: errorBanner.bottom; bottom: parent.bottom; left: parent.left; right: parent.right }
        clip: true
        contentHeight: column.height
        boundsBehavior: Flickable.StopAtBounds
        onWidthChanged: resizeTimer.restart()
        onContentHeightChanged: if (root.viewWanted !== null) root.applyView()
        // Giving focus back scrolls the cursor line into view (KeyboardAvoider);
        // until the page is still, the saved scroll wins.
        onContentYChanged: {
                if (!root.resizing) root.settledY = contentY
            if (root.viewWanted !== null) Qt.callLater(root.applyView)
        }

        Platform.KeyboardAvoider { objectName: "keyboardAvoider"; flickable: flick }

        Column {
            id: column
            width: root.wide ? Math.min(flick.width - root.theme.gu(10), root.theme.gu(90)) : flick.width
            x: root.wide ? root.theme.gu(5) : 0
            FromRow {
                width: parent.width
                labelColumn: root.labelColumn
                theme: root.theme
                accounts: root.accounts
                account: root.account
                onChosen: function(key) { root.account = key; root.edited() }
            }
            RecipientField {
                id: to
                objectName: "to"
                width: parent.width
                labelColumn: root.labelColumn
                theme: root.theme
                core: root.core
                suggestions: root.suggestions
                label: qsTr("To")
                trailingText: root.showCc ? "" : qsTr("Cc Bcc")
                trailingObjectName: "showCc"
                onTrailingClicked: root.showCc = true
                onEdited: root.edited()
                onAsked: { cc.asking = false; bcc.asking = false }
            }
            RecipientField {
                id: cc
                objectName: "ccField"
                width: parent.width
                labelColumn: root.labelColumn
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
                labelColumn: root.labelColumn
                visible: root.showCc
                height: visible ? implicitHeight : 0
                theme: root.theme
                core: root.core
                suggestions: root.suggestions
                label: qsTr("Bcc")
                onEdited: root.edited()
                onAsked: { to.asking = false; cc.asking = false }
            }
            SubjectRow {
                id: subject
                objectName: "subjectField"
                width: parent.width
                labelColumn: root.labelColumn
                theme: root.theme
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
        objectName: "discardActions"
        theme: root.theme
        onSaved: root.leave()
        onDiscarded: {
            root.core.closeCompose(root.compose, true)
            root.closed()
        }
    }

    Content {
        id: content
        anchors.fill: parent
        theme: root.theme
        onFilesImported: function(paths, transfer) { root.pick(paths, transfer) }
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
