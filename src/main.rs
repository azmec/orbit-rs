mod orbit;

use std::io::Write;
use std::path::Path;
use std::ffi::OsStr;
use std::error::Error;
use std::result;

use pulldown_cmark::{Parser, Event, Tag, CodeBlockKind, Options};
use walkdir::WalkDir;
use handlebars::Handlebars;
use regex::Regex;

use orbit::Orbit;

type Result<T> = result::Result<T, Box<dyn Error>>;

lazy_static::lazy_static! {
    static ref NORMAL_FOOTNOTE: Regex = Regex::new("\\[\\^(.*)\\]:(.*)$").unwrap();
}

const TEMPLATE: &str = include_str!("../template.html");
const CSS: &str = include_str!("../tufte.css");

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();  
    let src_dir_opt = args.get(1);
    let dest_dir_opt = args.get(2);

    if let (Some(src), Some(dest)) = (src_dir_opt, dest_dir_opt) {
        walk_markdown_directory(src, dest)?;
    }

    return Ok(())
}


fn walk_markdown_directory<P: AsRef<Path>>(source: P, destination: P) -> Result<()> {
    let walker = WalkDir::new(source).into_iter();
    for entry in walker.filter_entry(|e| !is_hidden(e)) {
        let entry = entry?;
        let filename = entry.file_name();
        let metadata = entry.metadata()?;

        if !metadata.is_dir() && is_markdown(filename) {
            let entry_path = entry.path();
            let markdown = std::fs::read_to_string(&entry_path)?;

            // I want to skip over the frontmatter. Because this is a small
            // project, I can assume the frontmatter will be four lines long,
            // excluding the `---` delimitters. So, skipping the first 6
            // newlines ('\n') is equivalent to skipping all frontmatter.
            let mut idx: usize = 0;
            let mut newline_no: u32 = 0;
            let markdown_bytes = markdown.as_bytes();
            while newline_no < 6 {
                if markdown_bytes[idx] == '\n' as u8 {
                    newline_no += 1;
                }

                idx += 1
            }

            let render = markdown_to_html(&markdown[idx..])?;
            let dest_path = destination.as_ref().join(filename).with_extension("html");
            let mut file = std::fs::File::create(&dest_path)?;
            write!(&mut file, "{}", render)?;
        }
    }

    let css_dest_path = destination.as_ref().join("tufte.css");
    let mut file = std::fs::File::create(&css_dest_path)?;
    write!(&mut file, "{}", CSS)?;

    return Ok(())
}

fn markdown_to_html(markdown: &str) -> Result<String> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_FOOTNOTES);

    let (content, footnotes) = split_content_and_footnotes(&markdown);

    let parser = Parser::new_ext(&content, options).into_offset_iter();
    let mut html_output = String::new();

    let mut in_orbit_block = false;
    let mut footnote_no: u32 = 0;

    let mut events = Vec::new();
    for event in parser {
        match event {
            (Event::FootnoteReference(name), _) => {
                footnote_no += 1;
                let footnote_html = format!("<sup class=\"fn\"><a id=\"{}-back\" href=\"#{}\">[{}]</a></sup>", name, name, footnote_no);
                events.push(Event::Html(footnote_html.into()));
            }
            (Event::Start(Tag::Link(foo, destination, bar)), _) => {
                let mut new_destination = destination.to_string();
                if destination.ends_with(".md") {
                    new_destination = destination.replace(".md", ".html");
                }

                events.push(Event::Start(Tag::Link(foo, new_destination.into(), bar)));
            }
            (Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(language))), range) => {
                if language.clone().into_string() == "orbit" {
                    let codeblock = &markdown[range.start..range.end];
                    let orbit: Orbit = deserialize_orbit_codeblock(codeblock)?;
                    let orbit_html = orbit.to_html()?;

                    in_orbit_block = true; 

                    events.push(Event::Html(orbit_html.into()));
                }
            },
            (Event::End(Tag::CodeBlock(CodeBlockKind::Fenced(language))), _) => {
                if language.clone().into_string() == "orbit" {
                    in_orbit_block = false;
                }
            }

            _ => {
                if !in_orbit_block { // Practically, skip over content in Orbit blocks
                    events.push(event.0);
                }
            }
        }
    }

    pulldown_cmark::html::push_html(&mut html_output, events.into_iter());
    let footnotes_html = fmt_footnotes_to_html(footnotes)?;
    html_output.push_str(&footnotes_html);

    let mut register = Handlebars::new();
    register.register_escape_fn(handlebars::no_escape);

    let body_map = &serde_json::json!({"body": html_output});
    let render = register.render_template(TEMPLATE, body_map)?;

    return Ok(render);
}

fn split_content_and_footnotes(markdown: &str) -> (String, Vec<String>) {
    let mut footnotes = Vec::new();
    let mut content = Vec::new();

    for line in markdown.lines() {
        if line.starts_with("[^") {
            footnotes.push(line.to_string());
        } else {
            content.push(line);
        }
    }

    return (content.join("\n"), footnotes);
}

fn fmt_footnotes_to_html(footnotes: Vec<String>) -> Result<String> {
    let mut markdown = String::from("---\n");
    for footnote in &footnotes {
        let captures = NORMAL_FOOTNOTE.captures(&footnote).unwrap();
        let formatted = format!("1. {} <a class=\"fn-back\" href=\"#{}-back\">↩</a>", &captures[2], &captures[1]);
        markdown.push_str(&formatted);
    }

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_FOOTNOTES);

    let mut footnote_no: usize = 0;
    let parser = Parser::new_ext(&markdown, options);
    let events = parser.map(|event| match event {
        Event::Start(Tag::Item) => {
            let capture = NORMAL_FOOTNOTE.captures(&footnotes[footnote_no]).unwrap();
            footnote_no += 1;

            Event::Html(format!("<li id=\"{}\">", &capture[1]).into())
        }
        Event::Start(Tag::Link(foo, destination, bar)) => {
            let mut new_destination = destination.to_string();
            if destination.ends_with(".md") {
                new_destination = destination.replace(".md", ".html");
            }

            Event::Start(Tag::Link(foo, new_destination.into(), bar))
        }

        _ => event,
    });

    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, events);

    Ok(html_output)
}

fn deserialize_orbit_codeblock(codeblock: &str) -> Result<Orbit> {
    let json = &codeblock[9..(codeblock.len() - 4)];
    let orbit: Orbit = serde_json::from_str(json)?;

    Ok(orbit)
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.file_name()
        .to_str()
        .map(|s| s.starts_with("."))
        .unwrap_or(false)
}

fn is_markdown(filename: &OsStr) -> bool {
    filename.to_string_lossy().ends_with(".md")
}
