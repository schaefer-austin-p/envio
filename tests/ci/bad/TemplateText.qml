// expect: Text without textFormat
import QtQuick 2.15

// Fixture: a backtick in a Text does not hide the missing textFormat that follows; must be rejected.
Text {
    property string t: `plain`
    text: t
}
