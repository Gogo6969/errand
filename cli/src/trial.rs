//! Put what went wrong to a model again, the way Errand would, and count.
//!
//!     errand-trial --base-url URL --model NAME [--provider P] [--key-from ID]
//!                  [--runs N] [--only a,b] [--out FILE] [--context TOKENS]
//!
//! The scenarios and the rules are in `errand_core::local::trial`. Nothing a
//! model asks for is done: every tool answers from the scenario's script.
//!
//! A key, where the server wants one, is read from where Errand keeps it, by
//! the id Errand keeps it under, and goes only into the requests. It is never
//! printed, and never written into the results.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use errand_core::local::talk::LlmClient;
use errand_core::local::trial::{self, Tally, Verdict};
use errand_core::local::LlmSettings;

const HOW: &str = "errand-trial --base-url URL --model NAME [--provider openai-compat|llamacpp] \
[--key-from BACKEND-ID] [--runs 10] [--only phantom-job,the-wall] [--out results.jsonl] \
[--context 131072]";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HOW}\n\nScenarios:");
        for s in trial::scenarios() {
            println!("  {:<20} {}", s.id, s.checks);
        }
        return Ok(());
    }
    let flag = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|at| args.get(at + 1))
            .cloned()
    };
    for (at, arg) in args.iter().enumerate() {
        let known = [
            "--base-url",
            "--model",
            "--provider",
            "--key-from",
            "--runs",
            "--only",
            "--out",
            "--context",
        ];
        let is_a_value = at > 0 && known.contains(&args[at - 1].as_str());
        if !is_a_value && !known.contains(&arg.as_str()) {
            bail!("`{arg}` is not something this understands.\n{HOW}");
        }
    }

    let base_url = flag("--base-url").with_context(|| format!("say where: --base-url\n{HOW}"))?;
    let model = flag("--model").with_context(|| format!("say which model: --model\n{HOW}"))?;
    let provider = flag("--provider").unwrap_or_else(|| "openai-compat".into());
    let runs: usize = match flag("--runs") {
        Some(n) => n.parse().context("--runs is a number")?,
        None => 10,
    };
    let context: usize = match flag("--context") {
        Some(n) => n.parse().context("--context is a number of tokens")?,
        None => 131_072,
    };
    let api_key = match flag("--key-from") {
        Some(id) => Some(
            errand_core::keys::look_up(&id)
                .with_context(|| format!("Errand keeps no key under {id}"))?,
        ),
        None => None,
    };
    let only: Option<Vec<String>> =
        flag("--only").map(|s| s.split(',').map(|one| one.trim().to_string()).collect());
    let mut out = match flag("--out") {
        Some(path) => Some(
            std::fs::File::create(PathBuf::from(&path))
                .with_context(|| format!("writing {path}"))?,
        ),
        None => None,
    };

    let client = LlmClient::new(LlmSettings {
        provider,
        base_url: base_url.clone(),
        model: model.clone(),
        context_window: context,
        max_tokens: errand_core::local::room_for_an_answer(context),
        api_key,
        ..Default::default()
    });

    let chosen: Vec<_> = trial::scenarios()
        .into_iter()
        .filter(|s| only.as_ref().is_none_or(|o| o.iter().any(|id| id == s.id)))
        .collect();
    if chosen.is_empty() {
        bail!("no scenario is called that; --help lists them");
    }
    println!(
        "{model} at {base_url}: {} scenarios, {runs} runs each\n",
        chosen.len()
    );

    let mut tallies: BTreeMap<&str, Tally> = BTreeMap::new();
    let (mut tokens_in, mut tokens_out) = (0i64, 0i64);
    for scenario in &chosen {
        for n in 1..=runs {
            let run = trial::run(&client, scenario).await;
            let verdict = scenario.judge(&run);
            tokens_in += run.tokens_in;
            tokens_out += run.tokens_out;
            let word = match &verdict {
                Verdict::Held => "held".to_string(),
                Verdict::Failed(why) => format!("FAILED  {why}"),
                Verdict::Missed(why) => format!("missed  {why}"),
                Verdict::Broke(why) => format!("broke   {why}"),
            };
            println!("{:<20} {n:>2}/{runs}  {word}", scenario.id);
            if let Some(file) = out.as_mut() {
                let line = serde_json::json!({
                    "model": model,
                    "scenario": scenario.id,
                    "n": n,
                    "verdict": verdict,
                    "run": run,
                });
                writeln!(file, "{line}")?;
            }
            tallies.entry(scenario.id).or_default().add(&verdict);
        }
    }

    println!(
        "\n{:<20} {:>6} {:>6} {:>6} {:>6}",
        "", "failed", "held", "missed", "broke"
    );
    for scenario in &chosen {
        let t = &tallies[scenario.id];
        println!(
            "{:<20} {:>6} {:>6} {:>6} {:>6}   {}",
            scenario.id,
            t.failed,
            t.held,
            t.missed,
            t.broke,
            t.why.first().map(String::as_str).unwrap_or("")
        );
    }
    if tokens_in + tokens_out > 0 {
        println!("\n{tokens_in} tokens in, {tokens_out} out, where the server said.");
    }
    Ok(())
}
