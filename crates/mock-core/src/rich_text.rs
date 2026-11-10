use std::cell::RefCell;

use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use serde_json::Value;

const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_TOKENS: usize = 100_000;
const MAX_DEPTH: usize = 128;

#[derive(Default)]
struct TextState {
    text: String,
    elements: Vec<String>,
    unsupported: Option<String>,
    tokens: usize,
}

#[derive(Default)]
struct FormattingSink {
    state: RefCell<TextState>,
}

/// Elements that only lay visible text out: paragraphs, lists, headings and quotes.
fn block(name: &str) -> bool {
    matches!(name, "div" | "p" | "ul" | "ol" | "li" | "blockquote") || heading(name)
}

fn inline(name: &str) -> bool {
    matches!(
        name,
        "span" | "b" | "strong" | "i" | "em" | "u" | "code" | "a"
    )
}

fn heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

fn void(name: &str) -> bool {
    matches!(name, "br" | "hr")
}

/// A link to a web page shows its label; the page it cites is not a claim. Links that act on a
/// contact or run code (`tel:`, `mailto:`, `javascript:`) are not passive.
pub(crate) fn web_link(target: &str) -> bool {
    let target = target.trim().to_ascii_lowercase();
    target.starts_with("https://") || target.starts_with("http://")
}

/// Only a web link's own attributes are passive.
fn passive_attributes(tag: &html5ever::tokenizer::Tag) -> bool {
    tag.attrs.is_empty()
        || (&*tag.name == "a"
            && tag.kind == TagKind::StartTag
            && tag
                .attrs
                .iter()
                .all(|attribute| match &*attribute.name.local {
                    "href" => web_link(&attribute.value),
                    "target" | "rel" => true,
                    _ => false,
                }))
}

pub(crate) fn unsupported_control(character: char) -> bool {
    (character.is_control() && !matches!(character, '\t' | '\n' | '\r'))
        || matches!(
            character,
            '\u{061c}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
        )
}

impl TokenSink for FormattingSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        let mut state = self.state.borrow_mut();
        if state.unsupported.is_some() {
            return TokenSinkResult::Continue;
        }
        state.tokens += 1;
        if state.tokens > MAX_TOKENS {
            state.unsupported = Some("rich-text token budget exceeded".into());
            return TokenSinkResult::Continue;
        }
        match token {
            Token::CharacterTokens(text) => {
                if text.chars().any(unsupported_control) {
                    state.unsupported = Some(
                        "rich-text directional or invisible controls require adjudication".into(),
                    );
                } else {
                    state.text.push_str(&text);
                }
            }
            Token::CommentToken(_) => {}
            Token::TagToken(tag) => {
                let name = tag.name.as_ref();
                if !passive_attributes(&tag)
                    || tag.had_duplicate_attributes
                    || !(block(name) || inline(name) || void(name))
                {
                    state.unsupported =
                        Some("rich-text markup is outside the passive formatting contract".into());
                } else if void(name) {
                    if tag.kind == TagKind::StartTag {
                        state.text.push('\n');
                    } else {
                        state.unsupported =
                            Some("rich-text end tag requires unsupported HTML recovery".into());
                    }
                } else if tag.self_closing {
                    state.unsupported =
                        Some("non-void self-closing rich text requires adjudication".into());
                } else if tag.kind == TagKind::EndTag {
                    if state.elements.last().map(String::as_str) != Some(name) {
                        state.unsupported =
                            Some("rich-text nesting requires unsupported HTML recovery".into());
                    } else {
                        state.elements.pop();
                        if block(name) {
                            state.text.push('\n');
                        }
                    }
                } else if state.elements.len() >= MAX_DEPTH {
                    state.unsupported = Some("rich-text nesting budget exceeded".into());
                } else if block(name)
                    && state
                        .elements
                        .iter()
                        .any(|parent| parent == "p" || inline(parent) || heading(parent))
                {
                    state.unsupported =
                        Some("rich-text block nesting requires unsupported HTML recovery".into());
                } else {
                    if block(name) {
                        state.text.push('\n');
                    }
                    state.elements.push(name.to_owned());
                }
            }
            Token::EOFToken => {
                if !state.elements.is_empty() {
                    state.unsupported =
                        Some("unclosed rich-text markup requires adjudication".into());
                }
            }
            Token::ParseError(_) | Token::NullCharacterToken | Token::DoctypeToken(_) => {
                state.unsupported =
                    Some("rich-text syntax requires unsupported HTML recovery".into());
            }
        }
        TokenSinkResult::Continue
    }
}

fn canonical_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn passive_text(source: &str) -> Result<String, String> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("rich-text input budget exceeded".into());
    }
    let input = BufferQueue::default();
    input.push_back(source.into());
    let tokenizer = Tokenizer::new(
        FormattingSink::default(),
        TokenizerOpts {
            discard_bom: false,
            ..Default::default()
        },
    );
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    if !input.is_empty() {
        return Err("rich-text input was not completely tokenized".into());
    }
    let state = tokenizer.sink.state.borrow();
    if let Some(reason) = &state.unsupported {
        return Err(reason.clone());
    }
    Ok(canonical_text(&state.text))
}

pub(crate) fn equivalent(source: &str, expected: &str) -> Result<bool, String> {
    Ok(passive_text(source)? == canonical_text(expected))
}

pub(crate) fn zendesk_text(source: &str) -> Result<String, String> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("rich-text input budget exceeded".into());
    }
    let content = source
        .strip_prefix("<div class=\"zd-comment\" dir=\"auto\">")
        .and_then(|content| content.strip_suffix("</div>"))
        .unwrap_or(source);
    passive_text(content)
}

pub(crate) fn notion_text(source: &Value) -> Result<String, String> {
    let segments = source
        .as_array()
        .ok_or("Notion rich text must be a structured array")?;
    if segments.len() > MAX_TOKENS {
        return Err("Notion rich-text segment budget exceeded".into());
    }
    let mut text = String::new();
    for segment in segments {
        let object = segment
            .as_object()
            .ok_or("Notion rich-text segment is not an object")?;
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "type" | "text" | "plain_text" | "annotations" | "href"
            )
        }) || segment["type"].as_str() != Some("text")
        {
            return Err(
                "Notion mentions, equations or unknown metadata require adjudication".into(),
            );
        }
        let payload = segment["text"]
            .as_object()
            .ok_or("Notion text payload is missing")?;
        // A web link shows its label, so the label is content; the page it cites is not.
        let web = |target: &Value| target.as_str().is_some_and(web_link);
        if payload
            .keys()
            .any(|key| !matches!(key.as_str(), "content" | "link"))
            || payload.get("link").is_some_and(|link| {
                !link.is_null()
                    && !link
                        .as_object()
                        .is_some_and(|link| link.len() == 1 && link.get("url").is_some_and(web))
            })
            || segment
                .get("href")
                .is_some_and(|href| !href.is_null() && !web(href))
        {
            return Err(
                "Notion contact or script links and unknown text metadata require adjudication"
                    .into(),
            );
        }
        let content = segment["text"]["content"]
            .as_str()
            .ok_or("Notion text content is not a string")?;
        if segment
            .get("plain_text")
            .is_some_and(|value| value.as_str() != Some(content))
        {
            return Err("Notion plain_text disagrees with text.content".into());
        }
        if let Some(annotations) = segment.get("annotations") {
            let annotations = annotations
                .as_object()
                .ok_or("Notion annotations are not an object")?;
            for (name, value) in annotations {
                match name.as_str() {
                    "bold" | "italic" | "underline" | "code" | "strikethrough" => {
                        let enabled = value
                            .as_bool()
                            .ok_or("Notion annotation flag is not a boolean")?;
                        if name == "strikethrough" && enabled {
                            return Err("withdrawn Notion text requires adjudication".into());
                        }
                    }
                    "color" => {
                        let color = value.as_str().ok_or("Notion color is not a string")?;
                        let base = color.strip_suffix("_background").unwrap_or(color);
                        if color != "default"
                            && !matches!(
                                base,
                                "gray"
                                    | "brown"
                                    | "orange"
                                    | "yellow"
                                    | "green"
                                    | "blue"
                                    | "purple"
                                    | "pink"
                                    | "red"
                            )
                        {
                            return Err("unknown Notion color requires adjudication".into());
                        }
                    }
                    _ => return Err("unknown Notion annotation requires adjudication".into()),
                }
            }
        }
        if content.chars().any(unsupported_control) {
            return Err("Notion directional or invisible controls require adjudication".into());
        }
        if content.len() > MAX_INPUT_BYTES.saturating_sub(text.len()) {
            return Err("Notion rich-text input budget exceeded".into());
        }
        text.push_str(content);
    }
    Ok(canonical_text(&text))
}
