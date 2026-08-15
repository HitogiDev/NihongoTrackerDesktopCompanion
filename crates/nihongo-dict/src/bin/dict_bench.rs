//! What a popup actually costs: resolving every offset of a line, the way the
//! reader can hover anywhere in it.
//!
//!   dict-bench <sentences.json> <dict-dir>...
//!
//! `sentences.json` is a JSON array of real Japanese lines (the census script
//! extracts them from the dictionary's own example sentences, so this measures
//! real text rather than something synthetic).

use std::time::{Duration, Instant};

use nihongo_dict::Dictionary;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dict-bench <sentences.json> <dict-dir>...");
        std::process::exit(1);
    }

    let lines = load_lines(&args[1]);
    if lines.is_empty() {
        eprintln!("no usable lines in {}", args[1]);
        std::process::exit(1);
    }

    let dictionary = Dictionary::new();
    for dict in &args[2..] {
        dictionary.add_term_dict(dict);
    }
    dictionary.warm_up();

    let chars: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
    println!(
        "{} lines, {:.1} characters on average (min {}, max {})",
        lines.len(),
        chars.iter().sum::<usize>() as f64 / chars.len() as f64,
        chars.iter().min().unwrap(),
        chars.iter().max().unwrap()
    );

    per_offset(&dictionary, &lines);
    uncached(&dictionary, &lines);
    cached(&dictionary, &lines);
    repeats(&dictionary, &lines);
}

/// One offset, which is what a hover actually asks for. The whole-line numbers
/// below are the pre-computation case, not the interactive one.
fn per_offset(dictionary: &Dictionary, lines: &[String]) {
    let mut per_call: Vec<Duration> = Vec::new();
    for line in lines.iter().take(120) {
        for (at, _) in line.char_indices() {
            let started = Instant::now();
            std::hint::black_box(dictionary.lookup_uncached(&line[at..], 8));
            per_call.push(started.elapsed());
        }
    }
    let wall: Duration = per_call.iter().sum();
    report_calls("single offset (a hover), no cache", &mut per_call, wall);
}

fn report_calls(label: &str, per_call: &mut [Duration], wall: Duration) {
    per_call.sort_unstable();
    let mean = per_call.iter().sum::<Duration>().as_secs_f64() * 1000.0 / per_call.len() as f64;
    let at = |q: f64| {
        let index = ((per_call.len() as f64 - 1.0) * q).round() as usize;
        per_call[index].as_secs_f64() * 1000.0
    };
    println!("\n{label}: {} calls in {:.1}s", per_call.len(), wall.as_secs_f64());
    println!("  mean   {mean:8.3} ms");
    println!("  p50    {:8.3} ms", at(0.50));
    println!("  p95    {:8.3} ms", at(0.95));
    println!("  p99    {:8.3} ms", at(0.99));
    println!("  max    {:8.3} ms", at(1.0));
}

fn load_lines(path: &str) -> Vec<String> {
    let raw = std::fs::read_to_string(path).expect("read sentences");
    // A hand-rolled reader for a flat array of strings, to keep the crate free
    // of a JSON dependency it would not otherwise need.
    raw.split('"')
        .filter(|piece| piece.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c) || ('\u{4e00}'..='\u{9fff}').contains(&c)))
        .map(|piece| piece.to_string())
        .filter(|line| (20..=120).contains(&line.chars().count()))
        .collect()
}

/// Per-line cost with the cache disabled: the honest worst case, and what a
/// cold service would pay for every new line.
fn uncached(dictionary: &Dictionary, lines: &[String]) {
    let mut per_line: Vec<Duration> = Vec::with_capacity(lines.len());
    let started = Instant::now();
    for line in lines {
        let at = Instant::now();
        std::hint::black_box(dictionary.resolve_line_uncached(line, 8));
        per_line.push(at.elapsed());
    }
    let wall = started.elapsed();
    report("no cache", &mut per_line, wall, lines);
}

/// The same lines through the LRU, each seen once. This is the realistic
/// first-pass cost: within a single line, offsets share suffixes and the reader
/// repeats particles constantly.
fn cached(dictionary: &Dictionary, lines: &[String]) {
    dictionary.clear_cache();
    dictionary.reset_stats();

    let mut per_line: Vec<Duration> = Vec::with_capacity(lines.len());
    let started = Instant::now();
    for line in lines {
        let at = Instant::now();
        std::hint::black_box(dictionary.resolve_line(line, 8));
        per_line.push(at.elapsed());
    }
    let wall = started.elapsed();
    let (hits, misses) = dictionary.cache_stats();
    report("LRU, each line once", &mut per_line, wall, lines);
    println!(
        "    cache: {hits} hits / {} lookups = {:.1}% hit rate",
        hits + misses,
        hits as f64 * 100.0 / (hits + misses).max(1) as f64
    );
}

/// Ten consecutive lines of the same text, which is how a reader actually moves
/// through a visual novel. Reported per window so the warm-up effect is visible.
fn repeats(dictionary: &Dictionary, lines: &[String]) {
    const WINDOW: usize = 10;
    if lines.len() < WINDOW {
        return;
    }
    println!("\nconsecutive windows of {WINDOW} lines (fresh cache per window):");

    let mut rates = Vec::new();
    let mut first_line = Vec::new();
    let mut later_lines = Vec::new();

    for window in lines.chunks(WINDOW).take(20) {
        dictionary.clear_cache();
        dictionary.reset_stats();
        for (index, line) in window.iter().enumerate() {
            let at = Instant::now();
            std::hint::black_box(dictionary.resolve_line(line, 8));
            if index == 0 {
                first_line.push(at.elapsed());
            } else {
                later_lines.push(at.elapsed());
            }
        }
        let (hits, misses) = dictionary.cache_stats();
        rates.push(hits as f64 * 100.0 / (hits + misses).max(1) as f64);
    }

    let mean = |values: &[Duration]| {
        values.iter().sum::<Duration>().as_secs_f64() * 1000.0 / values.len().max(1) as f64
    };
    println!("  hit rate: {:.1}% average over {} windows", rates.iter().sum::<f64>() / rates.len() as f64, rates.len());
    println!("  first line of a window: {:.2} ms", mean(&first_line));
    println!("  lines 2-{WINDOW}:            {:.2} ms", mean(&later_lines));
}

fn report(label: &str, per_line: &mut [Duration], wall: Duration, lines: &[String]) {
    per_line.sort_unstable();
    let mean = per_line.iter().sum::<Duration>().as_secs_f64() * 1000.0 / per_line.len() as f64;
    let at = |q: f64| {
        let index = ((per_line.len() as f64 - 1.0) * q).round() as usize;
        per_line[index].as_secs_f64() * 1000.0
    };
    let total_chars: usize = lines.iter().map(|l| l.chars().count()).sum();

    println!("\n{label}:");
    println!("  mean   {mean:8.2} ms per line");
    println!("  p50    {:8.2} ms", at(0.50));
    println!("  p95    {:8.2} ms", at(0.95));
    println!("  p99    {:8.2} ms", at(0.99));
    println!("  max    {:8.2} ms", at(1.0));
    println!(
        "  cpu    {:8.2} ms per 1,000 characters ({:.1} s of wall for {} lines)",
        wall.as_secs_f64() * 1000.0 * 1000.0 / total_chars as f64,
        wall.as_secs_f64(),
        lines.len()
    );
}
