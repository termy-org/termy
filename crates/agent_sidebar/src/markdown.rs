use gpui::{AnyElement, FontWeight, IntoElement, ParentElement, Styled, div, px};

#[derive(Debug)]
enum MarkdownBlock {
    Heading { level: usize, text: String },
    Paragraph(String),
    Bullet(String),
    Quote(String),
    Code(String),
}

pub fn render_markdown_message(
    content: &str,
    text_color: gpui::Rgba,
    heading_color: gpui::Rgba,
    border_color: gpui::Rgba,
    code_bg: gpui::Rgba,
) -> AnyElement {
    let blocks = parse_markdown_blocks(content);
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(5.0))
        .children(blocks.into_iter().map(|block| {
            match block {
                MarkdownBlock::Heading { level, text } => {
                    let size = match level {
                        1 => 13.0,
                        2 => 12.5,
                        _ => 12.0,
                    };
                    div()
                        .w_full()
                        .text_size(px(size))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(heading_color)
                        .child(text)
                        .into_any_element()
                }
                MarkdownBlock::Paragraph(text) => div()
                    .w_full()
                    .text_size(px(11.0))
                    .text_color(text_color)
                    .child(text)
                    .into_any_element(),
                MarkdownBlock::Bullet(text) => div()
                    .w_full()
                    .flex()
                    .items_start()
                    .gap(px(6.0))
                    .child(
                        div()
                            .pt(px(1.0))
                            .text_size(px(11.0))
                            .text_color(heading_color)
                            .child("•"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.0))
                            .text_color(text_color)
                            .child(text),
                    )
                    .into_any_element(),
                MarkdownBlock::Quote(text) => div()
                    .w_full()
                    .pl(px(8.0))
                    .border_l_1()
                    .border_color(border_color)
                    .text_size(px(11.0))
                    .text_color(text_color)
                    .child(text)
                    .into_any_element(),
                MarkdownBlock::Code(text) => div()
                    .w_full()
                    .px(px(8.0))
                    .py(px(6.0))
                    .border_1()
                    .border_color(border_color)
                    .bg(code_bg)
                    .text_size(px(10.5))
                    .text_color(text_color)
                    .child(text)
                    .into_any_element(),
            }
        }))
        .into_any_element()
}

fn parse_markdown_blocks(content: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut paragraph_lines = Vec::<String>::new();
    let mut in_code = false;
    let mut code_lines = Vec::<String>::new();

    let flush_paragraph = |blocks: &mut Vec<MarkdownBlock>, lines: &mut Vec<String>| {
        if lines.is_empty() {
            return;
        }
        let text = lines.join(" ");
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            blocks.push(MarkdownBlock::Paragraph(trimmed.to_string()));
        }
        lines.clear();
    };

    for raw_line in content.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            if in_code {
                let code = code_lines.join("\n");
                blocks.push(MarkdownBlock::Code(code));
                code_lines.clear();
                in_code = false;
            } else {
                in_code = true;
            }
            continue;
        }

        if in_code {
            code_lines.push(line.to_string());
            continue;
        }

        if trimmed.is_empty() {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            continue;
        }

        if let Some((level, text)) = heading_line(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            blocks.push(MarkdownBlock::Heading {
                level,
                text: text.to_string(),
            });
            continue;
        }

        if let Some(text) = bullet_line(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            blocks.push(MarkdownBlock::Bullet(text.to_string()));
            continue;
        }

        if let Some(text) = trimmed.strip_prefix("> ") {
            flush_paragraph(&mut blocks, &mut paragraph_lines);
            blocks.push(MarkdownBlock::Quote(text.to_string()));
            continue;
        }

        paragraph_lines.push(trimmed.to_string());
    }

    flush_paragraph(&mut blocks, &mut paragraph_lines);
    if in_code {
        blocks.push(MarkdownBlock::Code(code_lines.join("\n")));
    }

    if blocks.is_empty() {
        blocks.push(MarkdownBlock::Paragraph(String::new()));
    }

    blocks
}

fn heading_line(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = line[hashes..].trim_start();
    if rest.is_empty() {
        return None;
    }
    Some((hashes, rest))
}

fn bullet_line(line: &str) -> Option<&str> {
    if let Some(text) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        return Some(text);
    }

    let bytes = line.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0 && idx + 1 < bytes.len() && bytes[idx] == b'.' && bytes[idx + 1] == b' ' {
        return Some(&line[(idx + 2)..]);
    }

    None
}
