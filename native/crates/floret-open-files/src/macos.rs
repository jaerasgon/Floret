//! The macOS "open documents" Apple Event (`kAEOpenDocuments`).
//!
//! `NSApplication` installs its own handler for it while it finishes
//! launching, and the event that launched the app arrives right after
//! `NSApplicationWillFinishLaunchingNotification`. So [`install`] (called
//! before winit creates and runs `NSApp`) only subscribes to that
//! notification; when it fires, our handler replaces AppKit's (which would
//! show "Floret cannot open files of this type", winit's delegate having no
//! `application:openURLs:`). From then on every open request, at launch or
//! later, lands in [`crate::push`].

use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter, NSObject,
    NSObjectProtocol, NSString,
};
use std::path::PathBuf;

/// Four-character codes from `AE/AppleEvents.h` and `AEDataModel.h`.
const fn four_cc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}
/// `kCoreEventClass`
const CORE_EVENT_CLASS: u32 = four_cc(b"aevt");
/// `kAEOpenDocuments`
const OPEN_DOCUMENTS: u32 = four_cc(b"odoc");
/// `keyDirectObject`: the event's list of files.
const DIRECT_OBJECT: u32 = four_cc(b"----");

/// `NSApplicationWillFinishLaunchingNotification` (AppKit's constant is
/// this string; naming it here avoids depending on objc2-app-kit).
const WILL_FINISH_LAUNCHING: &str = "NSApplicationWillFinishLaunchingNotification";

define_class!(
    // SAFETY: NSObject has no subclassing requirements and this class
    // doesn't implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "FloretOpenDocumentsHandler"]
    struct Handler;

    impl Handler {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Retained<Self> {
            let this = this.set_ivars(());
            unsafe { msg_send![super(this), init] }
        }

        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _note: &NSNotification) {
            guard("install the open-documents handler", || {
                let manager = NSAppleEventManager::sharedAppleEventManager();
                let handler: &AnyObject = self.as_ref();
                // `setEventHandler:andSelector:forEventClass:andEventID:`
                // (objc2-foundation only binds it with the CoreServices
                // types; AEEventClass and AEEventID are FourCharCode, u32).
                // SAFETY: `self` lives for the rest of the process (see
                // `install`) and implements the selector with the
                // signature AppKit calls it with.
                unsafe {
                    let _: () = msg_send![
                        &*manager,
                        setEventHandler: handler,
                        andSelector: sel!(handleOpenDocuments:withReplyEvent:),
                        forEventClass: CORE_EVENT_CLASS,
                        andEventID: OPEN_DOCUMENTS,
                    ];
                }
            });
        }

        #[unsafe(method(handleOpenDocuments:withReplyEvent:))]
        fn handle_open_documents(
            &self,
            event: &NSAppleEventDescriptor,
            _reply: &NSAppleEventDescriptor,
        ) {
            guard("read an open-documents event", || {
                let paths = paths_in(event);
                if paths.is_empty() {
                    eprintln!("floret: an open-documents event named no files");
                }
                crate::push(paths);
            });
        }
    }

    unsafe impl NSObjectProtocol for Handler {}
);

/// Subscribe to the will-finish-launching notification (see the module
/// docs). Logs and gives up off the main thread.
pub fn install() {
    guard("listen for open-documents events", || {
        let Some(mtm) = MainThreadMarker::new() else {
            eprintln!("floret: open-documents hook not installed (not on the main thread)");
            return;
        };
        // SAFETY: `init` is the designated initialiser defined above.
        let handler: Retained<Handler> = unsafe { msg_send![Handler::alloc(mtm), init] };
        let name = NSString::from_str(WILL_FINISH_LAUNCHING);
        // SAFETY: `handler` implements `willFinishLaunching:` taking an
        // NSNotification, and is never released (below), so the centre's
        // unretained reference to it stays valid.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                handler.as_ref(),
                sel!(willFinishLaunching:),
                Some(&name),
                None,
            );
        }
        // Neither the notification centre nor the Apple Event manager
        // retains its observer / handler: keep this one for good.
        std::mem::forget(handler);
    });
}

/// The files in an open-documents event: its direct object, a list of
/// file references (or, defensively, a single one).
fn paths_in(event: &NSAppleEventDescriptor) -> Vec<PathBuf> {
    // SAFETY: `paramDescriptorForKeyword:` takes an AEKeyword (u32) and
    // returns an autoreleased descriptor or nil.
    let list: Option<Retained<NSAppleEventDescriptor>> =
        unsafe { msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT] };
    let Some(list) = list else {
        return Vec::new();
    };
    let n = list.numberOfItems();
    if n <= 0 {
        return file_path(&list).into_iter().collect();
    }
    // Apple Event lists count from 1.
    (1..=n)
        .filter_map(|i| list.descriptorAtIndex(i))
        .filter_map(|d| file_path(&d))
        .collect()
}

/// A descriptor coerced to a file URL, as a path.
fn file_path(d: &NSAppleEventDescriptor) -> Option<PathBuf> {
    d.fileURLValue()?.to_file_path()
}

/// Run `f`, logging instead of unwinding into AppKit if it panics.
fn guard(what: &str, f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        eprintln!("floret: couldn't {what}; Finder can't open patches in Floret this session");
    }
}
