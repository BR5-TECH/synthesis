//! The `project_scoped` attribute (`PWN-project-windows.md` PWN-FR-CTXT) and
//! the `blocking_command` attribute (`CEX-command-execution.md`).
//!
//! ## `project_scoped`
//!
//! It goes above `#[tauri::command]`. It runs the body of the command in the
//! context of the project of the call, so every call below the body can read
//! the project with `project_scope::context::current()`.
//!
//! The attribute reads the project id from the first parameter of type
//! `Scoped<_>`. A command without such a parameter gets one hidden parameter of
//! type `ProjectKey`, which reads the `projectId` field of the invoke payload.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
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

/// CEX-FR-JBTB, CEX-FR-KFVT: a synchronous Tauri command that runs off the
/// main thread.
///
/// The attribute goes in place of `#[tauri::command]`, below
/// `#[project_scoped]` when the command has both. It emits
/// `#[tauri::command(async, ..)]`, so Tauri runs the command on a worker of
/// the async runtime and not on the main thread. It wraps the body in
/// `crate::command_thread::run_blocking`, which tells the runtime that the
/// body blocks. The function stays synchronous, so a test calls it directly.
/// The arguments of the attribute go to `tauri::command` unchanged.
#[proc_macro_attribute]
pub fn blocking_command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let function = parse_macro_input!(item as ItemFn);
    match expand_blocking_command(attr.into(), function) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.into_compile_error().into(),
    }
}

fn expand_blocking_command(attr: TokenStream2, mut function: ItemFn) -> syn::Result<TokenStream2> {
    if let Some(token) = function.sig.asyncness {
        return Err(syn::Error::new_spanned(
            token,
            "blocking_command is for a synchronous command; an async fn takes #[tauri::command]",
        ));
    }
    let output = match &function.sig.output {
        ReturnType::Default => quote!(()),
        ReturnType::Type(_, ty) => quote!(#ty),
    };
    let body = &function.block;
    let wrapped = quote!({
        crate::command_thread::run_blocking(|| -> #output #body)
    });
    *function.block = syn::parse2(wrapped)?;
    let args = if attr.is_empty() { quote!(async) } else { quote!(async, #attr) };
    Ok(quote!(#[tauri::command(#args)] #function))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(attr: TokenStream2, item: TokenStream2) -> syn::Result<String> {
        let function: ItemFn = syn::parse2(item)?;
        expand_blocking_command(attr, function).map(|tokens| tokens.to_string().replace(' ', ""))
    }

    // CEX-FR-JBTB: the attribute registers the command to run off the main
    // thread and keeps the function synchronous.
    #[test]
    fn emits_an_async_tauri_command_and_keeps_the_function_sync() {
        let out = expand(
            quote!(),
            quote!(
                pub fn read(a: String) -> Result<u8, String> {
                    Ok(1)
                }
            ),
        )
        .unwrap();
        assert!(out.starts_with("#[tauri::command(async)]pubfnread("), "{out}");
        assert!(!out.contains("asyncfn"), "{out}");
    }

    // CEX-FR-KFVT: the body runs under run_blocking with the declared return type.
    #[test]
    fn wraps_the_body_in_run_blocking() {
        let out = expand(
            quote!(),
            quote!(
                pub fn read() -> u8 {
                    1
                }
            ),
        )
        .unwrap();
        assert!(out.contains("crate::command_thread::run_blocking(||->u8{1})"), "{out}");
        let unit = expand(
            quote!(),
            quote!(
                pub fn touch() {}
            ),
        )
        .unwrap();
        assert!(unit.contains("run_blocking(||->(){})"), "{unit}");
    }

    // CEX-FR-JBTB: arguments of the attribute go to tauri::command unchanged.
    #[test]
    fn passes_its_arguments_to_tauri_command() {
        let out = expand(
            quote!(rename_all = "snake_case"),
            quote!(
                pub fn read() {}
            ),
        )
        .unwrap();
        assert!(out.starts_with("#[tauri::command(async,rename_all=\"snake_case\")]"), "{out}");
    }

    // CEX-FR-JBTB: generics and where clauses stay on the function.
    #[test]
    fn keeps_generics() {
        let out = expand(
            quote!(),
            quote!(
                pub fn read<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> u8
                where
                    R: Send,
                {
                    1
                }
            ),
        )
        .unwrap();
        assert!(
            out.contains("pubfnread<R:tauri::Runtime>(app:tauri::AppHandle<R>)->u8whereR:Send,{"),
            "{out}"
        );
    }

    // CEX-FR-JBTB: an async fn is refused, because it already runs off the main thread.
    #[test]
    fn refuses_an_async_fn() {
        let error = expand(
            quote!(),
            quote!(
                pub async fn read() {}
            ),
        )
        .unwrap_err();
        assert!(error.to_string().contains("synchronous command"), "{error}");
    }
}
