//! Presentation only: no guide interpretation, HTML execution or remote images.
use eframe::egui::{self, RichText};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::{hash::Hash, sync::Arc};

const MAX_BYTES: usize = 1024 * 1024;
const MAX_EVENTS: usize = 32_768;

#[derive(Clone, Default)]
struct Style {
    strong: usize,
    emphasis: usize,
    strike: usize,
    code: bool,
    heading: Option<HeadingLevel>,
    link: Option<String>,
}
struct Span {
    text: String,
    style: Style,
}
#[derive(Default)]
struct Block {
    spans: Vec<Span>,
    indent: usize,
    rule: bool,
}
struct Document {
    source: String,
    blocks: Vec<Block>,
    fallback: Option<&'static str>,
}

fn finish(block: &mut Block, blocks: &mut Vec<Block>) {
    if block.rule || !block.spans.is_empty() {
        blocks.push(std::mem::take(block));
    }
}

fn parse(source: &str) -> Document {
    let mut result = Document {
        source: source.into(),
        blocks: Vec::new(),
        fallback: None,
    };
    let mut style = Style::default();
    let mut block = Block::default();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut quotes = 0_usize;
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    for (count, event) in Parser::new_ext(source, options).enumerate() {
        let inline_code = matches!(&event, Event::Code(_));
        if count >= MAX_EVENTS {
            result.fallback = Some(
                "This content is too complex for rich display. The complete source is shown below.",
            );
            break;
        }
        match event {
            Event::Start(Tag::Paragraph) => {}
            Event::Start(Tag::Heading { level, .. }) => {
                finish(&mut block, &mut result.blocks);
                style.heading = Some(level);
            }
            Event::Start(Tag::BlockQuote(_)) => {
                finish(&mut block, &mut result.blocks);
                quotes += 1;
            }
            Event::Start(Tag::List(start)) => {
                finish(&mut block, &mut result.blocks);
                lists.push(start);
            }
            Event::Start(Tag::Item) => {
                finish(&mut block, &mut result.blocks);
                let text = match lists.last_mut() {
                    Some(Some(number)) => {
                        let text = format!("{number}. ");
                        *number = number.saturating_add(1);
                        text
                    }
                    _ => "• ".into(),
                };
                block.indent = lists.len().saturating_sub(1) + quotes;
                block.spans.push(Span {
                    text,
                    style: style.clone(),
                });
            }
            Event::Start(Tag::CodeBlock(_)) => {
                finish(&mut block, &mut result.blocks);
                style.code = true;
            }
            Event::Start(Tag::Emphasis) => style.emphasis += 1,
            Event::Start(Tag::Strong) => style.strong += 1,
            Event::Start(Tag::Strikethrough) => style.strike += 1,
            Event::Start(Tag::Link { dest_url, .. }) => {
                // Authored links are inert unless they are ordinary web URLs.
                // Opening requires an explicit click and exposes the destination.
                let url = reqwest::Url::parse(&dest_url).ok().filter(|url| {
                    matches!(url.scheme(), "http" | "https")
                        && url.host_str().is_some()
                        && url.username().is_empty()
                        && url.password().is_none()
                });
                if url.is_none() {
                    result.fallback = Some(
                        "This content contains a non-web or credential-bearing link. The complete source is shown as plain text.",
                    );
                    break;
                }
                style.link = url.map(|url| url.into());
            }
            Event::End(TagEnd::Emphasis) => style.emphasis = style.emphasis.saturating_sub(1),
            Event::End(TagEnd::Strong) => style.strong = style.strong.saturating_sub(1),
            Event::End(TagEnd::Strikethrough) => style.strike = style.strike.saturating_sub(1),
            Event::End(TagEnd::Link) => style.link = None,
            Event::End(TagEnd::Heading(_)) => {
                finish(&mut block, &mut result.blocks);
                style.heading = None;
            }
            Event::End(TagEnd::CodeBlock) => {
                finish(&mut block, &mut result.blocks);
                style.code = false;
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                finish(&mut block, &mut result.blocks);
                quotes = quotes.saturating_sub(1);
            }
            Event::End(TagEnd::List(_)) => {
                finish(&mut block, &mut result.blocks);
                lists.pop();
            }
            Event::End(TagEnd::Paragraph | TagEnd::Item) => finish(&mut block, &mut result.blocks),
            Event::Text(text) | Event::Code(text) => {
                let mut span_style = style.clone();
                span_style.code |= inline_code;
                if block.spans.is_empty() {
                    block.indent = lists.len() + quotes;
                }
                block.spans.push(Span {
                    text: text.into_string(),
                    style: span_style,
                });
            }
            Event::SoftBreak => block.spans.push(Span {
                text: " ".into(),
                style: style.clone(),
            }),
            Event::HardBreak => finish(&mut block, &mut result.blocks),
            Event::Rule => {
                finish(&mut block, &mut result.blocks);
                result.blocks.push(Block {
                    rule: true,
                    ..Block::default()
                });
            }
            _ => {
                result.fallback = Some(
                    "Images, HTML and other unsupported formatting are displayed as source text; nothing is fetched or executed.",
                );
                break;
            }
        }
    }
    finish(&mut block, &mut result.blocks);
    if result.fallback.is_some() {
        result.blocks.clear();
    }
    result
}

/// Stable caller identity keeps one cached document per visible content surface.
pub(super) fn show(ui: &mut egui::Ui, identity: impl Hash, source: &str) {
    ui.push_id(identity, |ui| {
        if ui.small_button("Copy source text").clicked() {
            ui.ctx().copy_text(source.to_owned());
        }
        if source.len() > MAX_BYTES {
            ui.label("Rich display is limited to 1 MiB. Use Copy source text to obtain the complete content.");
            return;
        }
        let key = ui.id().with("parsed-markdown");
        let document = ui.ctx().data_mut(|data| {
            let previous = data.get_temp::<Arc<Document>>(key);
            if let Some(document) = previous.filter(|value| value.source == source) {
                document
            } else {
                let document = Arc::new(parse(source));
                data.insert_temp(key, document.clone());
                document
            }
        });
        if let Some(reason) = document.fallback {
            ui.label(reason);
            ui.add(egui::Label::new(RichText::new(source).monospace()).wrap().selectable(true));
            return;
        }
        for (index, block) in document.blocks.iter().enumerate() {
            if block.rule {
                ui.separator();
                continue;
            }
            ui.push_id(index, |ui| {
                ui.horizontal_wrapped(|ui| {
                    // Inline runs carry their own whitespace; widget gaps would split words.
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space((block.indent.min(12) as f32) * 12.0);
                    for span in &block.spans {
                        let mut text = RichText::new(&span.text);
                        if span.style.strong > 0 { text = text.strong(); }
                        if span.style.emphasis > 0 { text = text.italics(); }
                        if span.style.strike > 0 { text = text.strikethrough(); }
                        if span.style.code { text = text.monospace(); }
                        if let Some(heading) = span.style.heading {
                            let size = match heading {
                                HeadingLevel::H1 => 26.0,
                                HeadingLevel::H2 => 23.0,
                                HeadingLevel::H3 => 20.0,
                                HeadingLevel::H4 => 18.0,
                                HeadingLevel::H5 => 16.0,
                                HeadingLevel::H6 => 14.0,
                            };
                            text = text.size(size).strong();
                        }
                        if let Some(url) = &span.style.link {
                            ui.hyperlink_to(text, url).on_hover_text(url);
                        } else {
                            ui.add(egui::Label::new(text).wrap().selectable(true));
                        }
                    }
                });
            });
            ui.add_space(4.0);
        }
        ui.collapsing("Source text", |ui| {
            ui.add(egui::Label::new(RichText::new(source).monospace()).wrap().selectable(true));
        });
    });
}
