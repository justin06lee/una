//! macOS implementation: NSPasteboard clipboard save/restore, CGEvent Cmd+V
//! paste chord, NSWorkspace frontmost app, TCC permission checks, and the
//! CGEventTap hotkey backend (see [`eventtap`]).

pub mod axtext;
pub mod eventtap;
pub mod keys;

use std::ffi::c_void;
use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting, NSWorkspace,
};
use objc2_foundation::{NSArray, NSData, NSString};

use crate::{
    FrontmostApp, InjectError, InjectOptions, InjectOutcome, InjectProbe, PermissionState,
    Permissions, TextInjector,
};

/// NSPasteboard concealed/transient marker so clipboard managers skip the
/// dictation text. See <http://nspasteboard.org>.
const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";
/// Don't save/restore pasteboards holding more than this much data.
const MAX_SAVED_PASTEBOARD_BYTES: usize = 10 * 1024 * 1024;
/// ANSI keycode for 'v', used when layout resolution fails.
const FALLBACK_V_KEYCODE: u16 = 9;

// ---------------------------------------------------------------------------
// C externs (Carbon / ApplicationServices)
// ---------------------------------------------------------------------------

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn IsSecureEventInputEnabled() -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: *const c_void;
}

/// Resolve the virtual keycode producing 'v' on the current keyboard layout.
fn keycode_for_v() -> u16 {
    keys::keycode_for_char('v').unwrap_or(FALLBACK_V_KEYCODE)
}

/// NSStatusWindowLevel: above the Dock (level 20), below screen savers. The
/// pill HUD sits at the very bottom edge of the screen, so without this the
/// Dock would cover it.
const STATUS_WINDOW_LEVEL: isize = 25;

/// Raise a window above the Dock so the bottom-edge pill stays visible.
///
/// # Safety
/// `ns_window` must be a valid pointer to an `NSWindow` (e.g. from tauri's
/// `WebviewWindow::ns_window`), and must be called on the main thread.
pub unsafe fn raise_window_above_dock(ns_window: *mut c_void) {
    use objc2::runtime::AnyObject;
    let window = ns_window as *mut AnyObject;
    if window.is_null() {
        return;
    }
    let _: () = objc2::msg_send![&*window, setLevel: STATUS_WINDOW_LEVEL];
}

fn post_cmd_v() -> Result<(), InjectError> {
    let keycode = keycode_for_v();
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| InjectError::Keystroke("could not create CGEventSource".into()))?;
    let down = CGEvent::new_keyboard_event(source.clone(), keycode, true)
        .map_err(|_| InjectError::Keystroke("could not create key-down event".into()))?;
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    let up = CGEvent::new_keyboard_event(source, keycode, false)
        .map_err(|_| InjectError::Keystroke("could not create key-up event".into()))?;
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    down.post(CGEventTapLocation::HID);
    std::thread::sleep(Duration::from_millis(10));
    up.post(CGEventTapLocation::HID);
    Ok(())
}

// ---------------------------------------------------------------------------
// Pasteboard save / write / restore
// ---------------------------------------------------------------------------

type SavedPasteboard = Vec<Vec<(String, Vec<u8>)>>;

fn save_pasteboard(pb: &NSPasteboard) -> Option<SavedPasteboard> {
    let items = pb.pasteboardItems()?;
    let mut saved: SavedPasteboard = Vec::new();
    let mut total = 0usize;
    for item in items.iter() {
        let mut saved_item = Vec::new();
        let types = item.types();
        for ty in types.iter() {
            if let Some(data) = item.dataForType(&ty) {
                let bytes = data.to_vec();
                total += bytes.len();
                if total > MAX_SAVED_PASTEBOARD_BYTES {
                    return None; // too big: skip save/restore entirely
                }
                saved_item.push((ty.to_string(), bytes));
            }
        }
        saved.push(saved_item);
    }
    Some(saved)
}

fn write_text(pb: &NSPasteboard, text: &str) -> Result<(), InjectError> {
    pb.clearContents();
    let ns_text = NSString::from_str(text);
    let pb_type_string = unsafe { NSPasteboardTypeString };
    if !pb.setString_forType(&ns_text, pb_type_string) {
        return Err(InjectError::Clipboard(
            "could not write text to pasteboard".into(),
        ));
    }
    // Mark as transient so clipboard managers ignore it.
    let transient = NSString::from_str(TRANSIENT_TYPE);
    let empty = NSData::new();
    pb.setData_forType(Some(&empty), &transient);
    Ok(())
}

fn restore_pasteboard(pb: &NSPasteboard, saved: SavedPasteboard) {
    pb.clearContents();
    let mut objects: Vec<objc2::rc::Retained<ProtocolObject<dyn NSPasteboardWriting>>> = Vec::new();
    for saved_item in saved {
        if saved_item.is_empty() {
            continue;
        }
        let item = NSPasteboardItem::new();
        for (ty, bytes) in saved_item {
            let data = NSData::with_bytes(&bytes);
            let ty = NSString::from_str(&ty);
            item.setData_forType(&data, &ty);
        }
        objects.push(ProtocolObject::from_retained(item));
    }
    if !objects.is_empty() {
        let array = NSArray::from_retained_slice(&objects);
        pb.writeObjects(&array);
    }
}

// ---------------------------------------------------------------------------
// Trait impls
// ---------------------------------------------------------------------------

pub struct MacInjector;

impl MacInjector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl TextInjector for MacInjector {
    fn probe(&self) -> InjectProbe {
        let trusted = unsafe { AXIsProcessTrusted() };
        let secure = unsafe { IsSecureEventInputEnabled() };
        let can_paste = trusted && !secure;
        let detail = if !trusted {
            "Accessibility permission is required to send the paste keystroke. \
             Grant it in System Settings > Privacy & Security > Accessibility."
                .to_string()
        } else if secure {
            "Secure input is active (a password field is focused); text will be \
             left on the clipboard instead of pasted."
                .to_string()
        } else {
            "Ready: clipboard + Cmd+V via CGEvent.".to_string()
        };
        InjectProbe {
            backend: "cgevent".into(),
            can_paste,
            detail,
        }
    }

    fn inject(&self, text: &str, opts: &InjectOptions) -> Result<InjectOutcome, InjectError> {
        let pb = NSPasteboard::generalPasteboard();

        // If a password field has grabbed secure input, synthetic Cmd+V will
        // not be delivered: leave the text on the clipboard and report it.
        if unsafe { IsSecureEventInputEnabled() } {
            write_text(&pb, text)?;
            return Ok(InjectOutcome::ClipboardOnly);
        }

        let saved = if opts.restore_clipboard {
            save_pasteboard(&pb)
        } else {
            None
        };

        write_text(&pb, text)?;
        // Give the pasteboard change a moment to propagate before pasting.
        std::thread::sleep(Duration::from_millis(50));
        let paste_result = post_cmd_v();

        match paste_result {
            Ok(()) => {
                if let Some(saved) = saved {
                    std::thread::sleep(opts.restore_delay);
                    restore_pasteboard(&pb, saved);
                }
                Ok(InjectOutcome::Injected)
            }
            Err(e) => {
                // Leave the dictated text on the clipboard so it isn't lost.
                Err(e)
            }
        }
    }
}

pub struct MacFrontmost;

impl FrontmostApp for MacFrontmost {
    fn current(&self) -> Option<String> {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        let name = app.localizedName()?;
        Some(name.to_string())
    }
}

pub struct MacPermissions;

impl Permissions for MacPermissions {
    fn mic(&self) -> PermissionState {
        use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
        let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
            return PermissionState::Unknown;
        };
        let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
        match status {
            AVAuthorizationStatus::Authorized => PermissionState::Granted,
            AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
                PermissionState::Denied
            }
            AVAuthorizationStatus::NotDetermined => PermissionState::Undetermined,
            _ => PermissionState::Unknown,
        }
    }

    fn request_mic(&self) {
        use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
        let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
            return;
        };
        let handler = block2::RcBlock::new(|granted: objc2::runtime::Bool| {
            tracing_granted(granted.as_bool());
        });
        unsafe {
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);
        }
    }

    fn accessibility(&self) -> PermissionState {
        if unsafe { AXIsProcessTrusted() } {
            PermissionState::Granted
        } else {
            PermissionState::Denied
        }
    }

    fn prompt_accessibility(&self) {
        use core_foundation::base::TCFType;
        use core_foundation::boolean::CFBoolean;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;
        unsafe {
            let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt as _);
            let options = CFDictionary::from_CFType_pairs(&[(
                key.as_CFType(),
                CFBoolean::true_value().as_CFType(),
            )]);
            AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const c_void);
        }
    }
}

fn tracing_granted(granted: bool) {
    if granted {
        eprintln!("una: microphone access granted");
    } else {
        eprintln!("una: microphone access denied");
    }
}

/// Whether secure event input is currently enabled (password field focused).
pub fn secure_input_active() -> bool {
    unsafe { IsSecureEventInputEnabled() }
}
