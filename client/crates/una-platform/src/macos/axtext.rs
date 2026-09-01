//! Reading and writing the focused text field through the Accessibility API.
//!
//! This is how una notices that the text it pasted has been edited without
//! showing any UI: snapshot the focused element's value right after the
//! paste, read it again once the user stops typing, and diff the two.
//!
//! It only works where the focused control publishes an `AXValue` string.
//! Native AppKit fields, Safari, Mail, Notes and most Electron apps do;
//! GPU-rendered terminals (Ghostty, Alacritty, kitty), canvas editors and
//! password fields (which deliberately withhold their contents) do not. Every
//! entry point therefore returns an `Option`, and the caller falls back to
//! the correction window when the answer is `None`.
//!
//! Uses the AX C API directly rather than a binding crate: the app already
//! links `ApplicationServices` for `AXIsProcessTrusted`, and the surface used
//! here is five functions wide.
//!
//! **Threading**: AX calls are synchronous IPC to the focused application and
//! can block for as long as the messaging timeout, so they must never run on
//! the UI thread or inside the event-tap callback. Every element gets a short
//! [`MESSAGING_TIMEOUT`] so an unresponsive app cannot wedge the caller.

use std::ffi::c_void;

use core_foundation::base::{CFIndex, CFRange, CFType, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};

/// Cap on how long any single AX call may block, in seconds. A frozen app
/// costs us this much once, not a hung dictation thread.
const MESSAGING_TIMEOUT: f32 = 0.5;

/// `kAXValueCFRangeType`, for packing/unpacking `AXValue`-wrapped ranges.
const AX_VALUE_CF_RANGE_TYPE: u32 = 4;
const AX_ERROR_SUCCESS: i32 = 0;

type AXUIElementRef = CFTypeRef;
type AXError = i32;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout: f32) -> AXError;
    fn AXValueGetValue(value: CFTypeRef, value_type: u32, out: *mut c_void) -> bool;
    fn AXValueCreate(value_type: u32, value: *const c_void) -> CFTypeRef;
}

extern "C" {
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> bool;
}

/// An owned `AXUIElementRef`.
///
/// `Send` because AX element references may be messaged from any thread; the
/// module contract is that they are messaged from a worker, never the main
/// thread. They are not `Sync` — a single element is used from one thread at
/// a time.
struct AxRef(AXUIElementRef);

impl AxRef {
    /// Takes ownership of a +1 reference (the `Copy`/`Create` rule).
    unsafe fn from_create(raw: CFTypeRef) -> Option<Self> {
        if raw.is_null() {
            None
        } else {
            Some(Self(raw))
        }
    }

    fn attribute(&self, name: &str) -> Option<CFType> {
        let attr = CFString::new(name);
        let mut out: CFTypeRef = std::ptr::null();
        let err = unsafe {
            AXUIElementCopyAttributeValue(self.0, attr.as_concrete_TypeRef(), &mut out)
        };
        if err != AX_ERROR_SUCCESS || out.is_null() {
            return None;
        }
        Some(unsafe { CFType::wrap_under_create_rule(out) })
    }

    fn set_attribute(&self, name: &str, value: CFTypeRef) -> bool {
        let attr = CFString::new(name);
        let err =
            unsafe { AXUIElementSetAttributeValue(self.0, attr.as_concrete_TypeRef(), value) };
        err == AX_ERROR_SUCCESS
    }

    fn string_attribute(&self, name: &str) -> Option<String> {
        let value = self.attribute(name)?;
        if !value.instance_of::<CFString>() {
            return None;
        }
        let s = unsafe { CFString::wrap_under_get_rule(value.as_CFTypeRef() as CFStringRef) };
        Some(s.to_string())
    }

    fn range_attribute(&self, name: &str) -> Option<CFRange> {
        let value = self.attribute(name)?;
        let mut range = CFRange {
            location: 0 as CFIndex,
            length: 0 as CFIndex,
        };
        let ok = unsafe {
            AXValueGetValue(
                value.as_CFTypeRef(),
                AX_VALUE_CF_RANGE_TYPE,
                &mut range as *mut CFRange as *mut c_void,
            )
        };
        ok.then_some(range)
    }
}

impl Drop for AxRef {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

unsafe impl Send for AxRef {}

/// A snapshot of the focused text field, taken right after a paste.
///
/// Holds a reference to the element so a later read can confirm the user is
/// still in the *same* field — focus moving elsewhere invalidates the whole
/// correction attempt.
pub struct FocusedField {
    element: AxRef,
    /// Full text of the field at snapshot time.
    pub value: String,
    /// Caret position at snapshot time, as a `char` offset into `value`.
    pub caret: usize,
}

impl FocusedField {
    /// Snapshot the currently focused text field, or `None` when there isn't
    /// one that publishes readable text.
    pub fn capture() -> Option<Self> {
        let element = focused_element()?;
        let value = element.string_attribute("AXValue")?;
        let caret = element
            .range_attribute("AXSelectedTextRange")
            .map(|r| utf16_to_char_offset(&value, r.location.max(0) as usize))
            .unwrap_or_else(|| value.chars().count());
        Some(Self {
            element,
            value,
            caret,
        })
    }

    /// Re-read the field's text, but only if focus never left it.
    pub fn reread(&self) -> Option<String> {
        let current = focused_element()?;
        if !unsafe { CFEqual(current.0, self.element.0) } {
            return None;
        }
        self.element.string_attribute("AXValue")
    }

    /// Replace `chars` characters starting at char offset `start` with
    /// `text`, leaving the caret after the inserted text.
    ///
    /// Returns false when the field refused the write (many AX elements are
    /// read-only), in which case the caller must not assume the field
    /// changed.
    pub fn replace_range(&self, start: usize, chars: usize, text: &str) -> bool {
        let Some(value) = self.element.string_attribute("AXValue") else {
            return false;
        };
        let location = char_to_utf16_offset(&value, start);
        let length = utf16_len(&value, start, chars);
        let range = CFRange {
            location: location as CFIndex,
            length: length as CFIndex,
        };
        let ax_range = unsafe {
            AXValueCreate(
                AX_VALUE_CF_RANGE_TYPE,
                &range as *const CFRange as *const c_void,
            )
        };
        if ax_range.is_null() {
            return false;
        }
        let ax_range = unsafe { CFType::wrap_under_create_rule(ax_range) };
        if !self
            .element
            .set_attribute("AXSelectedTextRange", ax_range.as_CFTypeRef())
        {
            return false;
        }
        let replacement = CFString::new(text);
        self.element
            .set_attribute("AXSelectedText", replacement.as_CFTypeRef())
    }
}

fn focused_element() -> Option<AxRef> {
    let system = unsafe { AxRef::from_create(AXUIElementCreateSystemWide())? };
    unsafe { AXUIElementSetMessagingTimeout(system.0, MESSAGING_TIMEOUT) };
    let focused = system.attribute("AXFocusedUIElement")?;
    let raw = unsafe { CFRetain(focused.as_CFTypeRef()) };
    let element = unsafe { AxRef::from_create(raw)? };
    unsafe { AXUIElementSetMessagingTimeout(element.0, MESSAGING_TIMEOUT) };
    Some(element)
}

/// Whether the focused control exposes editable text at all — the test for
/// "can we watch this silently, or do we need the correction window?".
pub fn focused_field_is_readable() -> bool {
    focused_element()
        .and_then(|el| el.string_attribute("AXValue"))
        .is_some()
}

// ---------------------------------------------------------------------------
// UTF-16 <-> char offset conversion
//
// AX speaks UTF-16 code units (NSString's native unit); the correction diff
// speaks `char`s. Emoji and other astral-plane characters make the two
// disagree, and dictated text does contain them.
// ---------------------------------------------------------------------------

fn utf16_to_char_offset(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0usize;
    for (chars, ch) in text.chars().enumerate() {
        if units >= utf16_offset {
            return chars;
        }
        units += ch.len_utf16();
    }
    text.chars().count()
}

fn char_to_utf16_offset(text: &str, char_offset: usize) -> usize {
    text.chars().take(char_offset).map(char::len_utf16).sum()
}

/// UTF-16 length of `chars` characters starting at char offset `start`.
fn utf16_len(text: &str, start: usize, chars: usize) -> usize {
    text.chars()
        .skip(start)
        .take(chars)
        .map(char::len_utf16)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_round_trip_for_ascii() {
        let text = "hello world";
        assert_eq!(utf16_to_char_offset(text, 5), 5);
        assert_eq!(char_to_utf16_offset(text, 5), 5);
        assert_eq!(utf16_len(text, 0, 5), 5);
    }

    /// An emoji is one `char` but two UTF-16 units; a caret reported after it
    /// must not land mid-character.
    #[test]
    fn offsets_account_for_surrogate_pairs() {
        let text = "hi 🎉 there";
        assert_eq!(char_to_utf16_offset(text, 4), 5);
        assert_eq!(utf16_to_char_offset(text, 5), 4);
        assert_eq!(utf16_len(text, 3, 1), 2);
    }

    #[test]
    fn offsets_clamp_past_the_end() {
        assert_eq!(utf16_to_char_offset("abc", 99), 3);
        assert_eq!(char_to_utf16_offset("abc", 99), 3);
        assert_eq!(utf16_len("abc", 1, 99), 2);
    }
}
