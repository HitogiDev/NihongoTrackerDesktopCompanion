//! Import a Yomitan zip, look a word up, print lemma, deinflection chain and
//! definitions. No UI, no app.
//!
//!   dict-probe import <dictionary.zip> <out-dir>
//!   dict-probe lookup <word> <runs> <dict-dir>...

use std::time::Instant;

use nihongo_dict::Dictionary;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("import") if args.len() >= 4 => import(&args[2], &args[3]),
        Some("lookup") if args.len() >= 5 => {
            let runs: u32 = args[3].parse().unwrap_or(200);
            lookup(&args[4..], &args[2], runs);
        }
        _ => {
            eprintln!("usage:");
            eprintln!("  dict-probe import <dictionary.zip> <out-dir>");
            eprintln!("  dict-probe lookup <word> <runs> <dict-dir>...");
            std::process::exit(1);
        }
    }
}

fn import(zip: &str, out_dir: &str) {
    let started = Instant::now();
    let summary = nihongo_dict::import(zip, out_dir);
    println!("import: {}", if summary.success { "ok" } else { "FAILED" });
    println!("  title: {}", summary.title);
    println!("  terms: {}", summary.term_count);
    println!("  took:  {:.2}s", started.elapsed().as_secs_f64());
    for error in &summary.errors {
        println!("  error: {error}");
    }
    if !summary.success {
        std::process::exit(1);
    }
}

fn lookup(dicts: &[String], word: &str, runs: u32) {
    let opened = Instant::now();
    let dictionary = Dictionary::new();
    for dict in dicts {
        dictionary.add_term_dict(dict);
    }
    println!(
        "open: {:.1}ms ({} dictionaries)",
        opened.elapsed().as_secs_f64() * 1000.0,
        dicts.len()
    );

    let warming = Instant::now();
    dictionary.warm_up();
    println!("warm-up: {:.1}ms", warming.elapsed().as_secs_f64() * 1000.0);

    let cold = Instant::now();
    let rows = dictionary.lookup_uncached(word, 8);
    let cold = cold.elapsed();
    println!(
        "\nlookup {word:?}: {} results, first {:.2}ms",
        rows.len(),
        cold.as_secs_f64() * 1000.0
    );

    for row in &rows {
        println!("---");
        println!("  matched: {}", row.matched);
        println!("  lemma:   {} 【{}】", row.expression, row.reading);
        println!(
            "  chain:   {}",
            if row.process.is_empty() {
                "(none — surface form)".to_string()
            } else {
                row.process.join(" -> ")
            }
        );
        println!("  rules:   {}", row.rules);
        for freq in &row.frequencies {
            println!("  freq:    {} = {}", freq.dict_name, freq.display);
        }
        for gloss in &row.glossaries {
            let text: String = gloss.text.chars().take(160).collect();
            println!("  [{}] {text}…", gloss.dict_name);
        }
    }

    let timed = Instant::now();
    for _ in 0..runs {
        std::hint::black_box(dictionary.lookup_uncached(word, 8));
    }
    println!(
        "\nwarm lookup: {:.3}ms average over {runs} runs",
        timed.elapsed().as_secs_f64() * 1000.0 / f64::from(runs)
    );
}
