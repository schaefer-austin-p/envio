import QtQuick 2.15
import "../platform"

// A disclosure chevron in the secondary text colour (both themes): decoration
// only, with no touch handling, so the row it sits in takes the tap.
// `direction` is "forward" (opens a page, or a closed group) or "down" (opens
// a picker, or an open group).
Icon {
    id: root
    objectName: "chevron"
    property string direction: "forward"

    name: direction
    size: theme.iconSmall
    color: theme.secondary
}
