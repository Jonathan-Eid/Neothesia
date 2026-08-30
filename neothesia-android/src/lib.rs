//! Thin JNI/NativeActivity entry shell - see the crate's doc comment in
//! Cargo.toml for why this is a separate crate from `neothesia` itself.
#![cfg(target_os = "android")]

#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    neothesia::main_android(app);
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
            .map(|s| std::path::PathBuf::from(String::from(s)))
    };
    neothesia::android::on_file_picked(path);
}
