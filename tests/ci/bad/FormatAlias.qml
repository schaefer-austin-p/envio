// expect: textFormat must be a literal
import QtQuick 2.15

// Fixture: FormatAlias; must be rejected.
Text {
    text: "x"
    property int fmt: Text.PlainText
    textFormat: fmt
}
