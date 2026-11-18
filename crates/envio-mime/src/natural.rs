//! Capping an image with no width by its natural size. Qt's rich text has
//! no `max-width`, so an image without a width draws at the file's size; a
//! known natural size (a cid image's from its bytes, a remote one's as the
//! reader reports it) lets the sanitiser give it the room's width instead.

/// A remote image written with no width: once the reader knows its
/// natural size, a render with that size caps it when [`Unsized::caps`]
/// says so. The caller uses this to skip renders that would change nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsized {
    /// As in [`crate::Rendered::remote_images`].
    pub url: String,
    /// The width it may take: the reader's, or its cell's when narrower.
    pub room: u32,
    /// The height it is drawn at, when set.
    pub height: Option<u32>,
}

impl Unsized {
    /// Whether an image of natural `size` (width, height) would draw wider
    /// than its room.
    pub fn caps(&self, size: (u32, u32)) -> bool {
        draws_wider(size, self.height, self.room)
    }
}

/// Whether an image of natural `(nw, nh)`, drawn at `height` when set, is
/// wider than `room`. With a height but no width Qt scales the width
/// proportionally, so that is the width compared; with no natural height
/// to scale by, nothing is. A `room` of 0 means no limit.
pub(crate) fn draws_wider((nw, nh): (u32, u32), height: Option<u32>, room: u32) -> bool {
    if room == 0 {
        return false;
    }
    let drawn = match height {
        None => u64::from(nw),
        Some(h) if nh > 0 => u64::from(nw) * u64::from(h) / u64::from(nh),
        Some(_) => 0,
    };
    drawn > u64::from(room)
}
