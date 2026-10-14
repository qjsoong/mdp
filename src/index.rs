//! An index of the links and images in a document.

use std::ops::Range;

use comrak::nodes::NodeValue;
use mdk_core::{Dialect, Source, with_parsed_options};

/// A link or image and the source range that activates it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRef {
    /// Source range of the whole `[text](url)` construct.
    pub range: Range<usize>,
    /// Destination URL.
    pub url: String,
    /// Whether the link is an image.
    pub image: bool,
}

/// A fenced or indented code block and its language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    /// Source range of the whole block, including its fence.
    pub range: Range<usize>,
    /// The language from the info string, if any.
    pub language: Option<String>,
}

/// Links, images and code blocks in reading order.
#[derive(Debug, Clone, Default)]
pub struct Index {
    links: Vec<LinkRef>,
    code: Vec<CodeBlock>,
}

impl Index {
    /// Collects every link and image in the document.
    #[must_use]
    pub fn build(markdown: &str, dialect: Dialect) -> Self {
        let source = Source::new(markdown.to_string());
        let options = dialect.comrak_options();
        with_parsed_options(&source, &options, |root| {
            let mut links = Vec::new();
            let mut code = Vec::new();
            for node in root.descendants() {
                let data = node.data();
                match &data.value {
                    NodeValue::Link(link) => {
                        if link.url.is_empty() {
                            continue;
                        }
                        links.push(LinkRef {
                            range: source.span_range(mdk_core::ast::node_span(node)),
                            url: link.url.clone(),
                            image: false,
                        });
                    }
                    NodeValue::Image(link) => {
                        if link.url.is_empty() {
                            continue;
                        }
                        links.push(LinkRef {
                            range: source.span_range(mdk_core::ast::node_span(node)),
                            url: link.url.clone(),
                            image: true,
                        });
                    }
                    NodeValue::CodeBlock(block) => {
                        let language = block
                            .info
                            .split(|ch: char| ch.is_whitespace() || ch == ',')
                            .next()
                            .filter(|token| !token.is_empty())
                            .map(str::to_string);
                        code.push(CodeBlock {
                            range: source.span_range(mdk_core::ast::node_span(node)),
                            language,
                        });
                    }
                    _ => {}
                }
            }
            Self { links, code }
        })
    }

    /// The language of the code block containing a source offset.
    #[must_use]
    pub fn code_language(&self, offset: usize) -> Option<&str> {
        self.code
            .iter()
            .filter(|block| block.range.contains(&offset))
            .min_by_key(|block| block.range.len())
            .and_then(|block| block.language.as_deref())
    }

    /// The innermost link containing a source offset.
    #[must_use]
    pub fn at(&self, offset: usize) -> Option<&LinkRef> {
        self.links
            .iter()
            .filter(|link| link.range.contains(&offset))
            .min_by_key(|link| link.range.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(markdown: &str) -> Index {
        Index::build(markdown, Dialect::Gfm)
    }

    #[test]
    fn finds_a_link_at_its_text_and_url() {
        let index = Index::build("[text](https://example.com)\n", Dialect::Gfm);
        let link = index.at(2).expect("link at text");
        assert_eq!(link.url, "https://example.com");
        assert!(!link.image);
        assert!(index.at(9).is_some());
        assert!(index.at(100).is_none());
    }

    #[test]
    fn finds_an_image_at_its_alt_text() {
        let index = Index::build("![alt text](image.png)\n", Dialect::Gfm);
        let link = index.at(4).expect("image at alt");
        assert_eq!(link.url, "image.png");
        assert!(link.image);
    }

    #[test]
    fn ignores_links_without_a_destination() {
        let index = index("[text]()\n");
        assert!(index.at(2).is_none());
    }

    #[test]
    fn finds_code_block_languages() {
        let index = index("text\n\n```rust,no_run\nlet x = 1;\n```\n\n    indented\n");
        let offset = 12;
        assert_eq!(index.code_language(offset), Some("rust"));
        let indented = "text\n\n```rust,no_run\nlet x = 1;\n```\n\n    indented\n"
            .find("indented")
            .expect("offset");
        assert_eq!(index.code_language(indented), None);
    }

    #[test]
    fn prefers_the_innermost_link() {
        let index = Index::build("[![alt](image.png)](https://example.com)\n", Dialect::Gfm);
        let image = index.at(3).expect("image inside the link");
        assert_eq!(image.url, "image.png");
        assert!(image.image);
        let link = index.at(25).expect("link destination");
        assert_eq!(link.url, "https://example.com");
        assert!(!link.image);
    }
}
