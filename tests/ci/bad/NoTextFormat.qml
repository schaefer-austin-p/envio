import QtQuick 2.15

// Fixture: a Text without textFormat is AutoText, which renders markup in
// text from a sender (and loads its images); must be rejected.
Item {
    Text {
        text: "from a sender"
        // The nested one states it, which does not help the outer one.
        Text { textFormat: Text.PlainText; text: "inner" }
    }
}
