import QtQuick 2.15

// Fixture: must pass. A %n in a comment, a context argument, a plain
// string that mentions SwipeDelegate, and text built with .arg() from Rust.
/* qsTr("%n x", "", n) in a block comment is not code. */
Item {
    property int n: 2
    property string a: qsTr("Inbox")
    property string b: qsTr("Inbox", "folder name")
    property string c: qsTr("Total: %1").arg(n)
    property string d: "SwipeDelegate and %n are only words here"
    property string e: qsTr("Hello (world), \"quoted\", more")
}
