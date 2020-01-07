#[macro_use]
mod register;

mod ptr_wrapper;
mod storage;

pub use ptr_wrapper::PtrWrapper;
pub use register::wasmer_data_reg;
pub use storage::wasmer_data_app_storage;
