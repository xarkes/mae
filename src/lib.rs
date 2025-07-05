use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[unsafe(no_mangle)]
pub extern "C" fn rust_greeting(to: *const c_char) -> *mut c_char {
    let c_str = unsafe { CStr::from_ptr(to) };
    let recipient = match c_str.to_str() {
        Err(_) => "there",
        Ok(string) => string,
    };

    CString::new("Hello ".to_owned() + recipient)
        .unwrap()
        .into_raw()
}

#[cfg(target_os = "android")]
#[allow(non_snake_case)]
pub mod android {
    extern crate jni;

    use self::jni::JNIEnv;
    use self::jni::objects::{JClass, JString};
    use self::jni::sys::jstring;
    use super::*;

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_xark_es_mae_MainActivity_hello<'local>(
        mut env: JNIEnv<'local>,
        class: JClass<'local>,
        input: JString<'local>,
    ) -> jstring {
        let input: String = env
            .get_string(&input)
            .expect("Couldn't get java string!")
            .into();

        let output = env
            .new_string(format!("Hello, {}!", input))
            .expect("Couldn't create java string!");

        output.into_raw()
    }
}
