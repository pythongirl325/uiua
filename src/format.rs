//! Functions for formatting Uiua code.

use std::{env, fs, path::Path};

use crate::{
    ast::*, grid_fmt::GridFmt, lex::Sp, parse::parse, primitive::Primitive, UiuaError, UiuaResult,
};

#[derive(Debug, Clone)]
pub struct FormatConfig {
    /// Whether to add a trailing newline to the output.
    ///
    /// Default: `true`
    pub trailing_newline: bool,
    /// Whether to add a space after the `#` in comments.
    ///
    /// Default: `true`
    pub comment_space_after_hash: bool,
    /// The number of spaces to indent multiline arrays and functions
    ///
    /// Default: `2`
    pub multiline_indent: usize,
    /// Override multiline formatting to be always compact or always not.
    ///
    /// Default: `None`
    ///
    /// If `None`, then multiline arrays and functions that start on or before `multiline_compact_threshold` will be compact, and those that start after will not be.
    pub compact_multiline: Option<bool>,
    /// The number of characters on line preceding a multiline array or function, at or before which the multiline will be compact.
    ///
    /// Default: `10`
    pub multiline_compact_threshold: usize,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            trailing_newline: true,
            comment_space_after_hash: true,
            multiline_indent: 2,
            compact_multiline: None,
            multiline_compact_threshold: 10,
        }
    }
}

pub fn format_items(items: &[Item], config: &FormatConfig) -> String {
    let mut output = String::new();
    format_items_impl(&mut output, items, config);
    while output.ends_with('\n') {
        output.pop();
    }
    if config.trailing_newline {
        output.push('\n');
    }
    output
}

pub fn format<P: AsRef<Path>>(input: &str, path: P, config: &FormatConfig) -> UiuaResult<String> {
    format_impl(input, Some(path.as_ref()), config)
}
pub fn format_str(input: &str, config: &FormatConfig) -> UiuaResult<String> {
    format_impl(input, None, config)
}

fn format_impl(input: &str, path: Option<&Path>, config: &FormatConfig) -> UiuaResult<String> {
    let (items, errors) = parse(input, path);
    if errors.is_empty() {
        Ok(format_items(&items, config))
    } else {
        Err(errors.into())
    }
}

pub fn format_file<P: AsRef<Path>>(path: P, config: &FormatConfig) -> UiuaResult<String> {
    let path = path.as_ref();
    let input = fs::read_to_string(path).map_err(|e| UiuaError::Load(path.to_path_buf(), e))?;
    let formatted = format(&input, path, config)?;
    if formatted == input {
        return Ok(formatted);
    }
    let dont_write = env::var("UIUA_NO_FORMAT").is_ok_and(|val| val == "1");
    if !dont_write {
        fs::write(path, &formatted).map_err(|e| UiuaError::Format(path.to_path_buf(), e))?;
    }
    Ok(formatted)
}

fn format_items_impl(output: &mut String, items: &[Item], config: &FormatConfig) {
    for item in items {
        format_item(output, item, config);
        output.push('\n');
    }
}

fn format_item(output: &mut String, item: &Item, config: &FormatConfig) {
    match item {
        Item::Scoped { items, test } => {
            let delim = if *test { "~~~" } else { "---" };
            output.push_str(delim);
            output.push('\n');
            format_items_impl(output, items, config);
            output.push_str(delim);
        }
        Item::Words(w) => {
            format_words(output, w, config);
        }
        Item::Binding(binding) => {
            output.push_str(&binding.name.value.0);
            output.push_str(" ← ");
            format_words(output, &binding.words, config);
        }
        Item::Newlines => {}
    }
}

fn format_words(output: &mut String, words: &[Sp<Word>], config: &FormatConfig) {
    for word in trim_spaces(words) {
        format_word(output, word, config);
    }
}

fn format_word(output: &mut String, word: &Sp<Word>, config: &FormatConfig) {
    match &word.value {
        Word::Number(_, n) => {
            output.push_str(&n.grid_string());
        }
        Word::Char(c) => {
            let formatted = format!("{c:?}");
            let formatted = &formatted[1..formatted.len() - 1];
            output.push('@');
            output.push_str(formatted);
        }
        Word::String(s) => output.push_str(&format!("{:?}", s)),
        Word::FormatString(_) => output.push_str(word.span.as_str()),
        Word::MultilineString(lines) => {
            if lines.len() == 1 {
                output.push_str(lines[0].span.as_str());
                return;
            }
            let curr_line_pos = if output.ends_with('\n') {
                0
            } else {
                output.lines().last().unwrap_or_default().chars().count()
            };
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    output.push('\n');
                    for _ in 0..curr_line_pos {
                        output.push(' ');
                    }
                }
                output.push_str(line.span.as_str());
            }
        }
        Word::Ident(ident) => output.push_str(&ident.0),
        Word::Strand(items) => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    output.push('_');
                }
                format_word(output, item, config);
            }
            if items.len() == 1 {
                output.push('_');
            }
        }
        Word::Array(items) => {
            output.push('[');
            format_multiline_words(output, items, config, true);
            output.push(']');
        }
        Word::Func(func, bind) => {
            if *bind {
                output.push('\'');
                format_words(output, &func.lines[0], config);
            } else {
                output.push('(');
                format_multiline_words(output, &func.lines, config, false);
                output.push(')');
            }
        }
        Word::Primitive(prim) => output.push_str(&prim.to_string()),
        Word::Modified(m) => {
            // Special case for `anti noop` -> `load`
            if let (
                Primitive::Anti,
                [Sp {
                    value: Word::Primitive(Primitive::Noop),
                    ..
                }],
            ) = (m.modifier.value, &m.words[..])
            {
                output.push(Primitive::Load.names().unwrap().unicode.unwrap());
                return;
            }
            // Normal case
            output.push_str(&m.modifier.value.to_string());
            format_words(output, &m.words, config);
        }
        Word::Spaces => output.push(' '),
        Word::Comment(comment) => {
            output.push('#');
            if config.comment_space_after_hash {
                output.push(' ');
            }
            output.push_str(comment);
        }
    }
}

fn format_multiline_words(
    output: &mut String,
    lines: &[Vec<Sp<Word>>],
    config: &FormatConfig,
    allow_compact: bool,
) {
    if lines.len() == 1 && !lines[0].iter().any(|word| word_is_multiline(&word.value)) {
        format_words(output, &lines[0], config);
        return;
    }
    let curr_line = output.lines().last().unwrap_or_default();
    let curr_line_pos = if output.ends_with('\n') {
        0
    } else {
        curr_line.chars().count()
    };
    let compact = allow_compact
        && config.compact_multiline.unwrap_or_else(|| {
            curr_line_pos <= config.multiline_compact_threshold || curr_line.starts_with(' ')
        })
        && (lines.iter().flatten()).all(|word| !word_is_multiline(&word.value));
    let indent = if compact {
        curr_line_pos
    } else {
        config.multiline_indent
    };
    for (i, line) in lines.iter().enumerate() {
        if i > 0 || !compact {
            output.push('\n');
            if !line.is_empty() {
                for _ in 0..indent {
                    output.push(' ');
                }
            }
        }
        format_words(output, line, config);
    }
    if !compact {
        output.push('\n');
    }
}

fn trim_spaces(words: &[Sp<Word>]) -> &[Sp<Word>] {
    let mut start = 0;
    for word in words {
        if let Word::Spaces = word.value {
            start += 1;
        } else {
            break;
        }
    }
    let mut end = words.len();
    for word in words.iter().rev() {
        if let Word::Spaces = word.value {
            end -= 1;
        } else {
            break;
        }
    }
    if start >= end {
        return &[];
    }
    &words[start..end]
}

fn word_is_multiline(word: &Word) -> bool {
    match word {
        Word::Number(_, _) => false,
        Word::Char(_) => false,
        Word::String(_) => false,
        Word::FormatString(_) => false,
        Word::MultilineString(lines) => lines.len() > 1,
        Word::Ident(_) => false,
        Word::Strand(_) => false,
        Word::Array(items) => items.iter().any(|lines| {
            lines.len() > 1 || lines.iter().any(|word| word_is_multiline(&word.value))
        }),
        Word::Func(func, bind) => {
            !bind
                && (func.lines.len() > 1
                    || (func.lines.iter().flatten()).any(|word| word_is_multiline(&word.value)))
        }
        Word::Primitive(_) => false,
        Word::Modified(m) => m.words.iter().any(|word| word_is_multiline(&word.value)),
        Word::Comment(_) => false,
        Word::Spaces => false,
    }
}
