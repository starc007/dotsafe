//! Asks the person at the Mac to confirm, with Touch ID or their login
//! password, before an agent's request goes ahead.
//!
//! It runs in Rust, not the webview, so approving needs someone physically
//! there: a compromised webview can't approve a request by calling the
//! command. Blocks until answered, so call it off the UI thread. Other
//! platforms don't have this yet and always confirm.

/// Whether this Mac can ask (it has a login password, and Touch ID or a
/// paired Apple Watch if available).
#[cfg(target_os = "macos")]
pub fn available() -> bool {
    use objc2_local_authentication::{LAContext, LAPolicy};
    // SAFETY: LAContext is safe to create and query from any thread.
    unsafe { LAContext::new().canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthentication).is_ok() }
}

/// Shows the system prompt with `reason` (macOS phrases it as "dotsafe is
/// trying to <reason>") and waits for the answer. `Err` carries the system's
/// explanation, such as "Canceled by user."
#[cfg(target_os = "macos")]
pub fn confirm(reason: &str) -> Result<(), String> {
    use std::sync::mpsc;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};

    let policy = LAPolicy::DeviceOwnerAuthentication;
    // SAFETY: as above; the context lives until the reply arrives.
    let context = unsafe { LAContext::new() };
    if let Err(error) = unsafe { context.canEvaluatePolicy_error(policy) } {
        return Err(error.localizedDescription().to_string());
    }
    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    let reply = RcBlock::new(move |ok: Bool, error: *mut NSError| {
        let result = if ok.as_bool() {
            Ok(())
        } else {
            // SAFETY: LocalAuthentication passes a valid NSError (or null) for
            // the duration of the callback.
            let message = unsafe { error.as_ref() }.map(|e| e.localizedDescription().to_string());
            Err(message.unwrap_or_else(|| "Not confirmed.".into()))
        };
        let _ = tx.send(result);
    });
    // SAFETY: the reply block only captures a Sender, which is Send.
    unsafe { context.evaluatePolicy_localizedReason_reply(policy, &NSString::from_str(reason), &reply) };
    rx.recv().unwrap_or_else(|_| Err("Not confirmed.".into()))
}

#[cfg(not(target_os = "macos"))]
pub fn available() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn confirm(_reason: &str) -> Result<(), String> {
    Ok(())
}
