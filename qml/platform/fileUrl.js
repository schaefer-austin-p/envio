.pragma library

// The path of a transferred file from its `file://` URL (Content Hub hands
// over URLs). Pure, so it is tested without Lomiri.
//
// A name with a bare "%" ("100%.pdf") is not a valid escape and makes
// decodeURIComponent throw; it is kept as it came rather than dropping the
// whole import.
function pathOfUrl(url) {
    const raw = String(url).replace(/^file:\/\//, "")
    try {
        return decodeURIComponent(raw)
    } catch (e) {
        return raw
    }
}

// The paths of `urls`, in order.
function pathsOfUrls(urls) {
    const paths = []
    for (let i = 0; i < urls.length; i++)
        paths.push(pathOfUrl(urls[i]))
    return paths
}
