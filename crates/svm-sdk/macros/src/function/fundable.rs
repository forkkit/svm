use proc_macro2::TokenStream;

use quote::quote;
use syn::Result;

use super::attr;
use attr::{find_attr, has_fundable_attr, FuncAttrKind, FuncAttribute};

pub fn expand(_ast: TokenStream, attrs: &[FuncAttribute]) -> Result<TokenStream> {
    debug_assert!(has_fundable_attr(attrs));

    let attr = find_attr(attrs, FuncAttrKind::Fundable);

    let fund_hook = match attr {
        FuncAttribute::Fundable(s) => s,
        _ => unreachable!(),
    };

    let includes = crate::function::host_includes();

    let ast = quote! {
        {
            #includes;

            let value: svm_sdk::Amount = Node.get_value();

            #fund_hook(value);
        }
    };

    Ok(ast)
}
