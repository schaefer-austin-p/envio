.pragma library

// The path of a transferred file from its `file://` URL (Content Hub hands
// over URLs). Pure, so it is tested without Lomiri.
//
// Each run of valid "%XX" escapes is decoded on its own, so a bare "%"
// ("100%.pdf"), which makes decodeURIComponent throw on the whole string,
// stays as it came and the rest of the name is still decoded. A run that is
// not valid UTF-8 is kept as it came too.
function pathOfUrl(url) {
    const raw = String(url).replace(/^file:\/\//, "")
    return raw.replace(/(?:%[0-9A-Fa-f]{2})+/g, function (run) {
        try {
            return decodeURIComponent(run)
        } catch (e) {
            return run
        }
    })
}

// The paths of `urls`, in order.
function pathsOfUrls(urls) {
    const paths = []
    for (let i = 0; i < urls.length; i++)
        paths.push(pathOfUrl(urls[i]))
    return paths
}
