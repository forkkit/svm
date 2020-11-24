use std::convert::TryFrom;

pub use crate::svm_byte_array;

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct svm_trap_t {
    pub error: svm_byte_array,
}

impl svm_trap_t {
    pub fn alloc(size: usize) -> *mut svm_trap_t {
        let error = svm_byte_array::new(size as usize);

        let trap = svm_trap_t { error };

        Box::into_raw(Box::new(trap))
    }

    pub unsafe fn destroy(self) {
        self.error.destroy()
    }
}

impl From<&svm_trap_t> for String {
    fn from(trap: &svm_trap_t) -> String {
        match String::try_from(&trap.error) {
            Ok(s) => s,
            Err(..) => "svm_trap_t (exact error message had an interpretation error.".to_string(),
        }
    }
}

impl From<String> for svm_trap_t {
    fn from(err: String) -> svm_trap_t {
        let error: svm_byte_array = err.into();

        svm_trap_t { error }
    }
}
