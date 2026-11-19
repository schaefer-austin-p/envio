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

## Lucide: icons (ISC)

Most icons in `qml/components/iconShapes.js` are Lucide icons
(https://lucide.dev, lucide-static 0.460.0), each shape converted to path
data. `NOTICES.md` ships in the click (see `CMakeLists.txt`), so this notice
travels with the app.

> ISC License
>
> Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as
> part of Feather (MIT). All other copyright (c) for Lucide are held by Lucide
> Contributors 2022.
>
> Permission to use, copy, modify, and/or distribute this software for any
> purpose with or without fee is hereby granted, provided that the above
> copyright notice and this permission notice appear in all copies.
>
> THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
> REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
> AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
> INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
> LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
> OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
> PERFORMANCE OF THIS SOFTWARE.

## Fonts

Inter, IBM Plex Mono and Lora are under the SIL Open Font License 1.1; see
`third_party/fonts/README.md` and each folder's `OFL.txt`.
