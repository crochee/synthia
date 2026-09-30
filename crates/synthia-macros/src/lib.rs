//! Procedural macros for synthia.
//!
//! # `#[derive(Tool)]`
//!
//! Derives `synthia_tool::Tool` for an argument struct, removing the
//! seven-method boilerplate that every hand-written tool repeats. The
//! struct itself is the tool's input type: the caller adds
//! `serde::Deserialize` and `schemars::JsonSchema` derives, plus one
//! inherent method —
//!
//! ```rust,ignore
//! async fn execute(&self, context: &Context) -> ToolOutput
//! ```
//!
//! — and the macro generates the full trait impl:
//!
//! - `Tool::name` / `Tool::description` from the `#[tool(...)]`
//!   attributes (or defaults, below).
//! - `Tool::parameters` from the struct's `JsonSchema` impl,
//!   converted to a `serde_json::Value`.
//! - `Tool::call` that deserializes the model-supplied JSON into the
//!   struct (a parse failure becomes a model-facing
//!   `ToolOutput::error("Invalid arguments: {e}")` so the model can
//!   retry with corrected arguments) and then awaits `execute`.
//!
//! # Example
//!
//! ```
//! use schemars::JsonSchema;
//! use serde::Deserialize;
//! use synthia_macros::Tool;
//! use synthia_tool::{Context, Tool, ToolOutput};
//!
//! /// Adds two integers.
//! #[derive(Tool, Deserialize, JsonSchema)]
//! #[tool(name = "add_numbers", mode = "sequential")]
//! struct AddNumbers {
//!     /// Left operand.
//!     a: i64,
//!     /// Right operand.
//!     b: i64,
//! }
//!
//! impl AddNumbers {
//!     async fn execute(&self, _context: &Context) -> ToolOutput {
//!         ToolOutput::text((self.a + self.b).to_string())
//!     }
//! }
//!
//! # #[tokio::main]
//! # async fn main() {
//! let tool = AddNumbers { a: 2, b: 3 };
//! assert_eq!(tool.name(), "add_numbers");
//! assert_eq!(tool.description(), "Adds two integers.");
//! assert_eq!(tool.mode(), synthia_tool::ExecutionMode::Sequential);
//!
//! let context = Context::new("s1".into(), std::path::PathBuf::from("/tmp"));
//! let output = tool
//!     .call(serde_json::json!({"a": 2, "b": 3}), &context)
//!     .await;
//! assert!(output.is_text());
//!
//! // Malformed arguments are a model-facing error, not a panic.
//! let bad = tool
//!     .call(serde_json::json!({"a": "two"}), &context)
//!     .await;
//! assert_eq!(bad.is_error, Some(true));
//! # }
//! ```
//!
//! # Attributes
//!
//! All keys are optional:
//!
//! - `#[tool(name = "...")]` — override the tool name. Defaults to the
//!   struct's name converted to `snake_case` (`WebSearch` →
//!   `web_search`). Names are LLM function names: letters, digits,
//!   `_`, and `-` only.
//! - `#[tool(description("..."))]` — override the description.
//!   Defaults to the struct's `///` doc comment; if neither an
//!   attribute nor a doc comment is present, compilation fails (a
//!   descriptionless tool is unusable for tool-choice — fail visible,
//!   not silent).
//! - `#[tool(mode = "sequential")]` — scheduling mode, `"parallel"`
//!   (default, matching `synthia_tool::ExecutionMode::Parallel`) or
//!   `"sequential"`.
//!
//! # Misuse diagnostics
//!
//! All misuse is rejected at compile time: applying the derive to a
//! non-struct or a generic struct, unknown/duplicate/empty attribute
//! keys, an invalid `mode`, a missing description. A missing
//! `execute` method surfaces as rustc's method-resolution error
//! (`no method named execute found`) pointing at the struct.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Data,
    DeriveInput,
    Expr,
    ExprLit,
    Lit,
    LitStr,
    parenthesized,
    parse_macro_input,
    token::Paren,
};

/// Derive `synthia_tool::Tool` for a tool argument struct.
///
/// The struct must be concrete (no generics) and provide an inherent
/// `async fn execute(&self, context: &Context) -> ToolOutput`; the
/// caller is responsible for deriving `serde::Deserialize` and
/// `schemars::JsonSchema` alongside this derive. See the
/// [crate documentation](crate) for the attribute grammar and
/// generated methods.
#[proc_macro_derive(Tool, attributes(tool))]
pub fn derive_tool(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_tool(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[derive(Debug)]
struct ToolAttrs {
    name: Option<LitStr>,
    description: Option<LitStr>,
    mode: Option<LitStr>,
}

/// Expand `#[derive(Tool)]` into the `synthia_tool::Tool` impl.
fn expand_tool(input: &DeriveInput) -> syn::Result<TokenStream2> {
    match &input.data {
        Data::Struct(_) => {}
        other => {
            let kind = match other {
                Data::Enum(_) => "an enum",
                Data::Union(_) => "a union",
                Data::Struct(_) => unreachable!(),
            };
            return Err(syn::Error::new_spanned(
                &input.ident,
                format!(
                    "#[derive(Tool)] requires a struct, but `{}` is {kind}",
                    input.ident
                ),
            ));
        }
    }

    if !input.generics.params.is_empty()
        || input.generics.where_clause.is_some()
    {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[derive(Tool)] does not support generic structs; tool \
             arguments are concrete JSON shapes",
        ));
    }

    let attrs = parse_tool_attrs(input)?;
    let struct_name = &input.ident;

    let tool_name = match &attrs.name {
        Some(lit) => validate_name(lit)?,
        None => to_snake_case(&struct_name.to_string()),
    };
    let tool_description = resolve_description(&attrs.description, input)?;
    let mode_fn = match &attrs.mode {
        Some(lit) => {
            let variant = parse_mode(lit)?;
            Some(quote! { ::synthia_tool::ExecutionMode::#variant })
        }
        None => None,
    };
    let mode_fn = mode_fn.map(|expr| {
        quote! {
            fn mode(&self) -> ::synthia_tool::ExecutionMode {
                #expr
            }
        }
    });

    Ok(quote! {
        #[::async_trait::async_trait]
        impl ::synthia_tool::Tool for #struct_name {
            fn name(&self) -> &str {
                #tool_name
            }

            fn description(&self) -> &str {
                #tool_description
            }

            fn parameters(&self) -> ::serde_json::Value {
                // Type-driven schema generation (what
                // `schemars::schema_for!` expands to): uses the
                // caller's `JsonSchema` derive, so `required`,
                // enum `$defs`, and nullable optional fields are all
                // precise — unlike value-driven inference.
                let schema = ::schemars::SchemaGenerator::default()
                    .into_root_schema_for::<#struct_name>();
                ::serde_json::to_value(schema).unwrap_or_else(|_| {
                    ::serde_json::Value::Object(::serde_json::Map::new())
                })
            }

            #mode_fn

            async fn call(
                &self,
                input: ::serde_json::Value,
                context: &::synthia_tool::Context,
            ) -> ::synthia_tool::ToolOutput {
                // A model-facing error (not a panic): the model can
                // retry the call with corrected arguments.
                let args: #struct_name = match ::serde_json::from_value(input) {
                    ::core::result::Result::Ok(args) => args,
                    ::core::result::Result::Err(err) => {
                        return ::synthia_tool::ToolOutput::error(
                            ::std::format!("Invalid arguments: {err}"),
                        );
                    }
                };
                args.execute(context).await
            }
        }
    })
}

/// Collect the `#[tool(...)]` attribute values, rejecting unknown
/// keys, duplicates, and malformed values at their spans.
fn parse_tool_attrs(input: &DeriveInput) -> syn::Result<ToolAttrs> {
    let mut attrs = ToolAttrs {
        name: None,
        description: None,
        mode: None,
    };

    for attr in &input.attrs {
        if !attr.path().is_ident("tool") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                set_slot(&mut attrs.name, meta.value()?.parse()?, "name")?;
            } else if meta.path.is_ident("description") {
                set_slot(
                    &mut attrs.description,
                    parse_parenthesized_literal(&meta)?,
                    "description",
                )?;
            } else if meta.path.is_ident("mode") {
                set_slot(&mut attrs.mode, meta.value()?.parse()?, "mode")?;
            } else {
                return Err(meta.error(
                    "unknown #[tool] attribute; expected `name`, \
                     `description`, or `mode`",
                ));
            }
            Ok(())
        })?;
    }

    Ok(attrs)
}

/// Store an attribute value, rejecting a duplicate key.
fn set_slot(
    slot: &mut Option<LitStr>,
    value: LitStr,
    key: &str,
) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new(
            value.span(),
            format!("duplicate `#[tool({key})]` attribute"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

/// Parse the function-style `description("...")` form.
fn parse_parenthesized_literal(
    meta: &syn::meta::ParseNestedMeta<'_>,
) -> syn::Result<LitStr> {
    if !meta.input.peek(Paren) {
        return Err(
            meta.error("expected `description(\"...\")` with a string literal")
        );
    }
    let content;
    parenthesized!(content in meta.input);
    let value: LitStr = content.parse()?;
    if !content.is_empty() {
        return Err(syn::Error::new(
            value.span(),
            "`description(...)` takes exactly one string literal",
        ));
    }
    Ok(value)
}

/// Validate an explicit `#[tool(name = "...")]` override.
fn validate_name(lit: &LitStr) -> syn::Result<String> {
    let value = lit.value();
    if value.trim().is_empty() {
        return Err(syn::Error::new(lit.span(), "tool name must not be empty"));
    }
    let valid = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        return Err(syn::Error::new(
            lit.span(),
            "tool names are LLM function names: only letters, digits, \
             `_`, and `-` are allowed",
        ));
    }
    Ok(value)
}

/// Resolve the description: `description(...)` attribute, else the
/// struct's doc comment, else a compile error.
fn resolve_description(
    attr_description: &Option<LitStr>,
    input: &DeriveInput,
) -> syn::Result<String> {
    if let Some(lit) = attr_description {
        let text = lit.value().trim().to_string();
        if text.is_empty() {
            return Err(syn::Error::new(
                lit.span(),
                "#[tool(description(...))] must not be empty",
            ));
        }
        return Ok(text);
    }
    if let Some(doc) = doc_comment(input) {
        return Ok(doc);
    }
    Err(syn::Error::new_spanned(
        &input.ident,
        "#[derive(Tool)] requires a description: add \
         `#[tool(description(\"...\"))]` or a `///` doc comment",
    ))
}

/// Extract the struct's `///` doc comment as a single description.
fn doc_comment(input: &DeriveInput) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for attr in &input.attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        let Ok(name_value) = attr.meta.require_name_value() else {
            continue;
        };
        let Expr::Lit(ExprLit {
            lit: Lit::Str(lit), ..
        }) = &name_value.value
        else {
            continue;
        };
        // Doc lines carry a single leading separator space; drop it.
        lines.push(lit.value().trim_start().to_string());
    }
    let text = lines.join("\n").trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Map a `mode = "..."` literal onto its `ExecutionMode` variant.
fn parse_mode(lit: &LitStr) -> syn::Result<syn::Ident> {
    let variant = match lit.value().as_str() {
        "parallel" => "Parallel",
        "sequential" => "Sequential",
        other => {
            return Err(syn::Error::new(
                lit.span(),
                format!(
                    "unknown tool mode {other:?}; expected \"parallel\" \
                     or \"sequential\""
                ),
            ));
        }
    };
    Ok(syn::Ident::new(variant, lit.span()))
}

/// Convert a `PascalCase` identifier to `snake_case`, keeping acronym
/// runs intact (`WebSearch` → `web_search`, `HTTPClient` →
/// `http_client`, `ParseJSONData` → `parse_json_data`).
fn to_snake_case(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 4);
    for (index, &current) in chars.iter().enumerate() {
        if !current.is_uppercase() {
            out.push(current);
            continue;
        }
        // Start a new segment when the previous character is not an
        // uppercase run-mate (a lowercase letter or a digit), or when
        // the uppercase run ends here (the next character is
        // lowercase, as in `HTTPClient`).
        let prev_breaks = index > 0
            && (chars[index - 1].is_lowercase()
                || chars[index - 1].is_ascii_digit());
        let next_is_lower =
            chars.get(index + 1).is_some_and(|next| next.is_lowercase());
        if index > 0 && (prev_breaks || next_is_lower) {
            out.push('_');
        }
        out.extend(current.to_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> DeriveInput {
        syn::parse_str(src).expect("valid derive input")
    }

    #[test]
    fn snake_case_plain_words() {
        assert_eq!(to_snake_case("WebSearch"), "web_search");
        assert_eq!(to_snake_case("EchoSession"), "echo_session");
    }

    #[test]
    fn snake_case_single_word() {
        assert_eq!(to_snake_case("Read"), "read");
        assert_eq!(to_snake_case("X"), "x");
    }

    #[test]
    fn snake_case_keeps_acronym_runs_intact() {
        assert_eq!(to_snake_case("HTTPClient"), "http_client");
        assert_eq!(to_snake_case("ParseJSONData"), "parse_json_data");
        assert_eq!(to_snake_case("HTTP"), "http");
    }

    #[test]
    fn snake_case_digits_start_new_segment() {
        assert_eq!(to_snake_case("S3Bucket"), "s3_bucket");
        assert_eq!(to_snake_case("V2Ray"), "v2_ray");
    }

    #[test]
    fn snake_case_lowercase_passthrough() {
        assert_eq!(to_snake_case("already"), "already");
    }

    #[test]
    fn description_attribute_wins_over_doc_comment() {
        let input = parse(
            r#"
            /// From the doc comment.
            #[tool(description("From the attribute."))]
            struct Tool { field: i32 }
            "#,
        );
        let attrs = parse_tool_attrs(&input).expect("attrs parse");
        let description = resolve_description(&attrs.description, &input)
            .expect("description");
        assert_eq!(description, "From the attribute.");
    }

    #[test]
    fn doc_comment_is_the_description_default() {
        let input = parse(
            r#"
            /// Adds two
            /// integers.
            struct Adder { a: i32 }
            "#,
        );
        let attrs = parse_tool_attrs(&input).expect("attrs parse");
        let description = resolve_description(&attrs.description, &input)
            .expect("description");
        assert_eq!(description, "Adds two\nintegers.");
    }

    #[test]
    fn missing_description_is_a_compile_error() {
        let input = parse("struct Bare { a: i32 }");
        let attrs = parse_tool_attrs(&input).expect("attrs parse");
        let err = resolve_description(&attrs.description, &input)
            .expect_err("description required");
        assert!(err.to_string().contains("requires a description"));
    }

    #[test]
    fn name_defaults_to_snake_case_struct_name() {
        let input = parse("struct WebSearch { q: String }");
        let attrs = parse_tool_attrs(&input).expect("attrs parse");
        let name = match attrs.name {
            Some(lit) => validate_name(&lit).expect("valid name"),
            None => to_snake_case("WebSearch"),
        };
        assert_eq!(name, "web_search");
    }

    #[test]
    fn non_struct_is_a_compile_error() {
        let input = parse("enum Choice { A, B }");
        let err = expand_tool(&input).expect_err("enums are rejected");
        assert!(err.to_string().contains("requires a struct"));
    }

    #[test]
    fn generic_struct_is_a_compile_error() {
        let input = parse("struct Wrapper<T> { inner: T }");
        let err = expand_tool(&input).expect_err("generics are rejected");
        assert!(err.to_string().contains("does not support generic"));
    }

    #[test]
    fn unknown_attribute_is_a_compile_error() {
        let input = parse(r#"#[tool(unknown = "x")] struct T { a: i32 }"#);
        let err = parse_tool_attrs(&input).expect_err("unknown key rejected");
        assert!(err.to_string().contains("unknown #[tool] attribute"));
    }

    #[test]
    fn duplicate_name_is_a_compile_error() {
        let input =
            parse(r#"#[tool(name = "a", name = "b")] struct T { a: i32 }"#);
        let err = parse_tool_attrs(&input).expect_err("duplicate rejected");
        assert!(err.to_string().contains("duplicate `#[tool(name)]`"));
    }

    #[test]
    fn invalid_name_characters_are_a_compile_error() {
        let input = parse(r#"#[tool(name = "bad name!")] struct T { a: i32 }"#);
        let attrs = parse_tool_attrs(&input).expect("attrs parse");
        let err = validate_name(attrs.name.as_ref().expect("name set"))
            .expect_err("bad name");
        assert!(err.to_string().contains("only letters, digits"));
    }

    #[test]
    fn bad_mode_value_is_a_compile_error() {
        let input = parse(r#"#[tool(mode = "async")] struct T { a: i32 }"#);
        let attrs = parse_tool_attrs(&input).expect("attrs parse");
        let err = parse_mode(attrs.mode.as_ref().expect("mode set"))
            .expect_err("bad mode");
        assert!(err.to_string().contains("unknown tool mode"));
    }

    #[test]
    fn mode_values_map_to_variants() {
        assert_eq!(
            parse_mode(&LitStr::new(
                "parallel",
                proc_macro2::Span::call_site()
            ))
            .expect("valid mode")
            .to_string(),
            "Parallel"
        );
        assert_eq!(
            parse_mode(&LitStr::new(
                "sequential",
                proc_macro2::Span::call_site()
            ))
            .expect("valid mode")
            .to_string(),
            "Sequential"
        );
    }
}
