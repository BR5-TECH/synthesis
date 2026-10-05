//! Web fetch tool (`WFT-web-fetch-tool.md`).
//!
//! A **provider-native** tool (TLC-FR-21): the application implements no part of
//! it. There is no [`rig::tool::PortableTool`] here, no name, no description, no
//! parameter schema, no constructor, and no call — OpenRouter names the tool the
//! model sees, retrieves the address, extracts the text of a page or a PDF, and
//! produces the result (WFT-FR-01, WFT-FR-07).
//!
//! What this module holds is the one definition entry the request carries
//! (WFT-FR-02), and it holds nothing else: no HTTP client, no browser, no PDF
//! reader, no key of its own, and no fallback that retrieves an address when
//! the provider's own fetch fails (WFT-FR-09). The entry declares its `type` and no
//! other field, so every call is served by OpenRouter's own default behaviour
//! (WFT-FR-03).
//!
//! Separate from `web_search` and never merged with it, so a model holding an
//! address reaches for the page directly rather than searching for what it can
//! already name (WFT-FR-06).

use super::ProviderNativeTool;

/// WFT-FR-04: the one provider that serves this entry (TLC-FR-23).
pub const PROVIDER: &str = "openrouter";

/// WFT-FR-02: the whole of the application's contribution, fixed text compiled
/// into the binary (TLC-FR-22).
pub const ENTRY_TYPE: &str = "openrouter:web_fetch";

/// The entry a request carries, byte-identical on every one of them (WFT-FR-02).
pub fn entry() -> ProviderNativeTool {
    ProviderNativeTool::new(ENTRY_TYPE)
}
