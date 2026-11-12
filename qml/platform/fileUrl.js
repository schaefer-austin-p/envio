.pragma library

// The path of a transferred file from its `file://` URL (Content Hub hands
// over URLs). Pure, so it is tested without Lomiri.
//
// Each run of valid "%XX" escapes is decoded on its own, so a bare "%"
// ("100%.pdf"), which makes decodeURIComponent throw on the whole string,
// stays as it came and the rest of the name is still decoded. In a run that
// is not valid UTF-8 ("%FF%41"), each character that decodes (one to four
// escapes) is decoded and each byte that does not is kept as it came.
function pathOfUrl(url) {
    const raw = String(url).replace(/^file:\/\//, "")
    return raw.replace(/(?:%[0-9A-Fa-f]{2})+/g, function (run) {
        try {
            return decodeURIComponent(run)
        } catch (e) {
            return decodeEach(run)
        }
    })
}

// `run` is "%XX" escapes; a UTF-8 character takes one to four of them.
function decodeEach(run) {
    let out = ""
    let i = 0
    while (i < run.length) {
        let n = Math.min(4, (run.length - i) / 3)
        for (; n > 0; n--) {
            try {
                out += decodeURIComponent(run.substr(i, 3 * n))
                break
            } catch (e) {}
        }
        if (n === 0) {
            out += run.substr(i, 3)
            n = 1
        }
        i += 3 * n
    }
    return out
}

// The paths of `urls`, in order.
function pathsOfUrls(urls) {
    const paths = []
    for (let i = 0; i < urls.length; i++)
        paths.push(pathOfUrl(urls[i]))
    return paths
}
