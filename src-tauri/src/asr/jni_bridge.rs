//! Direct JNI bridge for the native Android floating bubble.

use crate::asr::AsrModelPaths;
use crate::commands::asr::{self, AsrCommandEvent};
use jni::objects::{GlobalRef, JFloatArray, JObject, JString, JValue};
use jni::sys::{jboolean, JNI_FALSE, JNI_TRUE};
use jni::{JNIEnv, JavaVM};
use once_cell::sync::Lazy;
use std::ffi::CString;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::Mutex;

static JNI_CALLBACK: Lazy<Mutex<Option<JniCallback>>> = Lazy::new(|| Mutex::new(None));

struct JniCallback {
    vm: JavaVM,
    listener: GlobalRef,
}

impl JniCallback {
    fn new(env: &mut JNIEnv<'_>, listener: JObject<'_>) -> anyhow::Result<Self> {
        Ok(Self {
            vm: env.get_java_vm()?,
            listener: env.new_global_ref(listener)?,
        })
    }

    fn call(&self, method: &str, text: &str) -> anyhow::Result<()> {
        let mut env = self.vm.attach_current_thread()?;
        let jtext = env.new_string(text)?;
        let jtext_obj: JObject<'_> = jtext.into();
        env.call_method(
            self.listener.as_obj(),
            method,
            "(Ljava/lang/String;)V",
            &[JValue::Object(&jtext_obj)],
        )?;
        Ok(())
    }
}

#[no_mangle]
pub extern "system" fn Java_com_galaxyruler_verbatim_FloatingBubbleService_nativeAsrStart(
    mut env: JNIEnv<'_>,
    _this: JObject<'_>,
    model_dir: JString<'_>,
    lang: JString<'_>,
    listener: JObject<'_>,
) -> jboolean {
    match native_asr_start(&mut env, model_dir, lang, listener) {
        Ok(()) => JNI_TRUE,
        Err(error) => {
            let message = format!("ASR start failed: {error}");
            log_native_error(&message);
            call_error(&message);
            clear_callback();
            JNI_FALSE
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_galaxyruler_verbatim_FloatingBubbleService_nativeAsrFeedPcm(
    mut env: JNIEnv<'_>,
    _this: JObject<'_>,
    frames: JFloatArray<'_>,
) -> jboolean {
    match native_asr_feed_pcm(&mut env, frames) {
        Ok(()) => JNI_TRUE,
        Err(error) => {
            let message = format!("ASR feed failed: {error}");
            log_native_error(&message);
            call_error(&message);
            JNI_FALSE
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_galaxyruler_verbatim_FloatingBubbleService_nativeAsrStop(
    _env: JNIEnv<'_>,
    _this: JObject<'_>,
) -> jboolean {
    match native_asr_stop() {
        Ok(()) => JNI_TRUE,
        Err(error) => {
            let message = format!("ASR stop failed: {error}");
            log_native_error(&message);
            call_error(&message);
            JNI_FALSE
        }
    }
}

fn native_asr_start(
    env: &mut JNIEnv<'_>,
    model_dir: JString<'_>,
    lang: JString<'_>,
    listener: JObject<'_>,
) -> anyhow::Result<()> {
    let model_dir: String = env.get_string(&model_dir)?.into();
    let lang: String = env.get_string(&lang)?.into();
    let callback = JniCallback::new(env, listener)?;

    *JNI_CALLBACK
        .lock()
        .map_err(|_| anyhow::anyhow!("JNI ASR callback lock is poisoned"))? = Some(callback);
    asr::global_start(AsrModelPaths::for_dir(&PathBuf::from(model_dir)), &lang)?;
    Ok(())
}

fn native_asr_feed_pcm(env: &mut JNIEnv<'_>, frames: JFloatArray<'_>) -> anyhow::Result<()> {
    let len = env.get_array_length(&frames)? as usize;
    let mut samples = vec![0.0f32; len];
    env.get_float_array_region(&frames, 0, &mut samples)?;

    let events = asr::global_feed_pcm(&samples)?;
    dispatch_events(events)
}

fn native_asr_stop() -> anyhow::Result<()> {
    let events = asr::global_stop()?;
    let result = dispatch_events(events);
    let _ = JNI_CALLBACK.lock().map(|mut callback| callback.take());
    result
}

fn dispatch_events(events: Vec<AsrCommandEvent>) -> anyhow::Result<()> {
    for event in events {
        match event {
            AsrCommandEvent::Partial { text } => call_partial(&text)?,
            AsrCommandEvent::Final { text } => call_final(&text)?,
        }
    }
    Ok(())
}

fn call_partial(text: &str) -> anyhow::Result<()> {
    with_callback(|callback| callback.call("onAsrPartial", text))
}

fn call_final(text: &str) -> anyhow::Result<()> {
    with_callback(|callback| callback.call("onAsrFinal", text))
}

fn call_error(text: &str) {
    let _ = with_callback(|callback| callback.call("onAsrError", text));
}

fn clear_callback() {
    let _ = JNI_CALLBACK.lock().map(|mut callback| callback.take());
}

fn log_native_error(message: &str) {
    log::error!("{message}");
    write_android_error_log(message);
}

fn write_android_error_log(message: &str) {
    const ANDROID_LOG_ERROR: i32 = 6;

    unsafe extern "C" {
        fn __android_log_write(prio: i32, tag: *const c_char, text: *const c_char) -> i32;
    }

    let Ok(tag) = CString::new("VerbatimASR") else {
        return;
    };
    let text = message.replace('\0', " ");
    let Ok(text) = CString::new(text) else {
        return;
    };

    unsafe {
        __android_log_write(ANDROID_LOG_ERROR, tag.as_ptr(), text.as_ptr());
    }
}

fn with_callback<T>(f: impl FnOnce(&JniCallback) -> anyhow::Result<T>) -> anyhow::Result<T> {
    let guard = JNI_CALLBACK
        .lock()
        .map_err(|_| anyhow::anyhow!("JNI ASR callback lock is poisoned"))?;
    let callback = guard
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("JNI ASR callback is unavailable"))?;
    f(callback)
}
