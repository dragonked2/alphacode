#![allow(
    unknown_lints,
    clippy::collapsible_match,
    clippy::manual_checked_ops,
    clippy::unnecessary_sort_by,
    clippy::useless_conversion
)]
// The `swarm` tool's `json!` parameter schema is large; the default macro
// recursion limit (128) is exceeded once more properties are added.
#![recursion_limit = "256"]

//! Root `alphacode` crate: the entrypoint + cli layer on top of the merged
//! crate stack (alphacode-base -> alphacode-app-core -> alphacode-tui).
//!
//! All former workspace crates are merged into this single crate. The three
//! presentation layers (base, app-core, tui) are preserved as nested modules
//! with re-exports so `crate::<module>` paths resolve unchanged.

// Leaf crates that the three main layers depend on (no further re-exports needed).
pub mod alphacode_agent_runtime;
pub mod alphacode_ambient_types;
pub mod alphacode_auth_types;
#[cfg(feature = "azure-auth")]
pub mod alphacode_azure_auth;
pub mod alphacode_background_types;
pub mod alphacode_batch_types;
pub mod alphacode_build_meta;
pub mod alphacode_build_support;
pub mod alphacode_command_risk;
pub mod alphacode_compaction_core;
pub mod alphacode_config_types;
pub mod alphacode_core;
#[cfg(feature = "embeddings")]
pub mod alphacode_embedding;
pub mod alphacode_fuzzy;
pub mod alphacode_gateway_types;
pub mod alphacode_harness_api;
#[cfg(unix)]
pub mod alphacode_harness_api_server;
pub mod alphacode_import_core;
pub mod alphacode_logging;
pub mod alphacode_memory_types;
pub mod alphacode_message_types;
pub mod alphacode_notify_email;
pub mod alphacode_overnight_core;
#[cfg(feature = "pdf")]
pub mod alphacode_pdf;
pub mod alphacode_plan;
pub mod alphacode_productivity_core;
pub mod alphacode_provider_anthropic;
pub mod alphacode_provider_antigravity;
pub mod alphacode_provider_bedrock;
pub mod alphacode_provider_claude_cli_runtime;
pub mod alphacode_provider_copilot;
pub mod alphacode_provider_copilot_runtime;
pub mod alphacode_provider_core;
pub mod alphacode_provider_cursor_runtime;
pub mod alphacode_provider_doctor;
pub mod alphacode_provider_env;
pub mod alphacode_provider_gemini;
pub mod alphacode_provider_gemini_runtime;
pub mod alphacode_provider_metadata;
pub mod alphacode_provider_openai;
pub mod alphacode_provider_openai_runtime;
pub mod alphacode_provider_openrouter_runtime;
pub mod alphacode_provider_anthropic_runtime;
pub mod alphacode_provider_antigravity_runtime;
pub mod alphacode_provider_openrouter;
pub mod alphacode_render_core;
pub mod alphacode_selfdev_types;
pub mod alphacode_session_types;
pub mod alphacode_side_panel_types;
pub mod alphacode_setup_hints;
pub mod alphacode_storage;
pub mod alphacode_swarm_core;
pub mod alphacode_task_types;
pub mod alphacode_telemetry_core;
pub mod alphacode_terminal_image;
pub mod alphacode_terminal_launch;
pub mod alphacode_tool_core;
pub mod alphacode_tool_types;
pub mod alphacode_update_core;
pub mod alphacode_usage_types;
pub mod alphacode_protocol;

// Foundation layer
pub mod alphacode_base;
pub use alphacode_base::*;

// Application core layer (re-exports alphacode_base)
pub mod alphacode_app_core;
pub use alphacode_app_core::*;
pub use alphacode_app_core::setup_hints;

// Presentation layer (re-exports alphacode_app_core)
pub mod alphacode_tui;
pub use alphacode_tui::*;

// Re-export sub-crate public items at crate root for internal crate:: paths
pub use alphacode_tui_account_picker::{AccountPickerCommand, AccountPickerItem, AccountPickerSummary, AccountProviderKind};
pub use alphacode_tui_mermaid::DiagramInfo;
pub use alphacode_tui_messages::{DisplayMessage, WrappedLineMap};
pub use alphacode_tui_render::swarm_tiles;
pub use alphacode_tui_render::memory_tiles;
pub use alphacode_tui_style::palette;
pub use alphacode_tui_style::harmony;
pub use alphacode_tui_style::color;
pub use alphacode_tui_markdown::{
    bold_color, code_bg, code_fg, heading_color, heading_h1_color, heading_h2_color,
    heading_h3_color, html_fg, link_fg, math_fg, math_inline_fg, md_dim_color, text_color,
};
pub use alphacode_tui_workspace::{color_support, workspace_map};
pub use alphacode_command_risk::{RiskAssessment, RiskLevel};
pub use alphacode_harness_api::{API_VERSION_MAJOR, ApiEvent, ApiRequest, ClientFrame, ServerFrame};
pub use alphacode_plan::{NodeMeta, PlanItem, VersionedPlan, summarize_plan_graph};
pub use alphacode_provider_core::ResolvedCredential;
pub use alphacode_provider_core::ModelRoute;
pub use alphacode_provider_openrouter::{PinSource, ProviderPin};
pub use alphacode_memory_types::{MemoryEntry, MemoryStore};
// terminal macros are #[macro_export] at crate root via alphacode_core::output_style

// TUI sub-crates (presentation layer leaves)
pub mod alphacode_tui_account_picker;
pub mod alphacode_tui_anim;
pub mod alphacode_tui_core;
pub mod alphacode_tui_markdown;
pub mod alphacode_tui_mermaid;
pub mod alphacode_tui_messages;
pub mod alphacode_tui_permissions;
pub mod alphacode_tui_render;
pub mod alphacode_tui_session_picker;
pub mod alphacode_tui_style;
pub mod alphacode_tui_tool_display;
pub mod alphacode_tui_usage_overlay;
pub mod alphacode_tui_visual_debug;
pub mod alphacode_tui_workspace;

// CLI + entrypoint layer
pub mod cli;

use anyhow::Result;

pub async fn run() -> Result<()> {
    cli::startup::run().await
}
