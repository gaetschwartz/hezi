use std::collections::HashMap;

use nu_protocol::{Record, Span, Value};

use crate::Color;

pub fn get_default_color() -> Color {
    if let Ok("1") = std::env::var("NO_COLOR").as_deref() {
        return Color::Never;
    }
    Color::Auto
}

pub fn get_styles() -> clap::builder::Styles {
    // check for NO_COLOR environment variable
    if let Ok("1") = std::env::var("NO_COLOR").as_deref() {
        return clap::builder::Styles::default();
    }

    clap::builder::Styles::styled()
        .usage(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow))),
        )
        .header(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Yellow))),
        )
        .literal(
            anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green))),
        )
        .invalid(
            anstyle::Style::new()
                .bold()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red))),
        )
        .error(
            anstyle::Style::new()
                .bold()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Red))),
        )
        .valid(
            anstyle::Style::new()
                .bold()
                .underline()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green))),
        )
        .placeholder(
            anstyle::Style::new().fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::White))),
        )
}

#[inline]
fn color<S: Into<String>>(s: S) -> Value {
    Value::string(s, Span::unknown())
}

#[inline]
fn ansi<S: Into<String>>(fg: S, bg: S, attrs: S) -> Value {
    Value::record(
        Record::from_iter(vec![
            ("fg".to_string(), Value::string(fg, Span::unknown())),
            ("bg".to_string(), Value::string(bg, Span::unknown())),
            ("attrs".to_string(), Value::string(attrs, Span::unknown())),
        ]),
        Span::unknown(),
    )
}

pub fn main_theme() -> HashMap<String, Value> {
    HashMap::from([
        ("separator".to_string(), color("white")),
        ("leading_trailing_space_bg".to_string(), color("#808080")),
        ("header".to_string(), color("green_bold")),
        ("empty".to_string(), color("blue")),
        ("bool".to_string(), color("light_cyan")),
        ("int".to_string(), color("white")),
        ("filesize".to_string(), color("cyan")),
        ("duration".to_string(), color("white")),
        ("date".to_string(), color("purple")),
        ("range".to_string(), color("white")),
        ("float".to_string(), color("light_blue")),
        ("string".to_string(), color("blue")),
        ("nothing".to_string(), color("white")),
        ("binary".to_string(), ansi("white", "", "i")),
        ("cell-path".to_string(), ansi("white", "", "u")),
        ("row_index".to_string(), color("green_bold")),
        ("record".to_string(), color("white")),
        ("list".to_string(), color("white")),
        ("block".to_string(), color("white")),
        ("hints".to_string(), color("dark_gray")),
        ("search_result".to_string(), ansi("white", "red", "")),
    ])
}

pub fn no_color_theme() -> HashMap<String, Value> {
    let mut main_theme = main_theme();

    for (_, v) in main_theme.iter_mut() {
        if let Value::String { val: s, .. } = v {
            *s = String::new();
        }
    }

    main_theme
}
