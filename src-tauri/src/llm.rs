//! The AI step between transcription and typing.
//!
//! Two jobs:
//! * **Dictation** — the transcript mixes content with spoken commands ("scratch
//!   that", "new paragraph", "make this more formal"). The model applies the
//!   commands and returns only the text to type.
//! * **Command mode** — the user selects text and speaks an instruction; the
//!   model returns the replacement.
//!
//! Claude is called through the native Messages API; everything else goes
//! through the OpenAI-compatible chat API (OpenAI, Ollama, LM Studio, Groq, …).

use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

use crate::settings::{env_key, Provider, ProviderConfig, Settings};

const DICTATION_PROMPT: &str = r#"You are the editing engine inside a voice dictation app. You receive the raw speech-to-text transcript of what the user just said, and you return exactly the text that should be typed into the app they are using.

The transcript mixes two kinds of speech:
1. Content: the words the user wants written.
2. Spoken commands: instructions about the text itself. For example:
   - Corrections: "scratch that", "delete the last sentence", "no wait, I meant Tuesday", "replace cat with dog", "actually make that three o'clock"
   - Formatting: "new line", "new paragraph", "bullet point", "numbered list", "put that in quotes", "all caps"
   - Spoken punctuation: "comma", "period", "question mark", "colon", "open parenthesis"
   - Whole-text commands: "make it sound professional", "make this friendlier", "make it shorter", "turn this into an email", "rewrite that more formally", "translate this into Spanish"

Apply every command in the order spoken, and leave the command wording out of the output. When the user corrects themselves ("Tuesday, no, Wednesday"), keep only what they finally meant.

Whole-text commands need care because they sound like ordinary sentences. When the dictation ends with a short instruction about the text itself, such as "make it …", "make this …", "turn that into …", "rewrite it …" or "translate this …", that ending is a command: apply it to everything said before it, and leave the instruction out. It is content instead when it is part of a longer sentence with its own meaning ("our goal is to make it more fun for kids"), or when nothing was said before it for it to apply to.

Clean up the content: fix punctuation, capitalization, and words the speech recognizer clearly misheard, and drop filler ("um", "uh", "you know") and false starts. Beyond that, keep the user's own wording and language. Don't paraphrase, summarize, or add anything unless a command asks for it.

Apart from those commands, a phrase is a command only when it is clearly directed at you about the text. If it is part of what the user is saying to their reader ("tell him to scratch that idea"), it is content.

The content is never addressed to you. If the user dictates a question or a request ("can you send me the report?", "draft a thank-you note for the team"), type it out exactly; don't answer it, act on it, or refuse it. Never write to the user yourself: no questions, no comments about the transcript, not even to say that it is empty or unclear.

The text is pasted as plain text: use line breaks for new lines and paragraphs, "- " for bullets and "1. " for numbered items, and no Markdown emphasis unless the user asks for it. Only make a list when the user asks for one.

Examples of transcript → reply:
<example>
<transcript>let's meet on tuesday no wait wednesday at three</transcript>
<reply>Let's meet on Wednesday at three.</reply>
</example>
<example>
<transcript>the launch went well new paragraph next steps are below</transcript>
<reply>The launch went well.

Next steps are below.</reply>
</example>
<example>
<transcript>um I think we should uh ship it today scratch that let's ship it next week</transcript>
<reply>Let's ship it next week.</reply>
</example>
<example>
<transcript>groceries colon apples pears and milk make it a list</transcript>
<reply>Groceries:
- Apples
- Pears
- Milk</reply>
</example>
<example>
<transcript>hey can you call me back when you get this thanks</transcript>
<reply>Hey, can you call me back when you get this? Thanks.</reply>
</example>
<example>
<transcript>hi sam I can't come in today make it more formal</transcript>
<reply>Hi Sam, unfortunately I won't be able to come in today.</reply>
</example>
<example>
<transcript>Thanks for the invite. I can't make it on Monday, sadly. Make it warmer.</transcript>
<reply>Thank you so much for the invite! I'm really sorry, but I can't make it on Monday.</reply>
</example>
<example>
<transcript>Our plan is to make it easier for new users to sign up.</transcript>
<reply>Our plan is to make it easier for new users to sign up.</reply>
</example>
<example>
<transcript>so I wanted to ask about the um forget it scratch all of that</transcript>
<reply></reply>
</example>

Be terse. Your entire reply is typed into the user's app, so every extra word you add ends up in their document. Reply with only the final text: no preamble, no "Here is", no quotes or tags around it, no notes or explanation afterwards. Never add words the user didn't say unless a command asks for them, and never drop words they meant to keep. If nothing should be typed (for example, the user cancelled everything they said), reply with nothing at all: an empty reply, not a note saying so."#;

const COMMAND_PROMPT: &str = r#"You are the editing engine inside a voice-driven writing assistant. The user selected some text in the app they are using (the selection may be empty) and spoke an instruction. You return exactly the text that should replace the selection.

- With a selection: apply the instruction to it (rewrite, shorten, translate, fix, reformat, continue, reply to it, and so on) and return the complete replacement text.
- Without a selection: the instruction asks you to write something new; write it.
- If the instruction is a question about the selection rather than a change to it, return the original selection unchanged, followed by a blank line and your answer, so nothing the user wrote is lost.

Match the language, tone, and formatting of the selection unless told otherwise. The result is pasted as plain text, so don't use Markdown unless the selection already does or the user asks for it.

Be terse. Your entire reply is pasted into the user's app. Reply with only the replacement text: no preamble, no "Here is", no quotes around it, no notes or explanation afterwards. When you write new text or answer a question, keep it as short as the request allows; don't pad it with greetings, caveats, or offers of further help."#;

/// Prompt additions shared by both modes, built from the user's settings.
fn personalization(settings: &Settings) -> String {
    let mut out = String::new();
    let word = settings.command_word.trim();
    if !word.is_empty() {
        out.push_str(&format!(
            "\n\nThe user has set the wake word \"{word}\". Treat speech as a command only when it begins with \"{word}\" (for example \"{word}, new paragraph\"). Everything else is content, even if it sounds like an instruction; spoken punctuation and self-corrections still apply. Leave the wake word out of the output."
        ));
    }
    let commands: Vec<String> = settings
        .voice_commands
        .iter()
        .filter(|c| !c.phrase.trim().is_empty() && !c.action.trim().is_empty())
        .map(|c| format!("- \"{}\": {}", c.phrase.trim(), c.action.trim()))
        .collect();
    if !commands.is_empty() {
        out.push_str("\n\nThe user has defined these custom voice commands. A trigger counts only when it is spoken as a command on its own, usually at the end; the same words inside an ordinary sentence are content. Never add anything from this list unless its trigger was spoken as a command:\n");
        out.push_str(&commands.join("\n"));
    }
    if !settings.vocabulary.is_empty() {
        out.push_str(&format!(
            "\n\nSpell these names and terms exactly like this when they come up (the recognizer may have misheard them): {}",
            settings.vocabulary.join(", ")
        ));
    }
    let style = settings.custom_instructions.trim();
    if !style.is_empty() {
        out.push_str(&format!("\n\nThe user's own preferences:\n{style}"));
    }
    out
}

/// The (system, user) messages for one dictation.
fn dictation_messages(settings: &Settings, transcript: &str) -> (String, String) {
    (
        format!("{DICTATION_PROMPT}{}", personalization(settings)),
        format!("<transcript>\n{transcript}\n</transcript>"),
    )
}

pub async fn polish_dictation(settings: &Settings, transcript: &str) -> Result<String> {
    let (system, user) = dictation_messages(settings, transcript);
    let out = complete(settings, &system, &user).await?;
    Ok(guard_meta_reply(transcript, out))
}

/// Models occasionally talk *about* the transcript ("No text to type.", "No
/// transcript was provided…") instead of returning text. That must never be
/// pasted: type nothing if the user cancelled what they said, otherwise their raw words.
fn guard_meta_reply(transcript: &str, out: String) -> String {
    const META: [&str; 7] = [
        "transcript", "dictat", "no text", "nothing to type", "nothing to output", "empty reply", "empty message",
    ];
    const CANCEL: [&str; 9] = [
        "scratch that", "scratch all", "never mind", "nevermind", "forget it", "forget all", "forget that",
        "delete that", "cancel that",
    ];
    let lower = out.to_lowercase();
    let said = transcript.to_lowercase();
    // A word the user actually said can't be a sign of a meta reply.
    if !META.iter().any(|m| lower.contains(m) && !said.contains(m)) {
        return out;
    }
    log::warn!("AI replied about the transcript instead of editing it: {out:?}");
    if CANCEL.iter().any(|c| said.contains(c)) {
        String::new()
    } else {
        transcript.trim().to_string()
    }
}

pub async fn run_command(settings: &Settings, instruction: &str, selection: &str) -> Result<String> {
    let system = format!("{COMMAND_PROMPT}{}", personalization(settings));
    let user = format!("<selection>\n{selection}\n</selection>\n\n<instruction>\n{instruction}\n</instruction>");
    complete(settings, &system, &user).await
}

async fn complete(settings: &Settings, system: &str, user: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(90))
        .build()?;
    if settings.provider == Provider::Local {
        // Uses Lipwise's own llama-server only if it's running; starting it is the user's call.
        let local = ProviderConfig {
            base_url: crate::local_ai::running_url(&settings.local_ai).await?,
            model: "lipwise-local".into(),
            api_key: String::new(),
        };
        let text = openai_compatible(&client, Provider::Local, &local, "", system, user).await?;
        return Ok(clean_output(&text));
    }

    let cfg = settings.active_provider();
    let key = if cfg.api_key.trim().is_empty() {
        env_key(settings.provider).unwrap_or_default()
    } else {
        cfg.api_key.trim().to_string()
    };
    let text = match settings.provider {
        Provider::Anthropic => anthropic(&client, cfg, &key, system, user).await?,
        _ => openai_compatible(&client, settings.provider, cfg, &key, system, user).await?,
    };
    Ok(clean_output(&text))
}

/// Claude via the Messages API.
async fn anthropic(client: &reqwest::Client, cfg: &ProviderConfig, key: &str, system: &str, user: &str) -> Result<String> {
    if key.is_empty() {
        bail!("Add your Anthropic API key in the AI settings");
    }
    let model = cfg.model.trim();
    let mut body = json!({
        "model": model,
        "max_tokens": 16000,
        "system": system,
        "messages": [{ "role": "user", "content": user }],
    });
    // Dictation cleanup is a light task where latency matters: keep effort low.
    if supports_effort(model) {
        body["output_config"] = json!({ "effort": "low" });
    }
    let mut req = client
        .post(format!("{}/v1/messages", base_url(cfg, "https://api.anthropic.com")))
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01");
    // If a safety classifier declines, let the API retry on its recommended fallback model.
    if supports_server_fallback(model) {
        body["fallbacks"] = json!("default");
        req = req.header("anthropic-beta", "server-side-fallback-2026-07-01");
    }

    let resp: Value = send(req.json(&body)).await?;
    if resp["stop_reason"] == "refusal" {
        bail!("The model declined this request");
    }
    let text = resp["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<String>()
        })
        .unwrap_or_default();
    Ok(text)
}

/// Any OpenAI-compatible `/chat/completions` endpoint.
async fn openai_compatible(
    client: &reqwest::Client,
    provider: Provider,
    cfg: &ProviderConfig,
    key: &str,
    system: &str,
    user: &str,
) -> Result<String> {
    if cfg.model.trim().is_empty() {
        bail!("Choose a model in the AI settings");
    }
    let mut body = json!({
        "model": cfg.model.trim(),
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
    });
    // Local models default to fairly creative sampling; editing wants it low.
    // (OpenAI's reasoning models reject any non-default temperature, so leave theirs alone.)
    if matches!(provider, Provider::Local | Provider::Ollama | Provider::Custom) {
        body["temperature"] = json!(0.2);
        // Hybrid reasoning models (Qwen3 and similar) think by default. This is a
        // latency-bound edit, not a reasoning task: turn thinking off via the chat
        // template (honoured by vLLM, SGLang and llama.cpp's server).
        body["chat_template_kwargs"] = json!({ "enable_thinking": false });
    }
    let mut req = client.post(format!("{}/chat/completions", base_url(cfg, "")));
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let resp: Value = send(req.json(&body)).await?;
    resp["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("Unexpected response from the AI provider"))
}

/// Lists model ids the configured provider offers, for the settings dropdown.
pub async fn list_models(provider: Provider, cfg: &ProviderConfig) -> Result<Vec<String>> {
    let key = if cfg.api_key.trim().is_empty() {
        env_key(provider).unwrap_or_default()
    } else {
        cfg.api_key.trim().to_string()
    };
    let client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?;
    let req = match provider {
        Provider::Anthropic => client
            .get(format!("{}/v1/models?limit=100", base_url(cfg, "https://api.anthropic.com")))
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
        _ => {
            let req = client.get(format!("{}/models", base_url(cfg, "")));
            if key.is_empty() { req } else { req.bearer_auth(key) }
        }
    };
    let resp: Value = send(req).await?;
    let mut ids: Vec<String> = resp["data"]
        .as_array()
        .map(|items| items.iter().filter_map(|m| m["id"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    if provider != Provider::Anthropic {
        ids.sort();
    }
    Ok(ids)
}

async fn send(req: reqwest::RequestBuilder) -> Result<Value> {
    let resp = req.send().await.map_err(|e| {
        if e.is_connect() {
            anyhow!("Couldn't reach the AI provider. Is it running and is the URL right?")
        } else if e.is_timeout() {
            anyhow!("The AI provider took too long to respond")
        } else {
            anyhow!("AI request failed: {e}")
        }
    })?;
    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let detail = body["error"]["message"]
            .as_str()
            .or_else(|| body["error"].as_str())
            .unwrap_or("no details");
        bail!("AI provider returned {status}: {detail}");
    }
    Ok(body)
}

fn base_url(cfg: &ProviderConfig, default: &str) -> String {
    let url = cfg.base_url.trim();
    let url = if url.is_empty() { default } else { url };
    url.trim_end_matches('/').to_string()
}

/// Models that accept `output_config.effort`.
fn supports_effort(model: &str) -> bool {
    [
        "claude-opus-5", "claude-sonnet-5", "claude-fable-5", "claude-mythos-5",
        "claude-opus-4-5", "claude-opus-4-6", "claude-opus-4-7", "claude-opus-4-8", "claude-sonnet-4-6",
    ]
    .iter()
    .any(|p| model.starts_with(p))
}

/// Models that accept server-side `fallbacks: "default"`.
fn supports_server_fallback(model: &str) -> bool {
    ["claude-opus-5", "claude-sonnet-5-5", "claude-fable-5-1"]
        .iter()
        .any(|p| model.starts_with(p))
}

/// Strips artifacts some models add despite instructions: reasoning tags from
/// local "thinking" models, and wrapping tags or quotes echoed from the prompt.
fn clean_output(text: &str) -> String {
    let mut s = text.to_string();
    while let (Some(start), Some(end)) = (s.find("<think>"), s.find("</think>")) {
        if end < start {
            break;
        }
        s.replace_range(start..end + "</think>".len(), "");
    }
    let mut s = s.trim();
    for tag in ["reply", "transcript", "selection", "text"] {
        let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
        if let Some(inner) = s.strip_prefix(open.as_str()).and_then(|r| r.strip_suffix(close.as_str())) {
            s = inner.trim();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_think_blocks_and_echoed_tags() {
        assert_eq!(clean_output("<think>hmm</think>\nHello there."), "Hello there.");
        assert_eq!(clean_output("<transcript>\nHi\n</transcript>"), "Hi");
        assert_eq!(clean_output("  plain  "), "plain");
    }

    #[derive(serde::Deserialize)]
    struct EvalFile {
        dictation: Vec<EvalCase>,
        command: Vec<EvalCase>,
    }

    #[derive(serde::Deserialize)]
    struct EvalCase {
        id: String,
        category: String,
        #[serde(default)]
        said: String,
        #[serde(default)]
        instruction: String,
        #[serde(default)]
        selection: String,
        #[serde(default)]
        contains: Vec<String>,
        #[serde(default)]
        contains_any: Vec<String>,
        #[serde(default)]
        absent: Vec<String>,
        max_words: Option<usize>,
    }

    /// Returns the reasons `out` fails the case's checks (empty = pass).
    fn grade(case: &EvalCase, out: &str) -> Vec<String> {
        let lower = out.to_lowercase();
        let mut problems = Vec::new();
        for c in &case.contains {
            if !lower.contains(&c.to_lowercase()) {
                problems.push(format!("missing {c:?}"));
            }
        }
        if !case.contains_any.is_empty() && !case.contains_any.iter().any(|c| lower.contains(&c.to_lowercase())) {
            problems.push(format!("missing one of {:?}", case.contains_any));
        }
        for a in &case.absent {
            if lower.contains(&a.to_lowercase()) {
                problems.push(format!("contains {a:?}"));
            }
        }
        if let Some(max) = case.max_words {
            let words = out.split_whitespace().count();
            if words > max {
                problems.push(format!("{words} words (max {max})"));
            }
        }
        problems
    }

    /// Scores the prompts against evals/dictation.json on a live model. Opt-in:
    ///   LIPWISE_EVAL_PROVIDER=custom LIPWISE_EVAL_BASE_URL=http://127.0.0.1:18080/v1 \
    ///   LIPWISE_EVAL_MODEL=x LIPWISE_EVAL_RUNS=3 cargo test --lib live_eval -- --ignored --nocapture
    /// LIPWISE_EVAL_ONLY=id1,id2 limits the run to some cases.
    #[test]
    #[ignore]
    fn live_eval() {
        let env = |k: &str| std::env::var(k).ok();
        let provider: Provider =
            serde_json::from_value(json!(env("LIPWISE_EVAL_PROVIDER").unwrap_or("custom".into()))).unwrap();
        let mut settings = Settings { provider, ..Settings::default() };
        let cfg = match provider {
            Provider::Anthropic => &mut settings.providers.anthropic,
            Provider::OpenAI => &mut settings.providers.openai,
            Provider::Ollama => &mut settings.providers.ollama,
            Provider::Custom | Provider::Local => &mut settings.providers.custom,
        };
        if let Some(model) = env("LIPWISE_EVAL_MODEL") {
            cfg.model = model;
        }
        if let Some(url) = env("LIPWISE_EVAL_BASE_URL") {
            cfg.base_url = url;
        }
        let runs: usize = env("LIPWISE_EVAL_RUNS").and_then(|r| r.parse().ok()).unwrap_or(3);
        let only: Option<Vec<String>> = env("LIPWISE_EVAL_ONLY").map(|s| s.split(',').map(str::to_string).collect());

        let file: EvalFile = serde_json::from_str(include_str!("../evals/dictation.json")).unwrap();
        let cases = file
            .dictation
            .iter()
            .map(|c| (false, c))
            .chain(file.command.iter().map(|c| (true, c)))
            .filter(|(_, c)| only.as_ref().is_none_or(|o| o.contains(&c.id)));

        let mut by_category: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
        let (mut passed, mut total, mut guard_hits) = (0, 0, 0);
        let started = std::time::Instant::now();
        tauri::async_runtime::block_on(async {
            for (is_command, case) in cases {
                let mut case_passes = 0;
                let mut failures = Vec::new();
                for _ in 0..runs {
                    let out = if is_command {
                        run_command(&settings, &case.instruction, &case.selection).await
                    } else {
                        // polish_dictation, unrolled so we can count safety-net saves.
                        let (system, user) = dictation_messages(&settings, &case.said);
                        complete(&settings, &system, &user).await.map(|raw| {
                            let guarded = guard_meta_reply(&case.said, raw.clone());
                            if guarded != raw {
                                guard_hits += 1;
                                println!("       (safety net replaced {raw:?})");
                            }
                            guarded
                        })
                    }
                    .unwrap_or_else(|e| format!("<error: {e}>"));
                    let problems = grade(case, &out);
                    if problems.is_empty() {
                        case_passes += 1;
                    } else {
                        failures.push((out, problems));
                    }
                }
                passed += case_passes;
                total += runs;
                let entry = by_category.entry(case.category.clone()).or_default();
                entry.0 += case_passes;
                entry.1 += runs;
                println!("{} {:<28} {case_passes}/{runs}", if case_passes == runs { "PASS" } else { "FAIL" }, case.id);
                for (out, problems) in failures.iter().take(1) {
                    println!("       {}", problems.join("; "));
                    println!("       -> {:?}", out);
                }
            }
        });
        println!("\nby category:");
        for (category, (p, t)) in &by_category {
            println!("  {category:<38} {p}/{t}");
        }
        println!(
            "\nTOTAL {passed}/{total} ({:.0}%) in {:.1}s, safety net used {guard_hits}x",
            100.0 * passed as f64 / total.max(1) as f64,
            started.elapsed().as_secs_f64()
        );
    }

    #[test]
    fn meta_replies_are_never_pasted() {
        // Cancelled dictation + a note about it -> type nothing.
        assert_eq!(guard_meta_reply("um never mind all of that", "No text to type.".into()), "");
        // Real content + a confused note -> fall back to the user's own words.
        assert_eq!(
            guard_meta_reply("We need to make it sound professional.", "No transcript content was provided.".into()),
            "We need to make it sound professional."
        );
        // Ordinary output, and output echoing words the user said, pass through.
        assert_eq!(guard_meta_reply("see you at three", "See you at three.".into()), "See you at three.");
        assert_eq!(
            guard_meta_reply("there's no text from him yet", "There's no text from him yet.".into()),
            "There's no text from him yet."
        );
    }

    #[test]
    fn model_feature_gates() {
        assert!(supports_effort("claude-opus-5-5"));
        assert!(!supports_effort("claude-haiku-4-5"));
        assert!(supports_server_fallback("claude-opus-5-5"));
        assert!(!supports_server_fallback("claude-sonnet-5"));
        assert!(!supports_server_fallback("claude-haiku-4-5"));
    }
}
