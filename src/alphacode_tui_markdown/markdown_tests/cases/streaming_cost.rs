// Baseline measurement for the streaming-markdown render path.
//
// Drives `IncrementalMarkdownRenderer::update_shared` over a simulated
// token stream and reports the per-batch cost, so the cost of re-parsing the
// *whole* accumulated response on every batch is visible as the growth
// curve.
//
// Run with:
//   cargo test --release --lib streaming_markdown_cost -- --nocapture
//
// The output is a table of (batch index, bytes accumulated, batch duration).
// A linear-per-batch cost means the total work is O(n^2) in the response
// length.

use std::time::Instant;

fn synth_response(total_bytes: usize) -> String {
    // Markdown-ish prose with headings, lists and code fences so the parser
    // does real work rather than emitting one text event per paragraph.
    let mut out = String::with_capacity(total_bytes + 1024);
    let mut i = 0usize;
    while out.len() < total_bytes {
        out.push_str(&format!(
            "## Section {i}\n\n\
             This is paragraph {i} of the streamed response. It contains **bold** text, \
             `inline code`, and a [link](https://example.com/{i}) so the parser has \
             several inline event kinds to process.\n\n\
             - bullet one for section {i}\n\
             - bullet two for section {i}\n\
             - bullet three for section {i}\n\n\
             ```rust\n\
             fn section_{i}() -> usize {{ {i} }}\n\
             ```\n\n",
            i = i
        ));
        i += 1;
    }
    out
}

#[test]
fn streaming_markdown_cost() {
    let total_bytes: usize = 256 * 1024;
    let text = synth_response(total_bytes);
    let eol = text.len();
    println!("streaming-markdown baseline: {eol} bytes total");

    let mut renderer = crate::alphacode_tui_markdown::IncrementalMarkdownRenderer::new(Some(100));
    // Batch size approximating one streamed token burst.
    let batch = 512usize;
    let mut idx = 0usize;
    let mut batches: Vec<(usize, usize, f64)> = Vec::new();
    let mut total = std::time::Duration::ZERO;

    while idx < text.len() {
        let end = (idx + batch).min(text.len());
        let slice = &text[idx..end];
        let started = Instant::now();
        let lines = renderer.update_shared(slice);
        let elapsed = started.elapsed();
        total += elapsed;
        batches.push((batches.len(), slice.len(), elapsed.as_secs_f64() * 1000.0));
        // Consume the lines so the render is not optimised away.
        std::hint::black_box(lines.len());
        idx = end;
    }

    // Print a decimated table plus the growth signal.
    println!("batch\tdelta_bytes\tms");
    let step = (batches.len() / 20).max(1);
    for (i, (idx, bytes, ms)) in batches.iter().enumerate() {
        if i % step == 0 || i + 1 == batches.len() {
            println!("{idx}\t{bytes}\t{ms:.3}");
        }
    }
    println!("TOTAL_MS\t{:.3}", total.as_secs_f64() * 1000.0);

    // Growth signal: mean cost of the first 10% of batches vs the last 10%.
    let n = batches.len();
    let tenth = (n / 10).max(1);
    let first: f64 = batches[..tenth].iter().map(|(_, _, ms)| *ms).sum::<f64>() / tenth as f64;
    let last: f64 = batches[n - tenth..].iter().map(|(_, _, ms)| *ms).sum::<f64>() / tenth as f64;
    println!("FIRST_TENTH_MEAN_MS\t{first:.4}");
    println!("LAST_TENTH_MEAN_MS\t{last:.4}");
    println!("GROWTH_RATIO\t{:.2}", last / first.max(f64::MIN_POSITIVE));
}