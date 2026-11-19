# Third-party notices

Envio itself is under the GNU General Public License, version 3 (see
`LICENSE`).

## bindle: shared interface components (LGPL-3.0-only)

Envio uses bindle, a shared QML library for Ubuntu Touch apps
(https://code.austinschaefer.com/schaefera/bindle), vendored as the git
submodule `qml/bindle` and compiled into the app through
`qml/bindle/bindle.qrc`. bindle is under the GNU Lesser General Public
License, version 3 only (`SPDX-License-Identifier: LGPL-3.0-only`); its source
is that submodule, at the commit the repository pins. The licence texts ship
in the click under `licenses/bindle/` (`COPYING.LESSER` is the LGPL-3.0,
`COPYING` the GPL-3.0 it refers to).

## psl: Public Suffix List data (MIT OR Apache-2.0, list under MPL-2.0)

Envio uses the `psl` crate (https://crates.io/crates/psl, with `psl-types`) to
find a sender's organisational domain when it checks that a signature's
domain aligns with the From domain. The crate is under the MIT or Apache-2.0
licence; the list it embeds is Mozilla's Public Suffix List
(https://publicsuffix.org/), which is under the Mozilla Public License 2.0.

## Lucide: icons (ISC, with portions MIT)

Most icons in `qml/components/iconShapes.js` are Lucide icons
(https://lucide.dev, lucide-static 0.460.0), each shape converted to path
data. `NOTICES.md` ships in the click (see `CMakeLists.txt`), so these notices
travel with the app.

Lucide's `LICENSE` as published in lucide-static 0.460.0:

    ISC License

    Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as part of Feather (MIT). All other copyright (c) for Lucide are held by Lucide Contributors 2022.

    Permission to use, copy, modify, and/or distribute this software for any
    purpose with or without fee is hereby granted, provided that the above
    copyright notice and this permission notice appear in all copies.

    THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
    WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
    MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
    ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
    WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
    ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
    OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

That file names portions that Lucide took from Feather
(https://github.com/feathericons/feather), which is under the MIT licence.
Feather's `LICENSE` as published (v4.29.2):

    The MIT License (MIT)

    Copyright (c) 2013-2023 Cole Bemis

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the "Software"), to deal
    in the Software without restriction, including without limitation the rights
    to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software, and to permit persons to whom the Software is
    furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in all
    copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
    SOFTWARE.

## Fonts

Inter, IBM Plex Mono and Lora are under the SIL Open Font License 1.1; see
`third_party/fonts/README.md` and each folder's `OFL.txt`.
