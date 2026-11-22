use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct AdfMark {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub attrs: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct AdfNode {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: Option<String>,
    #[serde(default)]
    pub content: Vec<AdfNode>,
    #[serde(default)]
    pub attrs: Option<Value>,
    #[serde(default)]
    pub marks: Vec<AdfMark>,
}

impl AdfNode {
    fn attr(&self, key: &str) -> Option<&Value> {
        self.attrs.as_ref()?.get(key)
    }
}

fn prefix_lines(text: &str, first: &str, rest: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, line)| {
            let prefix = if i == 0 { first } else { rest };
            format!("{prefix}{line}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn list_item(marker: &str, item: &AdfNode) -> String {
    let body = item
        .content
        .iter()
        .map(block_to_md)
        .collect::<Vec<_>>()
        .join("\n");
    let indent = " ".repeat(marker.len());
    prefix_lines(&body, marker, &indent)
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '~') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn text_with_marks(text: &str, marks: &[AdfMark]) -> String {
    let core = text.trim();
    if marks.is_empty() || core.is_empty() {
        return escape(text);
    }
    let lead = &text[..text.len() - text.trim_start().len()];
    let trail = &text[text.trim_end().len()..];
    let text = core;

    let is_code = marks.iter().any(|m| m.kind == "code");
    let mut out = if is_code {
        format!("`{text}`")
    } else {
        escape(text)
    };

    for mark in marks {
        out = match mark.kind.as_str() {
            "strong" => format!("**{out}**"),
            "em" => format!("*{out}*"),
            "strike" => format!("~~{out}~~"),
            _ => out,
        };
    }

    if let Some(href) = marks
        .iter()
        .find(|m| m.kind == "link")
        .and_then(|m| m.attrs.as_ref()?.get("href")?.as_str())
    {
        out = format!("[{out}]({href})");
    }
    format!("{lead}{out}{trail}")
}

fn inline_to_md(nodes: &[AdfNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node.kind.as_str() {
            "text" => {
                let text = node.text.as_deref().unwrap_or_default();
                out.push_str(&text_with_marks(text, &node.marks));
            }
            "hardBreak" => out.push('\n'),
            "mention" => {
                let name = node
                    .attr("text")
                    .and_then(Value::as_str)
                    .unwrap_or("@unknown");
                let id = node.attr("id").and_then(Value::as_str).unwrap_or_default();
                out.push_str(&format!("[{name}](mention:{id})"));
            }
            _ => out.push_str(&inline_to_md(&node.content)),
        }
    }
    out
}

fn block_to_md(node: &AdfNode) -> String {
    match node.kind.as_str() {
        "paragraph" => inline_to_md(&node.content),
        "heading" => {
            let level = node.attr("level").and_then(Value::as_u64).unwrap_or(1) as usize;
            format!("{} {}", "#".repeat(level), inline_to_md(&node.content))
        }
        "bulletList" => node
            .content
            .iter()
            .map(|item| list_item("- ", item))
            .collect::<Vec<_>>()
            .join("\n"),
        "orderedList" => {
            let start = node.attr("order").and_then(Value::as_u64).unwrap_or(1);
            node.content
                .iter()
                .enumerate()
                .map(|(i, item)| list_item(&format!("{}. ", start + i as u64), item))
                .collect::<Vec<_>>()
                .join("\n")
        }
        "blockquote" => prefix_lines(&blocks_to_md(&node.content), "> ", "> "),
        "codeBlock" => {
            let language = node
                .attr("language")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let code: String = node
                .content
                .iter()
                .filter_map(|child| child.text.as_deref())
                .collect();
            format!("```{language}\n{code}\n```")
        }
        "rule" => "---".into(),
        _ => blocks_to_md(&node.content),
    }
}

fn blocks_to_md(nodes: &[AdfNode]) -> String {
    nodes
        .iter()
        .map(block_to_md)
        .filter(|block| !block.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn to_markdown(doc: &AdfNode) -> String {
    blocks_to_md(&doc.content)
}

enum Frame {
    Node {
        node: Value,
        implicit: bool,
    },
    Mark(Value),
    Code {
        language: Option<String>,
        text: String,
    },
    Mention {
        id: String,
        text: String,
    },
    InlineCard {
        url: String,
    },
    Skip,
}

fn node(kind: &str) -> Value {
    json!({"type": kind, "content": []})
}

struct Builder {
    stack: Vec<Frame>,
}

impl Builder {
    fn new() -> Self {
        Self {
            stack: vec![Frame::Node {
                node: node("doc"),
                implicit: false,
            }],
        }
    }

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(_) => self.end(),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.inline_text(&code, Some(json!({"type": "code"}))),
            Event::SoftBreak | Event::HardBreak => self.inline(json!({"type": "hardBreak"})),
            Event::Rule => {
                self.close_implicit();
                self.append(json!({"type": "rule"}))
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        let frame = match tag {
            Tag::Paragraph => self.block(node("paragraph")),
            Tag::Heading { level, .. } => {
                let mut heading = node("heading");
                heading["attrs"] = json!({"level": level as u8});
                self.block(heading)
            }
            Tag::List(None) => self.block(node("bulletList")),
            Tag::List(Some(start)) => {
                let mut list = node("orderedList");
                list["attrs"] = json!({"order": start});
                self.block(list)
            }
            Tag::Item => {
                self.close_implicit();
                self.stack.push(Frame::Node {
                    node: node("listItem"),
                    implicit: false,
                });
                Frame::Node {
                    node: node("paragraph"),
                    implicit: true,
                }
            }
            Tag::CodeBlock(kind) => {
                self.close_implicit();
                let language = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                    _ => None,
                };
                Frame::Code {
                    language,
                    text: String::new(),
                }
            }
            Tag::Strong => Frame::Mark(json!({"type": "strong"})),
            Tag::Emphasis => Frame::Mark(json!({"type": "em"})),
            Tag::Strikethrough => Frame::Mark(json!({"type": "strike"})),
            Tag::Link {
                link_type,
                dest_url,
                ..
            } => {
                if let Some(id) = dest_url.strip_prefix("mention:") {
                    Frame::Mention {
                        id: id.to_string(),
                        text: String::new(),
                    }
                } else if link_type == LinkType::Autolink {
                    Frame::InlineCard {
                        url: dest_url.to_string(),
                    }
                } else {
                    Frame::Mark(json!({"type": "link", "attrs": {"href": &dest_url.to_string()}}))
                }
            }
            Tag::BlockQuote(_) => self.block(node("blockquote")),
            _ => Frame::Skip,
        };
        self.stack.push(frame)
    }

    fn block(&mut self, node: Value) -> Frame {
        self.close_implicit();
        Frame::Node {
            node,
            implicit: false,
        }
    }

    fn close_implicit(&mut self) {
        if matches!(self.stack.last(), Some(Frame::Node { implicit: true, .. }))
            && let Some(Frame::Node { node, .. }) = self.stack.pop()
        {
            let has_content = node["content"].as_array().is_some_and(|c| !c.is_empty());
            if has_content {
                self.append(node);
            }
        }
    }

    fn end(&mut self) {
        self.close_implicit();
        if self.stack.len() > 1
            && let Some(frame) = self.stack.pop()
        {
            self.finish_frame(frame);
        }
    }

    fn finish_frame(&mut self, frame: Frame) {
        match frame {
            Frame::Node { node, .. } => self.append(node),
            Frame::Code { language, text } => {
                let code = text.trim_end_matches('\n');
                let mut block =
                    json!({"type": "codeBlock", "content": [{"type": "text", "text": code}]});
                if let Some(language) = language {
                    block["attrs"] = json!({"language": language});
                }
                self.append(block);
            }
            Frame::Mention { id, text } => {
                self.inline(json!({"type": "mention", "attrs": {"id": id, "text": text}}));
            }
            Frame::InlineCard { url } => {
                self.inline(json!({"type": "inlineCard", "attrs": {"url": url}}));
            }
            Frame::Mark(_) | Frame::Skip => {}
        }
    }

    fn text(&mut self, text: &str) {
        match self.stack.last_mut() {
            Some(Frame::Code { text: code, .. }) => code.push_str(text),
            Some(Frame::Mention { text: name, .. }) => name.push_str(text),
            Some(Frame::InlineCard { .. }) => {}
            _ => self.inline_text(text, None),
        }
    }

    fn inline_text(&mut self, text: &str, extra_mark: Option<Value>) {
        let mut marks: Vec<Value> = self
            .stack
            .iter()
            .filter_map(|frame| match frame {
                Frame::Mark(mark) => Some(mark.clone()),
                _ => None,
            })
            .collect();
        marks.extend(extra_mark);

        let mut node = json!({"type": "text", "text": text});
        if !marks.is_empty() {
            node["marks"] = Value::Array(marks);
        }
        self.inline(node);
    }

    fn inline(&mut self, node: Value) {
        let content = self.content_mut();
        if let Some(last) = content.last_mut() {
            let both_text = last["type"] == "text" && node["type"] == "text";
            if both_text && last.get("marks") == node.get("marks") {
                let joined = format!(
                    "{}{}",
                    last["text"].as_str().unwrap_or_default(),
                    node["text"].as_str().unwrap_or_default()
                );
                last["text"] = Value::String(joined);
                return;
            }
        }
        content.push(node);
    }

    fn append(&mut self, node: Value) {
        self.content_mut().push(node);
    }

    fn content_mut(&mut self) -> &mut Vec<Value> {
        for frame in self.stack.iter_mut().rev() {
            if let Frame::Node { node, .. } = frame {
                return node["content"]
                    .as_array_mut()
                    .expect("nodes are created with content");
            }
        }
        unreachable!("the doc node is always at the bottom of the stack")
    }

    fn finish(mut self) -> Value {
        while self.stack.len() > 1 {
            self.end();
        }
        match self.stack.pop() {
            Some(Frame::Node { mut node, .. }) => {
                node["version"] = json!(1);
                node
            }
            _ => json!({"type": "doc", "version": 1, "content": []}),
        }
    }
}

pub fn from_markdown(markdown: &str) -> Value {
    let mut builder = Builder::new();
    for event in Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH) {
        builder.handle(event);
    }
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_md(doc: Value) -> String {
        let doc: AdfNode = serde_json::from_value(doc).expect("adf doc parses");
        to_markdown(&doc)
    }

    fn to_adf(md: &str) -> Value {
        from_markdown(md)
    }

    fn round_trip(md: &str) -> String {
        to_md(from_markdown(md))
    }

    #[test]
    fn a_markdown_doc_builds_blocks_in_order() {
        assert_eq!(
            to_adf("# Maple\n\n## Maplesub\n\nmaplepara\n\n- maple\n    - maplebush\n\n---"),
            json!({"type": "doc", "version": 1, "content": [
                {"type": "heading", "attrs": {"level": 1}, "content": [{"type": "text", "text": "Maple"}]},
                {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Maplesub"}]},
                {"type": "paragraph", "content": [{"type": "text", "text": "maplepara"}]},
                {"type": "bulletList", "content": [
                    {"type": "listItem", "content": [
                        {"type": "paragraph", "content": [{"type": "text", "text": "maple"}]},
                        {"type": "bulletList", "content": [
                            {"type": "listItem", "content": [
                                {"type": "paragraph", "content": [{"type": "text", "text": "maplebush"}]}
                            ]}
                        ]}
                    ]}
                ]},
                {"type": "rule"}
            ]})
        );
    }

    #[test]
    fn an_ordered_list_keeps_its_start_number() {
        assert_eq!(
            to_adf("3. Maple\n4. Mapleberry"),
            json!({"type": "doc", "version": 1, "content": [
                {"type": "orderedList", "attrs": {"order": 3}, "content": [
                    {"type": "listItem", "content": [
                        {"type": "paragraph", "content": [{"type": "text", "text": "Maple"}]}
                    ]},
                    {"type": "listItem", "content": [
                        {"type": "paragraph", "content": [{"type": "text", "text": "Mapleberry"}]}
                    ]}
                ]}
            ]})
        );
    }

    #[test]
    fn inline_marks_nest_as_adf_marks() {
        assert_eq!(
            to_adf("**maple** ~~maplegone~~ `maple()` [maples](https://maple.example) ***maple***"),
            json!({"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [
                {"type": "text", "text": "maple", "marks": [{"type": "strong"}]},
                {"type": "text", "text": " "},
                {"type": "text", "text": "maplegone", "marks": [{"type": "strike"}]},
                {"type": "text", "text": " "},
                {"type": "text", "text": "maple()", "marks": [{"type": "code"}]},
                {"type": "text", "text": " "},
                {"type": "text", "text": "maples", "marks": [
                    {"type": "link", "attrs": {"href": "https://maple.example"}}
                ]},
                {"type": "text", "text": " "},
                // marks stack outermost first
                {"type": "text", "text": "maple", "marks": [{"type": "em"}, {"type": "strong"}]}
            ]}]})
        );
    }

    #[test]
    fn a_soft_line_break_becomes_a_hard_break_node() {
        assert_eq!(
            to_adf("maple\nmaplebush"),
            json!({"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [
                {"type": "text", "text": "maple"},
                {"type": "hardBreak"},
                {"type": "text", "text": "maplebush"}
            ]}]})
        );
    }

    #[test]
    fn a_fenced_code_block_records_its_language() {
        assert_eq!(
            to_adf("```rust\nfn main() {}\n```"),
            json!({"type": "doc", "version": 1, "content": [
                {"type": "codeBlock", "attrs": {"language": "rust"}, "content": [
                    {"type": "text", "text": "fn main() {}"}
                ]}
            ]})
        );
    }

    #[test]
    fn an_indented_code_block_has_no_language() {
        assert_eq!(
            to_adf("    maplesyrup()\n"),
            json!({"type": "doc", "version": 1, "content": [
                {"type": "codeBlock", "content": [{"type": "text", "text": "maplesyrup()"}]}
            ]})
        );
    }

    #[test]
    fn a_mention_link_becomes_a_mention_node() {
        assert_eq!(
            to_adf("ask [@Maple](mention:maple-123)"),
            json!({"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [
                {"type": "text", "text": "ask "},
                {"type": "mention", "attrs": {"id": "maple-123", "text": "@Maple"}}
            ]}]})
        );
    }

    #[test]
    fn an_autolink_becomes_an_inline_card() {
        assert_eq!(
            to_adf("<https://maple.example>"),
            json!({"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [
                {"type": "inlineCard", "attrs": {"url": "https://maple.example"}}
            ]}]})
        );
    }

    #[test]
    fn a_blockquote_wraps_its_paragraphs() {
        assert_eq!(
            to_adf("> maple\n>\n> maplebush"),
            json!({"type": "doc", "version": 1, "content": [{"type": "blockquote", "content": [
                {"type": "paragraph", "content": [{"type": "text", "text": "maple"}]},
                {"type": "paragraph", "content": [{"type": "text", "text": "maplebush"}]}
            ]}]})
        );
    }

    #[test]
    fn an_image_keeps_its_alt_text_and_drops_the_url() {
        assert_eq!(
            to_adf("See ![maple diagram](https://maple.example/d.png)."),
            json!({"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [
                {"type": "text", "text": "See maple diagram."}
            ]}]})
        );
    }

    #[test]
    fn a_table_stays_literal_text_because_tables_are_not_enabled() {
        assert_eq!(
            to_adf("| a | b |\n|---|---|\n| 1 | 2 |"),
            json!({"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": [
                {"type": "text", "text": "| a | b |"},
                {"type": "hardBreak"},
                {"type": "text", "text": "|---|---|"},
                {"type": "hardBreak"},
                {"type": "text", "text": "| 1 | 2 |"}
            ]}]})
        );
    }

    #[test]
    fn raw_html_and_empty_input_both_yield_an_empty_doc() {
        let empty = json!({"type": "doc", "version": 1, "content": []});
        assert_eq!(to_adf("<div>raw maple html</div>"), empty);
        assert_eq!(to_adf(""), empty);
    }

    #[test]
    fn markdown_survives_a_round_trip_through_adf() {
        let md = "# Maple\n\nsoft\nbreak with **maple**, ~~maplegone~~, `maple` and [maples](https://maple.example)\n\n- maple\n  - maplebush\n\n1. maple\n2. maplebush\n\n```rust\nfn main() {}\n```\n\n> maple quote\n\n---";
        assert_eq!(round_trip(md), md);
    }

    #[test]
    fn a_doc_renders_headings_paragraphs_and_nested_bullets() {
        let md = to_md(json!({
            "type": "doc",
            "content": [
                // no level attr at all: falls back to h1
                {"type": "heading", "content": [{"type": "text", "text": "Maple"}]},
                {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Maplesub"}]},
                {"type": "paragraph", "content": [
                    {"type": "text", "text": "maple "},
                    {"type": "mention"},
                    {"type": "hardBreak"},
                    {"type": "text", "text": "maplebye"}
                ]},
                {"type": "bulletList", "content": [
                    {"type": "listItem", "content": [
                        {"type": "paragraph", "content": [{"type": "text", "text": "maple"}]},
                        {"type": "bulletList", "content": [
                            {"type": "listItem", "content": [
                                {"type": "paragraph", "content": [{"type": "text", "text": "maplebush"}]}
                            ]}
                        ]}
                    ]}
                ]},
                // unknown block types pass their children through
                {"type": "panel", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "maplepassthrough"}]}
                ]}
            ]
        }));

        assert_eq!(
            md,
            "# Maple\n\n## Maplesub\n\nmaple [@unknown](mention:)\nmaplebye\n\n- maple\n  - maplebush\n\nmaplepassthrough"
        );
    }

    #[test]
    fn an_ordered_list_numbers_from_its_start_attr() {
        let md = to_md(json!({
            "type": "doc",
            "content": [{"type": "orderedList", "attrs": {"order": 3}, "content": [
                {"type": "listItem", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "Maple"}]}
                ]},
                {"type": "listItem", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "Mapleberry"}]},
                    {"type": "bulletList", "content": [
                        {"type": "listItem", "content": [
                            {"type": "paragraph", "content": [{"type": "text", "text": "Maplebush"}]}
                        ]}
                    ]}
                ]}
            ]}]
        }));

        assert_eq!(md, "3. Maple\n4. Mapleberry\n   - Maplebush");
    }

    #[test]
    fn inline_marks_become_markdown_delimiters() {
        let md = to_md(json!({
            "type": "doc",
            "content": [{"type": "paragraph", "content": [
                {"type": "text", "text": "Use "},
                {"type": "text", "text": "maple", "marks": [{"type": "em"}]},
                {"type": "text", "text": " with "},
                {"type": "text", "text": "maplelegacy", "marks": [{"type": "strike"}]},
                {"type": "text", "text": " "},
                {"type": "text", "text": "maple()", "marks": [{"type": "code"}]},
                {"type": "text", "text": ", see "},
                {"type": "text", "text": "maples", "marks": [{"type": "link", "attrs": {"href": "https://maple.example"}}]},
                {"type": "text", "text": " and ask "},
                {"type": "mention", "attrs": {"text": "@Maple", "id": "maple-123"}},
                {"type": "text", "text": " maplenow", "marks": [{"type": "strong"}]}
            ]}]
        }));

        assert_eq!(
            md,
            "Use *maple* with ~~maplelegacy~~ `maple()`, see [maples](https://maple.example) and ask [@Maple](mention:maple-123) **maplenow**"
        );
    }

    #[test]
    fn markdown_specials_in_plain_text_are_escaped() {
        let md = to_md(json!({
            "type": "doc",
            "content": [{"type": "paragraph", "content": [
                {"type": "text", "text": "maple_case <maple> 2*3"}
            ]}]
        }));

        assert_eq!(md, "maple\\_case \\<maple\\> 2\\*3");
    }

    #[test]
    fn a_code_block_keeps_its_language_and_newlines() {
        let md = to_md(json!({
            "type": "doc",
            "content": [{"type": "codeBlock", "attrs": {"language": "rust"}, "content": [
                {"type": "text", "text": "fn main() {\n    println!(\"maple\");\n}"}
            ]}]
        }));

        assert_eq!(md, "```rust\nfn main() {\n    println!(\"maple\");\n}\n```");
    }

    #[test]
    fn a_blockquote_prefixes_every_block_it_holds() {
        let md = to_md(json!({
            "type": "doc",
            "content": [
                {"type": "blockquote", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "Maple carefully."}]},
                    {"type": "paragraph", "content": [{"type": "text", "text": "Maplebush."}]}
                ]},
                {"type": "rule"}
            ]
        }));

        assert_eq!(md, "> Maple carefully.\n> \n> Maplebush.\n\n---");
    }

    #[test]
    fn an_empty_doc_renders_nothing() {
        assert_eq!(to_md(json!({"type": "doc", "content": []})), "");
    }
}
