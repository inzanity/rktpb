use std::io;
use std::sync::LazyLock;

use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

use super::Result;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
pub static HIGHLIGHT_EXTS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let syntaxes = &*SYNTAXES;
    let mut exts: Vec<_> = syntaxes
        .syntaxes()
        .iter()
        .flat_map(|s| s.file_extensions.iter().cloned())
        .collect();
    exts.sort();
    exts
});

static OPTIONS: LazyLock<comrak::Options<'static>> = LazyLock::new(|| {
    comrak::Options {
        extension: comrak::options::Extension {
            strikethrough: true,
            tagfilter: true,
            table: true,
            autolink: true,
            tasklist: true,
            superscript: true,
            footnotes: true,
            front_matter_delimiter: Some("---".into()),
            header_ids: Some(String::new()),
            ..Default::default()
        },
        parse: comrak::options::Parse::default(),
        render: comrak::options::Render {
            github_pre_lang: true,
            // NB: we use CSP to ensure no JS leaks.
            r#unsafe: true,
            ..Default::default()
        },
    }
});

pub struct Highlighter {
    style: String,
}

impl Highlighter {
    pub fn default() -> Option<Self> {
        use syntect::html::{ClassStyle, css_for_theme_with_class_style};

        let mut reader = io::Cursor::new(include_str!("../static/GitHub.tmtheme"));
        let theme = ThemeSet::load_from_reader(&mut reader).ok()?;
        let style = css_for_theme_with_class_style(&theme, ClassStyle::Spaced).ok()?;

        Some(Self { style })
    }

    pub fn contains(ext: &str) -> bool {
        SYNTAXES.find_syntax_by_extension(ext).is_some()
    }

    pub fn style(&self) -> &str {
        &self.style
    }

    pub fn highlight(&self, code: &str, lang: &str) -> Result<String> {
        use syntect::html::{ClassStyle, ClassedHTMLGenerator};
        use syntect::util::LinesWithEndings;
        let syntaxes = &*SYNTAXES;
        let syntax = syntaxes
            .find_syntax_by_token(lang)
            .ok_or("missing syntax")?;

        let mut generator =
            ClassedHTMLGenerator::new_with_class_style(syntax, syntaxes, ClassStyle::Spaced);
        for line in LinesWithEndings::from(code) {
            generator.parse_html_for_line_which_includes_newline(line)?;
        }
        Ok(generator.finalize())
    }

    pub fn render_markdown(&self, markdown: &str) -> Result<String> {
        let arena = comrak::Arena::new();
        let ast = comrak::parse_document(&arena, markdown, &OPTIONS);
        self.highlight_ast(ast);

        let mut html = String::new();
        comrak::format_html(ast, &OPTIONS, &mut html)?;
        Ok(html)
    }

    fn highlight_ast<'a>(&self, ast: &'a comrak::nodes::AstNode<'a>) {
        use comrak::arena_tree::NodeEdge;
        use comrak::nodes::{NodeHtmlBlock, NodeValue};

        for node in ast.traverse() {
            if let NodeEdge::Start(node) = node {
                let mut data = node.data.borrow_mut();
                if let NodeValue::CodeBlock(ref mut block) = data.value
                    && let Ok(highlighted) = self.highlight(&block.literal, &block.info)
                {
                    data.value = NodeValue::HtmlBlock(NodeHtmlBlock {
                        literal: highlighted,
                        ..Default::default()
                    });
                }
            }
        }
    }
}
