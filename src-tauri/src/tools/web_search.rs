//! Web search tool (`WST-web-search-tool.md`).
//!
//! A **provider-native** tool (TLC-FR-21): the application implements no part of
//! it. There is no [`rig::tool::PortableTool`] here, no name, no description, no
//! parameter schema, no constructor, and no call — OpenRouter names the tool the
//! model sees, publishes the schema it composes against, runs the search, and
//! produces the results (WST-FR-01, WST-FR-07).
//!
//! What this module holds is the one definition entry the request carries
//! (WST-FR-02), and it holds nothing else: no search client,
//! no key of its own, and no fallback that answers when the provider's own
//! search fails (WST-FR-09).
//! The entry declares its `type` and no other field, so the search is served by
//! OpenRouter's own default engine selection, `auto` (WST-FR-03).

use super::ProviderNativeTool;

/// WST-FR-04: the one provider that serves this entry. A request carried by any
/// other provider carries nothing of it, and nothing local stands in for it
/// there (TLC-FR-23).
pub const PROVIDER: &str = "openrouter";

/// WST-FR-02: the whole of the application's contribution, fixed text compiled
/// into the binary (TLC-FR-22).
pub const ENTRY_TYPE: &str = "openrouter:web_search";

/// The entry a request carries, byte-identical on every one of them whichever
/// agent was addressed, whichever project is open, and whichever model is
/// selected (WST-FR-02).
pub fn entry() -> ProviderNativeTool {
    ProviderNativeTool::new(ENTRY_TYPE)
}
