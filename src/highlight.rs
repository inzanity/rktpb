use std::io;
use std::sync::LazyLock;

use syntect::highlighting::{Theme, ThemeSet};
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
    theme: Theme,
}

impl Highlighter {
    pub fn default() -> Option<Self> {
        let mut reader = io::Cursor::new(include_str!("../static/GitHub.tmtheme"));
        Some(Self {
            theme: ThemeSet::load_from_reader(&mut reader).ok()?,
        })
    }

    pub fn contains(ext: &str) -> bool {
        SYNTAXES.find_syntax_by_extension(ext).is_some()
    }

    pub fn highlight(&self, code: &str, lang: &str) -> Result<String> {
        let syntaxes = &*SYNTAXES;
        let syntax = syntaxes
            .find_syntax_by_token(lang)
            .ok_or("missing syntax")?;

        Ok(syntect::html::highlighted_html_for_string(
            code,
            syntaxes,
            syntax,
            &self.theme,
        )?)
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
