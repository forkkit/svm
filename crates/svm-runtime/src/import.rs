use std::convert::{TryFrom, TryInto};
use std::ffi::c_void;
use std::rc::Rc;

use crate::Context;

use wasmer::{Export, Exportable, Function, FunctionType, RuntimeError, Store, Type, Val};

use svm_ffi::{svm_byte_array, svm_env_t, svm_func_callback_t, svm_trap_t};
use svm_types::{WasmType, WasmValue};

#[derive(Debug, Clone)]
pub struct ExternImport {
    name: String,

    namespace: String,

    params: Vec<WasmType>,

    returns: Rc<Vec<WasmType>>,

    func: svm_func_callback_t,

    host_env: *const c_void,
}

impl ExternImport {
    pub fn new(
        name: String,
        namespace: String,
        params: Vec<WasmType>,
        returns: Vec<WasmType>,
        func: svm_func_callback_t,
        host_env: *const c_void,
    ) -> Self {
        Self {
            name,
            namespace,
            params,
            returns: Rc::new(returns),
            func,
            host_env,
        }
    }

    pub fn wasmer_export(&self, store: &Store, ctx: &mut Context) -> (Export, *const svm_env_t) {
        unsafe {
            // The following code has been highly influenced by code here:
            // https://github.com/wasmerio/wasmer/blob/7847acaae1e7a0eade13b65def1f3feeac95efd7/lib/c-api/src/wasm_c_api/externals/func.rs#L86

            let returns_types = self.returns.clone();
            let func = self.func;

            let inner_callback =
                move |env: &mut *mut svm_env_t, args: &[Val]| -> Result<Vec<Val>, RuntimeError> {
                    let args: Vec<WasmValue> = wasmer_vals_to_wasm_vals(args)?;
                    let args: svm_byte_array = args.into();

                    let mut results = svm_ffi::alloc_wasm_values(returns_types.len());
                    let trap = func(*env, &args, &mut results);

                    // manually releasing `args` internals
                    args.destroy();

                    if !trap.is_null() {
                        let trap: Box<svm_trap_t> = Box::from_raw(trap);

                        let err_msg: String = (&*trap).into();
                        let err = RuntimeError::new(err_msg);

                        // manually releasing `results` internals
                        results.destroy();

                        // manually releasing `trap` internals
                        trap.destroy();

                        return Err(err);
                    }

                    let vals = to_wasm_values(&results, &returns_types);

                    // manually releasing `results` internals
                    results.destroy();

                    if let Some(vals) = vals {
                        let vals = wasm_vals_to_wasmer_vals(&vals);

                        Ok(vals)
                    } else {
                        Err(RuntimeError::new("Invalid WASM values"))
                    }
                };

            let func_ty = self.wasmer_function_ty();

            /// making the input `&mut Context` appear as `*const c_void`
            let inner_env = ctx as *mut Context as *const Context as *const c_void;
            let host_env = self.host_env;

            /// The import used `env` (using Wasmer terminology) will be a struct of `svm_env_t`
            /// This `#[repr(C)]` struct will contain two pointers to two types of `env`:
            ///
            /// 1. SVM inner env - a pointer to the `Context`
            ///    Once SVM has finished executing a transaction its memory will be deallocated.
            ///
            /// 2. Host env - a pointer given as input by the so-called `Host`
            ///    The responsibility of release that memory is up to the caller (the `host`).
            let func_env = svm_env_t {
                inner_env,
                host_env,
            };

            /// The heap-allocated `func_env` will be deallocated by later by `SVM` running runtime.
            /// (See method `funcs_envs_destroy` under `src/runtime/default.rs`)
            let func_env = Box::into_raw(Box::new(func_env));

            let func = Function::new_with_env(store, &func_ty, func_env, inner_callback);
            let export = func.to_export();

            let func_env = func_env as *const svm_env_t;

            (export, func_env)
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    fn wasmer_function_ty(&self) -> FunctionType {
        let params = to_wasmer_types(&self.params);
        let returns = to_wasmer_types(&self.returns);

        FunctionType::new(params, returns)
    }
}

fn to_wasmer_types(types: &[WasmType]) -> Vec<Type> {
    types
        .iter()
        .map(|ty| match ty {
            WasmType::I32 => Type::I32,
            WasmType::I64 => Type::I64,
            _ => panic!("Only i32 and i64 are supported."),
        })
        .collect()
}

#[inline]
fn to_wasm_values(bytes: &svm_byte_array, types: &[WasmType]) -> Option<Vec<WasmValue>> {
    let results = Vec::<WasmValue>::try_from(bytes);

    if results.is_err() {
        return None;
    }

    let results = results.unwrap();

    if results.len() != types.len() {
        return None;
    }

    for (val, ty) in results.iter().zip(types.iter()) {
        if val.ty() != *ty {
            return None;
        }
    }

    Some(results)
}

#[inline]
fn wasmer_vals_to_wasm_vals(wasmer_vals: &[Val]) -> Result<Vec<WasmValue>, RuntimeError> {
    let mut values = Vec::new();

    for val in wasmer_vals {
        let value = match val {
            Val::I32(v) => WasmValue::I32(*v as u32),
            Val::I64(v) => WasmValue::I64(*v as u64),
            _ => return Err(RuntimeError::new("Invalid argument type")),
        };

        values.push(value);
    }

    Ok(values)
}

#[inline]
fn wasm_vals_to_wasmer_vals(vals: &[WasmValue]) -> Vec<Val> {
    vals.iter()
        .map(|val| match val {
            WasmValue::I32(v) => Val::I32(*v as i32),
            WasmValue::I64(v) => Val::I64(*v as i64),
        })
        .collect()
}
