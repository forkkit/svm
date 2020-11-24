#![feature(vec_into_raw_parts)]

mod address;
mod byte_array;
mod callback;
mod env;
mod layout;
mod macros;
mod state;
mod trap;
mod types;
mod value;

pub use byte_array::svm_byte_array;
pub mod tracking;
pub use callback::svm_func_callback_t;
pub use env::svm_env_t;
pub use trap::svm_trap_t;
pub use value::alloc_wasm_values;

use std::ffi::c_void;

/// Receives an object, and returns a raw `*const c_void` pointer to it.
#[must_use]
#[inline]
pub fn into_raw<T>(obj: T) -> *const c_void {
    let ptr: *mut T = Box::into_raw(Box::new(obj));

    ptr as *const T as _
}

/// Receives an object, and returns a raw `*mut c_void` pointer to it.
#[must_use]
#[inline]
pub fn into_raw_mut<T>(obj: T) -> *mut c_void {
    let ptr: *mut T = Box::into_raw(Box::new(obj));

    ptr as _
}

#[must_use]
#[inline]
pub fn from_raw<T>(ptr: *const c_void) -> T {
    let ptr: *mut T = ptr as *const T as *mut T;

    unsafe { *Box::from_raw(ptr) }
}

/// # Safety
///
/// Receives a `*const c_void` pointer and returns the a borrowed reference to the underlying object.
#[must_use]
#[inline]
pub unsafe fn r#as<'a, T>(ptr: *const c_void) -> &'a T {
    &*(ptr as *const T)
}

/// # Safety
///
/// Receives a `*const c_void` pointer and returns the a mutable borrowed reference to the underlying object.
#[must_use]
#[inline]
pub unsafe fn as_mut<'a, T>(ptr: *mut c_void) -> &'a mut T {
    &mut *(ptr as *mut T)
}