#[cfg(not(target_os = "android"))]
fn main() {
    neothesia::main_desktop();
}

// The android_activity/NativeActivity glue loads `neothesia_main` from the
// `cdylib` produced by the `neothesia` lib target directly; this `bin` target
// isn't used on Android, but still needs to compile.
#[cfg(target_os = "android")]
fn main() {}
