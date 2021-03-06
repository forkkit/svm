use std::cell::RefCell;
use std::convert::TryFrom;
use std::ffi::c_void;
use std::io;
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;

use log::{debug, error};

use svm_codec::api::builder::{AppTxBuilder, DeployAppTemplateBuilder, SpawnAppBuilder};
use svm_codec::api::raw;

use svm_layout::DataLayout;

use svm_runtime::env::default::DefaultSerializerTypes;
use svm_runtime::{gas::DefaultGasEstimator, Context, ExternImport};

use svm_storage::kv::{ExternKV, StatefulKV};
use svm_types::{Address, State, WasmType};

use crate::RuntimePtr;
use crate::{helpers, raw_error, raw_io_error, raw_utf8_error, raw_validate_error, svm_result_t};
use svm_ffi::{svm_byte_array, svm_env_t, svm_func_callback_t};

use svm_codec::receipt::{encode_app_receipt, encode_exec_receipt, encode_template_receipt};

macro_rules! max_gas {
    ($estimation:expr) => {{
        use svm_gas::Gas;

        match $estimation {
            Gas::Fixed(gas) => gas,
            Gas::Range { max: gas, .. } => gas,
        }
    }};
}

macro_rules! maybe_gas {
    ($gas_metering:expr, $gas_limit:expr) => {{
        use svm_types::gas::MaybeGas;

        if $gas_metering {
            MaybeGas::with($gas_limit)
        } else {
            MaybeGas::new()
        }
    }};
}

macro_rules! addr_to_svm_byte_array {
    ($raw_byte_array:expr, $addr:expr) => {{
        let (ptr, _len, _cap) = $addr.into_raw_parts();
        to_svm_byte_array!($raw_byte_array, ptr, Address::len());
    }};
}

macro_rules! state_to_svm_byte_array {
    ($raw_byte_array:expr, $state:expr) => {{
        let (ptr, _len, _cap) = $state.into_raw_parts();
        to_svm_byte_array!($raw_byte_array, ptr, State::len());
    }};
}

macro_rules! vec_to_svm_byte_array {
    ($raw_byte_array:expr, $vec:expr) => {{
        let len = $vec.len();
        $vec.truncate(len);

        let (ptr, _len, _cap) = $vec.into_raw_parts();

        to_svm_byte_array!($raw_byte_array, ptr, len);
    }};
}

macro_rules! to_svm_byte_array {
    ($raw_byte_array:expr, $ptr:expr, $length:expr) => {{
        let bytes: &mut svm_byte_array = &mut *$raw_byte_array;
        bytes.bytes = $ptr;
        bytes.length = $length as u32;
    }};
}

/// Validates syntactically a raw `deploy template` transaction.
///
/// Should be called while the transaction is in the `mempool` of the Host.
/// In case the transaction isn't valid - the transaction should be discarded.
///
///
/// # Example
///
/// ```rust, no_run
/// use svm_runtime_c_api::*;
///
/// use svm_ffi::svm_byte_array;
/// use svm_types::Address;
///
/// // allocate imports
/// let mut imports = testing::imports_alloc(0);
///
/// // create runtime
///
/// let mut kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut kv) };
/// assert!(res.is_ok());
///
/// let mut runtime = std::ptr::null_mut();
/// let mut error = svm_byte_array::default();
///
/// let res = unsafe { svm_memory_runtime_create(&mut runtime, kv, imports, &mut error) };
/// assert!(res.is_ok());
///
/// let bytes = svm_byte_array::default();
/// let _res = unsafe { svm_validate_template(runtime, bytes, &mut error) };
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_validate_template(
    runtime: *const c_void,
    bytes: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    let runtime = helpers::cast_to_runtime(runtime);

    match runtime.validate_template(bytes.into()) {
        Ok(()) => svm_result_t::SVM_SUCCESS,
        Err(e) => {
            error!("`svm_validate_template` returns `SVM_FAILURE`");
            raw_validate_error(&e, error);
            svm_result_t::SVM_FAILURE
        }
    }
}

/// Validates syntactically a raw `spawn app` transaction.
///
/// Should be called while the transaction is in the `mempool` of the Host.
/// In case the transaction isn't valid - the transaction should be discarded.
///
///
/// # Example
///
/// ```rust, no_run
/// use svm_runtime_c_api::*;
///
/// use svm_ffi::svm_byte_array;
/// use svm_types::Address;
///
/// // allocate imports
/// let mut imports = testing::imports_alloc(0);
///
/// // create runtime
/// let mut kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut kv) };
/// assert!(res.is_ok());
///
/// let mut runtime = std::ptr::null_mut();
/// let mut error = svm_byte_array::default();
///
/// let res = unsafe { svm_memory_runtime_create(&mut runtime, kv, imports, &mut error) };
/// assert!(res.is_ok());
///
/// let bytes = svm_byte_array::default();
/// let _res = unsafe { svm_validate_app(runtime, bytes, &mut error) };
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_validate_app(
    runtime: *const c_void,
    bytes: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    let runtime = helpers::cast_to_runtime(runtime);

    match runtime.validate_app(bytes.into()) {
        Ok(()) => svm_result_t::SVM_SUCCESS,
        Err(e) => {
            error!("`svm_validate_app` returns `SVM_FAILURE`");
            raw_validate_error(&e, error);
            svm_result_t::SVM_FAILURE
        }
    }
}

/// Validates syntactically a raw `execute app` transaction.
/// Returns the `App` address that appears in the transaction.
///
/// # Example
///
/// ```rust, no_run
/// use svm_runtime_c_api::*;
///
/// use svm_ffi::svm_byte_array;
/// use svm_types::Address;
///
/// // allocate imports
/// let mut imports = testing::imports_alloc(0);
///
/// // create runtime
///
/// let mut kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut kv) };
/// assert!(res.is_ok());
///
/// let mut runtime = std::ptr::null_mut();
/// let mut error = svm_byte_array::default();
///
/// let res = unsafe { svm_memory_runtime_create(&mut runtime, kv, imports, &mut error) };
/// assert!(res.is_ok());
///
/// let mut app_addr = svm_byte_array::default();
/// let bytes = svm_byte_array::default();
/// let _res = unsafe { svm_validate_tx(&mut app_addr, runtime, bytes, &mut error) };
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_validate_tx(
    app_addr: *mut svm_byte_array,
    runtime: *const c_void,
    bytes: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    debug!("`svm_validate_tx` start");

    let runtime = helpers::cast_to_runtime(runtime);

    match runtime.validate_tx(bytes.into()) {
        Ok(addr) => {
            // returning encoded `AppReceipt` as `svm_byte_array`.
            // should call later `svm_receipt_destroy`
            addr_to_svm_byte_array!(app_addr, addr.unwrap());

            debug!("`svm_validate_tx` returns `SVM_SUCCESS`");
            svm_result_t::SVM_SUCCESS
        }
        Err(e) => {
            error!("`svm_validate_tx` returns `SVM_FAILURE`");
            raw_validate_error(&e, error);
            svm_result_t::SVM_FAILURE
        }
    }
}

/// Allocates space for the host imports.
/// See `svm_imports_destroy` for freeing the imports.
///
/// # Example
///
/// ```rust
/// use svm_runtime_c_api::svm_imports_alloc;
///
/// let count = 2;
/// let mut imports = std::ptr::null_mut();
///
/// let res = unsafe { svm_imports_alloc(&mut imports, count) };
/// assert!(res.is_ok());
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_imports_alloc(imports: *mut *mut c_void, count: u32) -> svm_result_t {
    let vec: Vec<ExternImport> = Vec::with_capacity(count as usize);

    *imports = svm_common::into_raw_mut(vec);

    svm_result_t::SVM_SUCCESS
}

/// Builds a new `svm_import` (returned via `import` function parameter).
/// New built `svm_import_t` is pushed into `imports`
///
/// # Example
///
/// ```rust
/// use svm_runtime_c_api::*;
///
/// use svm_ffi::{svm_env_t, svm_func_callback_t, svm_byte_array};
/// use svm_types::WasmType;
///
/// unsafe extern "C" fn host_func(
///   env:     *mut svm_env_t,
///   args:    *const svm_byte_array,
///   results: *mut svm_byte_array
/// ) -> *mut svm_byte_array {
///   // ...
///   return std::ptr::null_mut()
/// }
///
/// #[repr(C)]
/// struct function_id(u32);
///
/// // allocate one import
/// let mut imports = testing::imports_alloc(1);
///
/// let namespace = String::from("env").into();
/// let import_name = String::from("foo").into();
/// let params = Vec::<WasmType>::new();
/// let returns = Vec::<WasmType>::new();
/// let mut error = svm_byte_array::default();
///
/// let host_env = svm_common::into_raw_mut(function_id(0));
///
/// let res = unsafe {
///   svm_import_func_new(
///     imports,
///     namespace,
///     import_name,
///     host_func,
///     host_env,
///     params.into(),
///     returns.into(),
///     &mut error)
/// };
/// assert!(res.is_ok());
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_import_func_new(
    imports: *mut c_void,
    namespace: svm_byte_array,
    import_name: svm_byte_array,
    func: svm_func_callback_t,
    host_env: *const c_void,
    params: svm_byte_array,
    returns: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    let imports = helpers::cast_to_imports(imports);

    assert!(imports.len() < imports.capacity());

    let host_env = NonNull::new(host_env as *mut c_void);
    if host_env.is_none() {
        let s = String::from("`host_env` parameter must not be NULL");
        raw_error(s, error);

        return svm_result_t::SVM_FAILURE;
    }

    let host_env: *const c_void = host_env.unwrap().as_ptr();

    let params: Result<Vec<WasmType>, io::Error> = Vec::try_from(params);
    if let Err(e) = params {
        raw_io_error(e, error);
        return svm_result_t::SVM_FAILURE;
    }

    let returns: Result<Vec<WasmType>, io::Error> = Vec::try_from(returns);
    if let Err(e) = returns {
        raw_io_error(e, error);
        return svm_result_t::SVM_FAILURE;
    }

    let import_name = String::try_from(import_name);
    if import_name.is_err() {
        raw_utf8_error(import_name, error);
        return svm_result_t::SVM_FAILURE;
    }

    let namespace = String::try_from(namespace);
    if namespace.is_err() {
        raw_utf8_error(namespace, error);
        return svm_result_t::SVM_FAILURE;
    }

    let import = ExternImport::new(
        import_name.unwrap(),
        namespace.unwrap(),
        params.unwrap(),
        returns.unwrap(),
        func,
        host_env,
    );

    imports.push(import);

    svm_result_t::SVM_SUCCESS
}

macro_rules! box_runtime {
    ($raw_runtime:expr, $runtime:expr) => {{
        let runtime_ptr = RuntimePtr::new(Box::new($runtime));

        //  `svm_runtime_destroy` should be called later for freeing memory.
        *$raw_runtime = svm_common::into_raw_mut(runtime_ptr);

        svm_result_t::SVM_SUCCESS
    }};
}

/// Creates a new in-memory key-value client.
/// Returns a raw pointer to allocated kv-store via input parameter `kv`.
///
/// # Example
///
/// ```rust
/// use svm_runtime_c_api::*;
///
/// let mut kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut kv) };
/// assert!(res.is_ok());
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_memory_state_kv_create(kv: *mut *mut c_void) -> svm_result_t {
    let state_kv = svm_runtime::testing::memory_state_kv_init();

    *kv = svm_common::into_raw_mut(state_kv);

    svm_result_t::SVM_SUCCESS
}

/// Creates a new FFI key-value client.
/// Returns a raw pointer to allocated kv-store via input parameter `kv`.
///
/// # Example
///
/// ```rust
/// use svm_runtime_c_api::*;
///
/// unsafe extern "C" fn get(key_ptr: *const u8, key_len: u32, value_ptr: *mut u8, value_len: *mut u32) {}
/// unsafe extern "C" fn set(key_ptr: *const u8, key_len: u32, value_ptr: *const u8, value_len: u32) {}
/// unsafe extern "C" fn discard() {}
/// unsafe extern "C" fn checkpoint(state: *mut u8) {}
///
/// let mut kv = std::ptr::null_mut();
/// let res = unsafe { svm_ffi_state_kv_create(&mut kv, get, set, discard, checkpoint) };
/// assert!(res.is_ok());
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_ffi_state_kv_create(
    state_kv: *mut *mut c_void,
    get_fn: unsafe extern "C" fn(*const u8, u32, *mut u8, *mut u32),
    set_fn: unsafe extern "C" fn(*const u8, u32, *const u8, u32),
    discard_fn: unsafe extern "C" fn(),
    checkpoint_fn: unsafe extern "C" fn(*mut u8),
) -> svm_result_t {
    let ffi_kv = ExternKV {
        get_fn,
        set_fn,
        discard_fn,
        checkpoint_fn,
        head: None,
    };

    let ffi_kv = Rc::new(RefCell::new(ffi_kv));

    *state_kv = svm_common::into_raw_mut(ffi_kv);

    svm_result_t::SVM_SUCCESS
}

/// Frees an in-memory key-value.
///
/// # Example
///
/// ```rust
/// use svm_runtime_c_api::*;
///
/// let mut kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut kv) };
/// assert!(res.is_ok());
///
/// let res = unsafe { svm_state_kv_destroy(kv) };
/// assert!(res.is_ok());
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_state_kv_destroy(kv: *mut c_void) -> svm_result_t {
    let kv: &mut Rc<RefCell<dyn StatefulKV>> = svm_common::from_raw_mut(kv);

    let _ = Box::from_raw(kv as *mut _);

    svm_result_t::SVM_SUCCESS
}

/// Creates a new SVM Runtime instance baced-by an in-memory KV.
/// Returns it via the `runtime` parameter.
///
/// # Example
///
/// ```rust
/// use svm_runtime_c_api::*;
///
/// use svm_ffi::svm_byte_array;
///
/// let mut runtime = std::ptr::null_mut();
/// let mut imports = testing::imports_alloc(0);
///
/// let mut kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut kv) };
/// assert!(res.is_ok());
///
/// let mut error = svm_byte_array::default();
/// let res = unsafe { svm_memory_runtime_create(&mut runtime, kv, imports, &mut error) };
/// assert!(res.is_ok());
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_memory_runtime_create(
    runtime: *mut *mut c_void,
    state_kv: *mut c_void,
    imports: *const c_void,
    _error: *mut svm_byte_array,
) -> svm_result_t {
    debug!("`svm_memory_runtime_create` start");

    let imports = helpers::cast_to_imports(imports);
    let state_kv = svm_common::from_raw_mut(state_kv);
    let mem_runtime = svm_runtime::testing::create_memory_runtime(state_kv, imports);

    let res = box_runtime!(runtime, mem_runtime);

    debug!("`svm_memory_runtime_create` end");

    res
}

/// Creates a new SVM Runtime instance.
/// Returns it via the `runtime` parameter.
///
/// # Example
///
/// ```rust, no_run
/// use svm_runtime_c_api::*;
///
/// use svm_ffi::svm_byte_array;
///
/// let mut runtime = std::ptr::null_mut();
/// let path = String::from("path goes here").into();
/// let mut imports = testing::imports_alloc(0);
/// let mut error = svm_byte_array::default();
///
/// let res = unsafe { svm_runtime_create(&mut runtime, path, imports, &mut error) };
/// assert!(res.is_ok());
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_runtime_create(
    runtime: *mut *mut c_void,
    kv_path: svm_byte_array,
    imports: *const c_void,
    error: *mut svm_byte_array,
) -> svm_result_t {
    debug!("`svm_runtime_create` start");

    let kv_path: Result<String, std::string::FromUtf8Error> = String::try_from(kv_path);

    if kv_path.is_err() {
        raw_utf8_error(kv_path, error);
        return svm_result_t::SVM_FAILURE;
    }

    let kv_path = kv_path.unwrap();
    let imports = helpers::cast_to_imports(imports);

    let rocksdb_runtime = svm_runtime::create_rocksdb_runtime::<
        &Path,
        DefaultSerializerTypes,
        DefaultGasEstimator,
    >(Path::new(&kv_path), imports);

    let res = box_runtime!(runtime, rocksdb_runtime);

    debug!("`svm_runtime_create` end");

    res
}

/// Deploys a new app-template
///
/// # Example
///
/// ```rust, no_run
/// use svm_runtime_c_api::*;
///
/// use svm_ffi::svm_byte_array;
/// use svm_types::Address;
///
/// // allocate imports
/// let mut imports = testing::imports_alloc(0);
///
/// // create runtime
/// let mut state_kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut state_kv) };
/// assert!(res.is_ok());
///
/// let mut runtime = std::ptr::null_mut();
/// let mut error = svm_byte_array::default();
/// let res = unsafe { svm_memory_runtime_create(&mut runtime, state_kv, imports, &mut error) };
/// assert!(res.is_ok());
///
/// // deploy template
/// let mut receipt = svm_byte_array::default();
/// let author: svm_byte_array = Address::of("@author").into();
/// let template_bytes = svm_byte_array::default();
/// let gas_metering = false;
/// let gas_limit = 0;
///
/// let res = unsafe {
///   svm_deploy_template(
///     &mut receipt,
///     runtime,
///     template_bytes,
///     author,
///     gas_metering,
///     gas_limit,
///     &mut error)
/// };
///
/// assert!(res.is_ok());
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_deploy_template(
    receipt: *mut svm_byte_array,
    runtime: *mut c_void,
    bytes: svm_byte_array,
    author: svm_byte_array,
    gas_metering: bool,
    gas_limit: u64,
    error: *mut svm_byte_array,
) -> svm_result_t {
    debug!("`svm_deploy_template` start`");

    let runtime = helpers::cast_to_runtime_mut(runtime);

    let author: Result<Address, String> = Address::try_from(author);

    if let Err(s) = author {
        raw_error(s, error);
        return svm_result_t::SVM_FAILURE;
    }

    let gas_limit = maybe_gas!(gas_metering, gas_limit);

    let rust_receipt = runtime.deploy_template(bytes.into(), &author.unwrap().into(), gas_limit);

    let mut receipt_bytes = encode_template_receipt(&rust_receipt);

    // returning encoded `TemplateReceipt` as `svm_byte_array`.
    // should call later `svm_receipt_destroy`
    vec_to_svm_byte_array!(receipt, receipt_bytes);

    debug!("`svm_deploy_template` returns `SVM_SUCCESS`");

    svm_result_t::SVM_SUCCESS
}

/// Spawns a new App.
///
/// # Example
///
/// ```rust, no_run
/// use svm_runtime_c_api::*;
///
/// use svm_ffi::svm_byte_array;
/// use svm_types::Address;
///
/// // allocate imports
/// let mut imports = testing::imports_alloc(0);
///
/// // create runtime
///
/// let mut state_kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut state_kv) };
/// assert!(res.is_ok());
///
/// let mut runtime = std::ptr::null_mut();
/// let mut error = svm_byte_array::default();
///
/// let res = unsafe { svm_memory_runtime_create(&mut runtime, state_kv, imports, &mut error) };
/// assert!(res.is_ok());
///
/// let mut app_receipt = svm_byte_array::default();
/// let mut init_state = svm_byte_array::default();
/// let creator = Address::of("@creator").into();
/// let app_bytes = svm_byte_array::default();
/// let gas_metering = false;
/// let gas_limit = 0;
///
/// let _res = unsafe {
///   svm_spawn_app(
///     &mut app_receipt,
///     runtime,
///     app_bytes,
///     creator,
///     gas_metering,
///     gas_limit,
///     &mut error)
/// };
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_spawn_app(
    receipt: *mut svm_byte_array,
    runtime: *mut c_void,
    bytes: svm_byte_array,
    creator: svm_byte_array,
    gas_metering: bool,
    gas_limit: u64,
    error: *mut svm_byte_array,
) -> svm_result_t {
    debug!("`svm_spawn_app` start");

    let runtime = helpers::cast_to_runtime_mut(runtime);
    let creator: Result<Address, String> = Address::try_from(creator);

    if let Err(s) = creator {
        raw_error(s, error);
        return svm_result_t::SVM_FAILURE;
    }

    let gas_limit = maybe_gas!(gas_metering, gas_limit);

    let rust_receipt = runtime.spawn_app(bytes.into(), &creator.unwrap().into(), gas_limit);

    let mut receipt_bytes = encode_app_receipt(&rust_receipt);

    // returning encoded `AppReceipt` as `svm_byte_array`.
    // should call later `svm_receipt_destroy`
    vec_to_svm_byte_array!(receipt, receipt_bytes);

    debug!("`svm_spawn_app` returns `SVM_SUCCESS`");

    svm_result_t::SVM_SUCCESS
}

/// Triggers an app-transaction execution of an already deployed app.
/// Returns the receipt of the execution via the `receipt` parameter.
///
/// # Example
///
/// ```rust, no_run
/// use std::ffi::c_void;
///
/// use svm_runtime_c_api::*;
///
/// use svm_types::{State, Address};
/// use svm_ffi::svm_byte_array;
///
/// // allocate imports
/// let mut imports = testing::imports_alloc(0);
///
/// // create runtime
///
/// let mut state_kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut state_kv) };
/// assert!(res.is_ok());
///
/// let mut runtime = std::ptr::null_mut();
/// let mut error = svm_byte_array::default();
///
/// let res = unsafe { svm_memory_runtime_create(&mut runtime, state_kv, imports, &mut error) };
/// assert!(res.is_ok());
///
/// let mut exec_receipt = svm_byte_array::default();
/// let bytes = svm_byte_array::default();
/// let state = State::empty().into();
/// let gas_metering = false;
/// let gas_limit = 0;
///
/// let _res = unsafe {
///   svm_exec_app(
///     &mut exec_receipt,
///     runtime,
///     bytes,
///     state,
///     gas_metering,
///     gas_limit,
///     &mut error)
/// };
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_exec_app(
    receipt: *mut svm_byte_array,
    runtime: *mut c_void,
    bytes: svm_byte_array,
    state: svm_byte_array,
    gas_metering: bool,
    gas_limit: u64,
    error: *mut svm_byte_array,
) -> svm_result_t {
    debug!("`svm_exec_app` start");

    let runtime = helpers::cast_to_runtime_mut(runtime);
    let state: Result<State, String> = State::try_from(state);

    if let Err(msg) = state {
        raw_error(msg, error);
        return svm_result_t::SVM_FAILURE;
    }

    let gas_limit = maybe_gas!(gas_metering, gas_limit);

    let rust_receipt = runtime.exec_app(bytes.into(), &state.unwrap(), gas_limit);
    let mut receipt_bytes = encode_exec_receipt(&rust_receipt);

    // returning encoded `ExecReceipt` as `svm_byte_array`.
    // should call later `svm_receipt_destroy`
    vec_to_svm_byte_array!(receipt, receipt_bytes);

    debug!("`svm_exec_app` returns `SVM_SUCCESS`");

    svm_result_t::SVM_SUCCESS
}

/// Destroys the Runtime and its associated resources.
///
/// # Example
///
/// ```rust, no_run
/// use svm_runtime_c_api::*;
///
/// use svm_types::Address;
/// use svm_ffi::svm_byte_array;
///
/// // allocate imports
/// let mut imports = testing::imports_alloc(0);
///
/// // create runtime
///
/// let mut state_kv = std::ptr::null_mut();
/// let res = unsafe { svm_memory_state_kv_create(&mut state_kv) };
/// assert!(res.is_ok());
///
/// let mut runtime = std::ptr::null_mut();
/// let mut error = svm_byte_array::default();
/// let res = unsafe { svm_memory_runtime_create(&mut runtime, state_kv, imports, &mut error) };
/// assert!(res.is_ok());
///
/// // destroy runtime
/// unsafe { svm_runtime_destroy(runtime); }
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_runtime_destroy(runtime: *mut c_void) {
    debug!("`svm_runtime_destroy`");

    let _ = Box::from_raw(runtime as *mut RuntimePtr);
}

/// Frees allocated imports resources.
///
/// # Example
///
/// ```rust
/// use svm_runtime_c_api::*;
///
/// // allocate imports
/// let count = 0;
/// let mut imports = std::ptr::null_mut();
/// let _res = unsafe { svm_imports_alloc(&mut imports, count) };
///
/// // destroy imports
/// unsafe { svm_imports_destroy(imports); }
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_imports_destroy(imports: *const c_void) {
    let _ = Box::from_raw(imports as *mut Vec<ExternImport>);
}

/// Frees `svm_byte_array`
///
/// # Example
///
/// ```rust
/// use svm_runtime_c_api::*;
///
/// use svm_ffi::svm_byte_array;
///
/// let bytes = svm_byte_array::default();
/// unsafe { svm_byte_array_destroy(bytes); }
/// ```
///
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_byte_array_destroy(bytes: svm_byte_array) {
    bytes.destroy()
}

#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_wasm_error_create(msg: svm_byte_array) -> *mut svm_byte_array {
    let msg: &[u8] = msg.into();
    let bytes = msg.to_vec();

    let err: svm_byte_array = bytes.into();

    Box::into_raw(Box::new(err))
}

/// Given a raw `deploy-template` transaction (the `bytes` parameter),
/// if it's valid (i.e: passes the `svm_validate_template`), returns `SVM_SUCCESS` and the estimated gas that will be required
/// in order to execute the transaction (via the `estimate` parameter).

/// # Panics
///
/// Panics when `bytes` input is not a valid `deploy-template` raw transaction.
/// Having `bytes` a valid raw input doesn't necessarily imply that `svm_validate_template` passes.
///
#[no_mangle]
pub unsafe extern "C" fn svm_estimate_deploy_template(
    estimation: *mut u64,
    runtime: *mut c_void,
    bytes: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    let runtime = helpers::cast_to_runtime_mut(runtime);

    match runtime.estimate_deploy_template(bytes.into()) {
        Ok(est) => {
            *estimation = max_gas!(est);
            svm_result_t::SVM_SUCCESS
        }
        Err(e) => {
            raw_validate_error(&e, error);
            svm_result_t::SVM_FAILURE
        }
    }
}

/// Given a raw `spawn-app` transaction (the `bytes` parameter),
/// if it's valid (i.e: passes the `svm_validate_app`), returns `SVM_SUCCESS` and the estimated gas that will be required
/// in order to execute the transaction (via the `estimate` parameter).
///
/// # Panics
///
/// Panics when `bytes` input is not a valid `spawn-app` raw transaction.
/// Having `bytes` a valid raw input doesn't necessarily imply that `svm_validate_app` passes.
///
#[no_mangle]
pub unsafe extern "C" fn svm_estimate_spawn_app(
    estimation: *mut u64,
    runtime: *mut c_void,
    bytes: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    let runtime = helpers::cast_to_runtime_mut(runtime);

    match runtime.estimate_spawn_app(bytes.into()) {
        Ok(est) => {
            *estimation = max_gas!(est);
            svm_result_t::SVM_SUCCESS
        }
        Err(e) => {
            raw_validate_error(&e, error);
            svm_result_t::SVM_FAILURE
        }
    }
}

/// Given a raw `exec-app` transaction (the `bytes` parameter),
/// if it's valid (i.e: passes the `svm_validate_tx`), returns `SVM_SUCCESS` and the estimated gas that will be required
/// in order to execute the transaction (via the `estimate` parameter).
///
/// # Panics
///
/// Panics when `bytes` input is not a valid `exec-app` raw transaction.
/// Having `bytes` a valid raw input doesn't necessarily imply that `svm_validate_tx` passes.
///
#[no_mangle]
pub unsafe extern "C" fn svm_estimate_exec_app(
    estimation: *mut u64,
    runtime: *mut c_void,
    bytes: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    let runtime = helpers::cast_to_runtime_mut(runtime);

    match runtime.estimate_exec_app(bytes.into()) {
        Ok(est) => {
            *estimation = max_gas!(est);
            svm_result_t::SVM_SUCCESS
        }
        Err(e) => {
            raw_validate_error(&e, error);
            svm_result_t::SVM_FAILURE
        }
    }
}

/// Constructs a new raw `app_template` transaction.
///
#[no_mangle]
pub unsafe extern "C" fn svm_encode_app_template(
    app_template: *mut svm_byte_array,
    version: u32,
    name: svm_byte_array,
    code: svm_byte_array,
    data: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    let name = String::try_from(name);
    if name.is_err() {
        raw_utf8_error(name, error);
        return svm_result_t::SVM_FAILURE;
    }

    let data: Result<DataLayout, io::Error> = DataLayout::try_from(data);
    if let Err(e) = data {
        raw_io_error(e, error);
        return svm_result_t::SVM_FAILURE;
    }

    let mut bytes = DeployAppTemplateBuilder::new()
        .with_version(version)
        .with_name(&name.unwrap())
        .with_code(code.into())
        .with_data(&data.unwrap())
        .build();

    vec_to_svm_byte_array!(app_template, bytes);

    svm_result_t::SVM_SUCCESS
}

/// Constructs a new raw `spawn_app` transaction.
///
#[no_mangle]
pub unsafe extern "C" fn svm_encode_spawn_app(
    spawn_app: *mut svm_byte_array,
    version: u32,
    template_addr: svm_byte_array,
    ctor_name: svm_byte_array,
    calldata: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    let template_addr: Result<Address, String> = Address::try_from(template_addr);
    if let Err(s) = template_addr {
        raw_error(s, error);
        return svm_result_t::SVM_FAILURE;
    }

    let calldata: &[u8] = calldata.into();
    let calldata: Vec<u8> = calldata.iter().cloned().collect();

    let template_addr = template_addr.unwrap();

    // TODO: return an error instead of `unwrap()`
    let ctor_name = String::try_from(ctor_name).unwrap();

    let mut bytes = SpawnAppBuilder::new()
        .with_version(version)
        .with_template(&template_addr.into())
        .with_ctor(&ctor_name)
        .with_calldata(&calldata)
        .build();

    vec_to_svm_byte_array!(spawn_app, bytes);

    svm_result_t::SVM_SUCCESS
}

/// Constructs a new raw `app_tx` transaction.
///
#[no_mangle]
pub unsafe extern "C" fn svm_encode_app_tx(
    app_tx: *mut svm_byte_array,
    version: u32,
    app_addr: svm_byte_array,
    func_name: svm_byte_array,
    calldata: svm_byte_array,
    error: *mut svm_byte_array,
) -> svm_result_t {
    let app_addr: Result<Address, String> = Address::try_from(app_addr);
    if let Err(s) = app_addr {
        raw_error(s, error);
        return svm_result_t::SVM_FAILURE;
    }

    let calldata: &[u8] = calldata.into();
    let calldata: Vec<u8> = calldata.iter().cloned().collect();

    // TODO: return an error instead of `unwrap()`
    let func_name = String::try_from(func_name).unwrap();
    let app_addr = app_addr.unwrap();

    let mut bytes = AppTxBuilder::new()
        .with_version(version)
        .with_app(&app_addr.into())
        .with_func(&func_name)
        .with_calldata(&calldata)
        .build();

    vec_to_svm_byte_array!(app_tx, bytes);

    svm_result_t::SVM_SUCCESS
}
