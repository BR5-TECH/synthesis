//! The `project_scoped` attribute (`PWN-project-windows.md` PWN-FR-CTXT).
//!
//! It goes above `#[tauri::command]`. It runs the body of the command in the
//! context of the project of the call, so every call below the body can read
//! the project with `project_scope::context::current()`.
//!
//! The attribute reads the project id from the first parameter of type
//! `Scoped<_>`. A command without such a parameter gets one hidden parameter of
//! type `ProjectKey`, which reads the `projectId` field of the invoke payload.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, Pat, ReturnType};

fn scoped_param(arg: &FnArg) -> Option<syn::Ident> {
    let FnArg::Typed(typed) = arg else {
        return None;
    };
    let ty = {
        let t = &typed.ty;
        quote!(#t)
    }
    .to_string()
    .replace(' ', "");
    if !ty.contains("Scoped<") {
        return None;
    }
    match &*typed.pat {
        Pat::Ident(ident) => Some(ident.ident.clone()),
        _ => None,
    }
}

#[proc_macro_attribute]
pub fn project_scoped(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut function = parse_macro_input!(item as ItemFn);
    let id_source = match function.sig.inputs.iter().find_map(scoped_param) {
        Some(param) => quote!(#param.project_id().to_string()),
        None => {
            let hidden: FnArg = syn::parse_quote!(__project_key: crate::project_scope::ProjectKey);
            function.sig.inputs.push(hidden);
            quote!(__project_key.as_str().to_string())
        }
    };
    let output = match &function.sig.output {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };
    let body = &function.block;
    let wrapped = if function.sig.asyncness.is_some() {
        quote!({
            let __project_id = #id_source;
            crate::project_scope::context::run_future_in_project(__project_id, async move {
                let __result: #output = #body;
                __result
            })
            .await
        })
    } else {
        quote!({
            let __project_id = #id_source;
            crate::project_scope::context::run_in_project(&__project_id, || -> #output #body)
        })
    };
    *function.block = syn::parse2(wrapped).expect("the wrapped body is a block");
    // The hidden parameter can take a command over the argument limit of
    // Clippy. The author wrote the parameters that count. The allow goes on
    // the one function that the attribute changes.
    quote!(#[allow(clippy::too_many_arguments)] #function).into()
}
