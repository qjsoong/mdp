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

/// Links and images in reading order.
#[derive(Debug, Clone, Default)]
pub struct Index {
    links: Vec<LinkRef>,
}

impl Index {
    /// Collects every link and image in the document.
    #[must_use]
    pub fn build(markdown: &str, dialect: Dialect) -> Self {
        let source = Source::new(markdown.to_string());
        let options = dialect.comrak_options();
        with_parsed_options(&source, &options, |root| {
            let mut links = Vec::new();
            for node in root.descendants() {
                let data = node.data();
                let (link, image) = match &data.value {
                    NodeValue::Link(link) => (link, false),
                    NodeValue::Image(link) => (link, true),
                    _ => continue,
                };
                if link.url.is_empty() {
                    continue;
                }
                links.push(LinkRef {
                    range: source.span_range(mdk_core::ast::node_span(node)),
                    url: link.url.clone(),
                    image,
                });
            }
            Self { links }
        })
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
