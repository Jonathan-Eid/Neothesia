//! Bridge to the Kotlin `MainActivity` shell (see `android/`) that wraps our
//! `NativeActivity`. Needed because `Intent`-based pickers (Storage Access
//! Framework) only deliver their result through `onActivityResult`, a Java
//! `Activity` callback with no pure-native equivalent.
use std::{path::PathBuf, sync::Mutex};

use futures_channel::oneshot;

static PENDING_PICK: Mutex<Option<oneshot::Sender<Option<PathBuf>>>> = Mutex::new(None);

/// Opens the system "open document" picker and resolves with a local,
/// readable path to the picked file once the user closes it (or `None` if
/// they cancelled, or something went wrong on the Kotlin side).
///
/// The returned path is a private cache copy of the picked content (SAF
/// `content://` URIs aren't `std::fs::File`-openable paths), so it's fine to
/// read but shouldn't be treated as a stable, user-meaningful location.
pub async fn pick_file() -> Option<PathBuf> {
    let (tx, rx) = oneshot::channel();
    *PENDING_PICK.lock().unwrap() = Some(tx);

    if let Err(err) = call_open_file_picker() {
        log::error!("Failed to launch Android file picker: {err}");
        PENDING_PICK.lock().unwrap().take();
        return None;
    }

    rx.await.ok().flatten()
}

fn call_open_file_picker() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
    env.call_method(&activity, "openFilePicker", "()V", &[])?;
    Ok(())
}

/// Called from Kotlin (`MainActivity.nativeOnFilePicked`) once the picker
/// activity returns. `path` is `None` when the user cancelled, or the
/// Kotlin-side copy-to-cache step failed.
fn on_file_picked(path: Option<PathBuf>) {
    if let Some(tx) = PENDING_PICK.lock().unwrap().take() {
        let _ = tx.send(path);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_github_polymeilex_neothesia_MainActivity_nativeOnFilePicked(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    path: jni::objects::JString,
) {
    let path = if path.is_null() {
        None
    } else {
        env.get_string(&path)
            .ok()
            .map(|s| PathBuf::from(String::from(s)))
    };
    on_file_picked(path);
}
