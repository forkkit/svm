use std::ffi::c_void;
use std::slice;

use log::{debug, error};

use svm_app::{
    default::DefaultJsonSerializerTypes,
    types::{App, AppTemplate, AppTransaction},
};

use svm_common::{Address, State};
use svm_runtime::{ctx::SvmCtx, settings::AppSettings, traits::Runtime, Receipt};

use crate::{
    helpers, svm_byte_array, svm_import_func_sig_t, svm_import_func_t, svm_import_kind,
    svm_import_t, svm_import_value, svm_result_t, svm_value_t, svm_value_type,
    svm_value_type_array, RuntimePtr,
};

/// Creates a new SVM Runtime instance.
/// Returns it via the `runtime` parameter.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_runtime_create(
    runtime: *mut *mut c_void,
    path_bytes: *const c_void,
    path_len: libc::c_uint,
    host: *mut c_void,
    imports: *const *const svm_import_t,
    imports_len: libc::c_uint,
) -> svm_result_t {
    debug!("`svm_runtime_create` start");

    let slice = std::slice::from_raw_parts(path_bytes as *const u8, path_len as usize);
    let path = String::from_utf8(slice.to_vec());

    if let Err(_err) = path {
        // update_last_error(err);
        return svm_result_t::SVM_FAILURE;
    }

    let wasmer_imports = helpers::cast_imports_to_wasmer_imports(imports, imports_len);
    let rt = svm_runtime::create_rocksdb_runtime::<String, DefaultJsonSerializerTypes>(
        host,
        &path.unwrap(),
        wasmer_imports,
    );
    let boxed_rt: Box<dyn Runtime> = Box::new(rt);

    let rt_ptr = RuntimePtr::new(boxed_rt);
    *runtime = svm_common::into_raw_mut(rt_ptr);

    debug!("`svm_runtime_create` end");

    svm_result_t::SVM_SUCCESS
}

/// Destroys the Runtime and it's associated resources.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_runtime_destroy(runtime: *mut c_void) -> svm_result_t {
    debug!("`svm_runtime_destroy`");

    let _runtime: Box<RuntimePtr> = Box::from_raw(runtime as *mut RuntimePtr);

    svm_result_t::SVM_SUCCESS
}

#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_import_func_build(
    import: *mut *mut svm_import_t,
    module_name: svm_byte_array,
    import_name: svm_byte_array,
    func: *const c_void,
    params: svm_value_type_array,
    returns: svm_value_type_array,
) -> svm_result_t {
    let sig = svm_import_func_sig_t {
        params: params.types,
        returns: returns.types,
        params_len: params.types_len,
        returns_len: returns.types_len,
    };

    let func = svm_import_func_t { func, sig };
    let func = Box::into_raw(Box::new(func));

    let svm_import = svm_import_t {
        module_name,
        import_name,
        kind: svm_import_kind::SVM_FUNCTION,
        value: svm_import_value { func },
    };

    *import = Box::into_raw(Box::new(svm_import));

    svm_result_t::SVM_SUCCESS
}

/// Deploys a new app-template
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_deploy_template(
    template_addr: *mut *mut c_void,
    runtime: *mut c_void,
    bytes: *const c_void,
    bytes_len: u64,
) -> svm_result_t {
    debug!("`svm_deploy_template` start`");

    let bytes = std::slice::from_raw_parts(bytes as *const u8, bytes_len as usize);
    let runtime = helpers::cast_to_runtime_mut(runtime);

    match runtime.deploy_template(&bytes) {
        Ok(addr) => {
            *template_addr = svm_common::into_raw_mut(addr);
            debug!("`svm_deploy_template`` returns `SVM_SUCCESS`");
            svm_result_t::SVM_SUCCESS
        }
        Err(_err) => {
            // update_last_error(err);
            error!("`svm_deploy_template` returns `SVM_FAILURE`");
            svm_result_t::SVM_FAILURE
        }
    }
}

/// Spawns a new App.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_spawn_app(
    app_addr: *mut *mut c_void,
    runtime: *mut c_void,
    bytes: *const c_void,
    bytes_len: u64,
) -> svm_result_t {
    debug!("`svm_spawn_app` start");

    let bytes = std::slice::from_raw_parts(bytes as *const u8, bytes_len as usize);
    let runtime = helpers::cast_to_runtime_mut(runtime);

    match runtime.spawn_app(bytes) {
        Ok(addr) => {
            *app_addr = svm_common::into_raw_mut(addr);
            debug!("`svm_spawn_app` returns `SVM_SUCCESS`");
            svm_result_t::SVM_SUCCESS
        }
        Err(_e) => {
            // update_last_error(error);
            error!("`svm_spawn_app` returns `SVM_FAILURE`");
            svm_result_t::SVM_FAILURE
        }
    }
}

#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_parse_exec_app(
    app_tx: *mut *mut c_void,
    runtime: *const c_void,
    bytes: *const c_void,
    bytes_len: u64,
) -> svm_result_t {
    debug!("`svm_parse_exec_app` start");

    let bytes = std::slice::from_raw_parts(bytes as *const u8, bytes_len as usize);
    let runtime = helpers::cast_to_runtime(runtime);

    match runtime.parse_exec_app(bytes) {
        Ok(tx) => {
            *app_tx = svm_common::into_raw_mut(tx);
            debug!("`svm_parse_exec_app` returns `SVM_SUCCESS`");
            svm_result_t::SVM_SUCCESS
        }
        Err(_error) => {
            // update_last_error(error);
            error!("`svm_parse_exec_app` returns `SVM_FAILURE`");
            svm_result_t::SVM_FAILURE
        }
    }
}

/// Triggers an app-transaction execution of an already deployed app.
///
/// Returns the receipt of the execution via the `receipt` parameter.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_exec_app(
    receipt: *mut *mut c_void,
    runtime: *mut c_void,
    app_tx: *const c_void,
    state: *const c_void,
) -> svm_result_t {
    debug!("`svm_exec_app` start");

    let app_tx = *Box::from_raw(app_tx as *mut AppTransaction);
    let runtime = helpers::cast_to_runtime_mut(runtime);
    let state = State::from(state);

    match runtime.exec_app(app_tx, state) {
        Ok(r) => {
            *receipt = svm_common::into_raw_mut(r);

            debug!("`svm_exec_app` returns `SVM_SUCCESS`");
            svm_result_t::SVM_SUCCESS
        }
        Err(e) => {
            // update_last_error(e);
            error!("`svm_exec_app` returns `SVM_FAILURE`");
            svm_result_t::SVM_FAILURE
        }
    }
}

#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_instance_context_host_get(ctx: *mut c_void) -> *mut c_void {
    use wasmer_runtime_core::vm::Ctx;

    let wasmer_ctx = svm_common::from_raw::<Ctx>(ctx);
    let svm_ctx = svm_common::from_raw::<SvmCtx>(wasmer_ctx.data);

    svm_ctx.host
}

/// Returns the receipt outcome (`true` for success and `false` otherwise)
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_receipt_status(raw_receipt: *const c_void) -> bool {
    let receipt = svm_common::from_raw::<Receipt>(raw_receipt);

    debug!("`svm_receipt_status` status={}", receipt.success);

    receipt.success
}

/// Returns the transaction execution results (wasm array).
/// Should be called only after verifying that the transaction succeeded.
/// Will panic when called for a failed transaction.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_receipt_results(
    raw_results: *mut *mut svm_value_t,
    raw_receipt: *const c_void,
    results_len: *mut u32,
) {
    debug!("`svm_receipt_results`");

    let receipt = svm_common::from_raw::<Receipt>(raw_receipt);

    if receipt.success {
        let mut results: Vec<svm_value_t> = Vec::with_capacity(*results_len as usize);

        for value in receipt.results.iter() {
            let raw_value = svm_value_t::from(value);
            results.push(raw_value);
        }

        // TODO: free `results` memory after usage
        let results: &mut Vec<svm_value_t> = Box::leak(Box::new(results));

        *results_len = results.len() as u32;
        *raw_results = results.as_mut_ptr();
    } else {
        let msg = "method not allowed to be called when transaction execution failed";
        error!("{}", msg);
        panic!("{}", msg);
    }
}

/// Returns the `receipt` error in transaction failed
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_receipt_error(raw_receipt: *const c_void) {
    let receipt = svm_common::from_raw::<Receipt>(raw_receipt);

    if let Some(ref _e) = receipt.error {
        // update_last_error(e);
    }
}

/// Returns a pointer to the new state of the app account.
#[must_use]
#[no_mangle]
pub unsafe extern "C" fn svm_receipt_new_state(raw_receipt: *const c_void) -> *const u8 {
    let receipt = svm_common::from_raw::<Receipt>(raw_receipt);

    if receipt.success {
        let state = receipt.new_state.as_ref().unwrap();
        state.as_ptr()
    } else {
        panic!("method not allowed to be called when transaction execution failed");
    }
}
