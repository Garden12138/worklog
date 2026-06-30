use crate::error::{AppError, AppResult};
use docx_rs::{Docx, Paragraph, Run};
use std::io::Cursor;

pub fn markdown_to_docx(markdown: &str) -> AppResult<Vec<u8>> {
    let mut document = Docx::new();
    for raw_line in markdown.lines() {
        let line = raw_line.trim();
        let mut paragraph = Paragraph::new();
        if line.is_empty() {
            document = document.add_paragraph(paragraph);
            continue;
        }
        if line.starts_with('#') {
            let level = line
                .chars()
                .take_while(|value| *value == '#')
                .count()
                .clamp(1, 4);
            let text = line[level..].trim();
            let style = match level {
                1 => "Heading1",
                2 => "Heading2",
                3 => "Heading3",
                _ => "Heading4",
            };
            paragraph = paragraph.style(style).add_run(Run::new().add_text(text));
        } else if line.starts_with("- ") || line.starts_with("* ") {
            paragraph = paragraph
                .style("ListBullet")
                .add_run(Run::new().add_text(line[2..].trim()));
        } else if let Some((number, text)) = line.split_once(". ") {
            if number.chars().all(|value| value.is_ascii_digit()) {
                paragraph = paragraph
                    .style("ListNumber")
                    .add_run(Run::new().add_text(text.trim()));
            } else {
                paragraph = paragraph.add_run(Run::new().add_text(line));
            }
        } else {
            paragraph = paragraph.add_run(Run::new().add_text(line));
        }
        document = document.add_paragraph(paragraph);
    }
    let mut cursor = Cursor::new(Vec::new());
    document
        .build()
        .pack(&mut cursor)
        .map_err(|error| AppError::new("docx_error", error.to_string()))?;
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_valid_docx_zip() {
        let bytes = markdown_to_docx("# 周报\n\n- 完成 Rust 改造").unwrap();
        assert!(bytes.starts_with(b"PK"));
    }
}
