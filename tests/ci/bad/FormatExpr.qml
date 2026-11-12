// expect: textFormat must be a literal
import QtQuick 2.15

// Fixture: FormatExpr; must be rejected.
Text {
    text: "x"
    property bool plain: true
    textFormat: plain ? Text.PlainText : Text.AutoText
}
