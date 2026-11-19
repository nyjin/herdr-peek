//! The clipboard: a PNG or text put on it (screenshots, selections), and the watcher's view of it (select to open).
use objc2::rc::autoreleasepool;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString};
use objc2_foundation::{NSData, NSString};
use std::io;
use std::path::Path;

/// The general pasteboard for `watcher::run`. Each look runs in its own autorelease pool: the watcher runs for
/// hours without an event loop to drain one.
pub struct Pasteboard;

impl crate::watcher::Clipboard for Pasteboard {
    fn change_count(&self) -> isize {
        autoreleasepool(|_| NSPasteboard::generalPasteboard().changeCount())
    }

    fn text(&self) -> Option<String> {
        autoreleasepool(|_| {
            let text = NSPasteboard::generalPasteboard().stringForType(unsafe { NSPasteboardTypeString })?;
            Some(text.to_string())
        })
    }

    fn from_peek(&self) -> bool {
        autoreleasepool(|_| {
            NSPasteboard::generalPasteboard()
                .types()
                .is_some_and(|types| types.iter().any(|t| t.to_string() == crate::watcher::PEEK_COPY_TYPE))
        })
    }
}

/// Put `text` on the clipboard as plain text, with peek's private mark next to it so the watcher does not take the
/// copy for a path to open.
pub fn copy_text(text: &str) -> io::Result<()> {
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    let mark = NSString::from_str(crate::watcher::PEEK_COPY_TYPE);
    let ok = board.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString })
        && board.setString_forType(&NSString::from_str("1"), &mark);
    if ok {
        Ok(())
    } else {
        Err(io::Error::other("the clipboard refused the text"))
    }
}

pub fn copy_png(path: &Path) -> io::Result<()> {
    let bytes = std::fs::read(path)?;
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    if board.setData_forType(Some(&NSData::with_bytes(&bytes)), unsafe { NSPasteboardTypePNG }) {
        Ok(())
    } else {
        Err(io::Error::other("the clipboard refused the image"))
    }
}
