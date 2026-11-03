# Local model responsiveness

Local startup through `--provider openai-compatible` now discovers the endpoint's chat models, just like the dedicated LM Studio and Ollama choices, unless `--model` is explicit. Discovery saves the endpoint-scoped catalog before runtime construction, preventing a borrowed `alpha` default from hiding the advertised model.

Local model catalogs become eligible for background refresh after 15 seconds, with a 5-second retry guard and one in-flight refresh. Hosted refresh policy is unchanged. The TUI route cache also expires after 15 seconds so reopening `/model` reaches provider catalog access again. Refresh is demand-driven when the catalog is accessed, not a polling timer.

Catalog parsing rejects blank ids, trims ids, prefers nonblank display names, and accepts `max_context_window`. The picker keeps wire ids and adds advertised names to route details. Narrow terminals show the model list instead of squeezing three columns into unusable widths. List pagination accounts for its title row and fills the final viewport.

Each prompt is serialized once and sent as one HTTP JSON body. Incremental SSE events are response output, not prompt uploads. Transport logs explicitly state the body size and this distinction without logging prompt contents. A redundant full transcript copy was removed. These changes do not change LM Studio's server-side tokenization, prefill batch size, GPU offload, or KV cache settings; those require measurements on the user's server to diagnose.
