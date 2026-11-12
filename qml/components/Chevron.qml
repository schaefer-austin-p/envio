import QtQuick 2.15
import "../platform"

// A disclosure chevron: decoration only (no touch handling), the row it sits
// in takes the tap. `tone` picks the Theme role: "red" and "blue" are the
// Airmail stamp colours (both read as secondary in Carbon), "secondary" the
// plain text-secondary grey. `open` turns it down, for a group that is open.
Icon {
    id: root
    objectName: "chevron"
    property string tone: "red"
    property bool open: false

    name: open ? "down" : "forward"
    size: theme.iconSmall
    color: tone === "blue" ? theme.chevronBlue
         : tone === "secondary" ? theme.secondary
         : theme.chevron
}
